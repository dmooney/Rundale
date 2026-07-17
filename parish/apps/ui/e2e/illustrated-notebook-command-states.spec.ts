import {
	emitEvent,
	expect,
	installTauriMock,
	test,
	waitForTextureCompleteNotebookFrame,
} from './fixtures';
import * as fs from 'fs';
import * as path from 'path';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const PROOF_DIR = path.resolve(__dirname, '../../../../.proofs/1636');
// Setup plus the five visual states each wait up to 10 seconds for a
// texture-complete WebGL frame. The ordinary 60-second test timeout therefore
// cannot cover the proof's own bounded waits under a loaded full-suite run.
const VISUAL_PROOF_TIMEOUT_MS = 120_000;

type NotebookBox = NonNullable<
	Awaited<ReturnType<import('@playwright/test').Locator['boundingBox']>>
>;

async function settleNotebookFrame(page: import('@playwright/test').Page) {
	await page.evaluate(
		() =>
			new Promise<void>((resolve) => {
				requestAnimationFrame(() => requestAnimationFrame(() => resolve()));
			}),
	);
	await waitForTextureCompleteNotebookFrame(page);
}

async function notebookBox(
	page: import('@playwright/test').Page,
): Promise<NotebookBox> {
	const box = await page.getByTestId('illustrated-notebook-game').boundingBox();
	expect(box).not.toBeNull();
	if (!box) throw new Error('notebook game must have a layout box');
	return box;
}

async function expectNotebookNative(page: import('@playwright/test').Page) {
	for (const selector of [
		'.input-wrapper',
		'.input-form',
		'[data-testid="input-field"]',
		'[data-testid="chat-panel"]',
	]) {
		await expect(page.locator(selector)).toHaveCount(0);
	}
	await expect(page.getByRole('button', { name: /^send$/i })).toHaveCount(0);
}

async function installControlledSubmitFailure(
	page: import('@playwright/test').Page,
) {
	await page.evaluate(() => {
		type Invoke = (
			command: string,
			args?: Record<string, unknown>,
		) => Promise<unknown>;
		const globals = window as unknown as Record<string, unknown>;
		const internals = globals.__TAURI_INTERNALS__ as { invoke: Invoke };
		const originalInvoke = internals.invoke.bind(internals);
		const control: { reject: (reason?: unknown) => void } = {
			reject: () => {},
		};
		globals.__TEST_REJECT_NOTEBOOK_SUBMIT__ = () =>
			control.reject(new Error('bridge unavailable'));
		internals.invoke = (command, args) => {
			if (command !== 'submit_input') return originalInvoke(command, args);
			return new Promise<unknown>((_resolve, reject) => {
				control.reject = reject;
			});
		};
	});
}

async function rejectControlledSubmit(page: import('@playwright/test').Page) {
	await page.evaluate(() => {
		const reject = (
			window as unknown as Record<string, (() => void) | undefined>
		).__TEST_REJECT_NOTEBOOK_SUBMIT__;
		if (!reject)
			throw new Error('controlled submit rejection was not installed');
		reject();
	});
}

async function setupNotebookPage(page: import('@playwright/test').Page) {
	await installTauriMock(page, 'morning');
	await page.goto('/', { waitUntil: 'domcontentloaded' });
	await expect(
		page.locator('[data-testid="illustrated-notebook-game"]'),
	).toBeVisible();
	await expect(
		page.locator('[data-testid="illustrated-notebook-pixi-host"] canvas'),
	).toBeVisible();
	await expect(page.locator('.input-wrapper')).toHaveCount(0);
	await expect(page.locator('.input-form')).toHaveCount(0);
	await expect(page.locator('[data-testid="chat-panel"]')).toHaveCount(0);
	await expect(
		page.getByRole('button', { name: 'Ask action', exact: true }),
	).toHaveCount(1);
	const input = page.getByRole('textbox', {
		name: 'Player intent',
		exact: true,
	});
	await expect(input).toHaveCSS('opacity', '0');
	await expect(input).toHaveCSS('pointer-events', 'none');
	await expect(input).toHaveCSS('width', '1px');
	await waitForTextureCompleteNotebookFrame(page);
	await expectNotebookNative(page);
}

async function proveCommandVisualStates(
	page: import('@playwright/test').Page,
	viewport: 'desktop' | 'mobile',
) {
	const input = page.getByRole('textbox', {
		name: 'Player intent',
		exact: true,
	});
	const status = page.locator('#notebook-command-status');
	const askStamp = page.getByRole('button', {
		name: 'Ask action',
		exact: true,
	});
	const stableBox = await notebookBox(page);

	await input.fill('');
	await input.focus();
	await expect(input).toHaveAttribute('data-command-state', 'focused');
	await expect(input).not.toHaveAttribute('aria-disabled');
	await expect(input).toHaveAttribute('aria-busy', 'false');
	await settleNotebookFrame(page);
	await page.screenshot({
		path: path.join(PROOF_DIR, `${viewport}-focused.png`),
		fullPage: false,
	});

	const longCommand =
		'ask Roisin to recount every detail because the latest typing stays visible';
	await input.fill(longCommand);
	await expect(input).toHaveAttribute('data-command-state', 'typing');
	await expect(input).toHaveValue(longCommand);
	await settleNotebookFrame(page);
	await page.screenshot({
		path: path.join(PROOF_DIR, `${viewport}-long-command.png`),
		fullPage: false,
	});
	await input.fill('');
	await expect(input).toHaveAttribute('data-command-state', 'focused');

	await emitEvent(page, 'loading', { active: true, phrase: 'Listening...' });
	await expect(input).toHaveAttribute('data-command-state', 'busy');
	await expect(input).not.toHaveAttribute('aria-disabled');
	await expect(input).toHaveAttribute('aria-busy', 'true');
	await expect(input).not.toHaveAttribute('disabled', '');
	await expect(input).not.toHaveAttribute('readonly', '');
	await expect(input).toBeEditable();
	await expect(status).toHaveAttribute('role', 'status');
	await expect(status).not.toHaveAttribute('aria-live');
	await expect(status).toContainText('Parish reply in progress');
	await expect(askStamp).toBeEnabled();
	await settleNotebookFrame(page);
	await expectNotebookNative(page);
	expect(await notebookBox(page)).toEqual(stableBox);
	await page.screenshot({
		path: path.join(PROOF_DIR, `${viewport}-busy.png`),
		fullPage: false,
	});

	await emitEvent(page, 'loading', { active: false });
	await expect(input).toHaveAttribute('data-command-state', 'focused');
	await expect(input).not.toHaveAttribute('aria-disabled');
	await expect(input).toHaveAttribute('aria-busy', 'false');
	await installControlledSubmitFailure(page);
	await input.fill('ask Roisin what she saw');
	await input.press('Enter');
	await expect(input).toHaveAttribute('data-command-state', 'disabled');
	await expect(input).toHaveAttribute('aria-disabled', 'true');
	await expect(input).toHaveAttribute('aria-busy', 'true');
	await expect(input).toHaveAttribute('readonly', '');
	await expect(input).not.toBeEditable();
	await expect(input).toHaveValue('ask Roisin what she saw');
	await input.press('x');
	await expect(input).toHaveValue('ask Roisin what she saw');
	await expect(askStamp).toBeDisabled();
	const askStampBox = await askStamp.boundingBox();
	expect(askStampBox).not.toBeNull();
	if (!askStampBox) throw new Error('ask stamp must have a layout box');
	await page.mouse.click(
		askStampBox.x + askStampBox.width / 2,
		askStampBox.y + askStampBox.height / 2,
	);
	await expect(input).toHaveValue('ask Roisin what she saw');
	await askStamp.dispatchEvent('click');
	await expect(input).toHaveValue('ask Roisin what she saw');
	await expect(status).toHaveAttribute('role', 'status');
	await expect(status).not.toHaveAttribute('aria-live');
	await expect(status).toContainText('Sending your line');
	await settleNotebookFrame(page);
	expect(await notebookBox(page)).toEqual(stableBox);
	await page.screenshot({
		path: path.join(PROOF_DIR, `${viewport}-disabled.png`),
		fullPage: false,
	});

	await rejectControlledSubmit(page);
	await expect(input).toHaveAttribute('data-command-state', 'error');
	await expect(input).not.toHaveAttribute('aria-disabled');
	await expect(input).toHaveAttribute('aria-busy', 'false');
	await expect(input).toHaveAttribute('aria-invalid', 'true');
	await expect(input).toHaveValue('ask Roisin what she saw');
	await expect(status).toHaveAttribute('role', 'alert');
	await expect(status).not.toHaveAttribute('aria-live');
	await expect(status).toContainText(
		'Ink blotted — Could not send input: bridge unavailable',
	);
	await expect(askStamp).toBeEnabled();
	await settleNotebookFrame(page);
	await expectNotebookNative(page);
	expect(await notebookBox(page)).toEqual(stableBox);
	await page.screenshot({
		path: path.join(PROOF_DIR, `${viewport}-error.png`),
		fullPage: false,
	});
}

test.describe('illustrated notebook command states', () => {
	test.describe.configure({ timeout: VISUAL_PROOF_TIMEOUT_MS });

	test.beforeAll(() => {
		fs.mkdirSync(PROOF_DIR, { recursive: true });
	});

	test('desktop command states stay notebook-native', async ({ page }) => {
		await page.setViewportSize({ width: 1440, height: 900 });
		await setupNotebookPage(page);

		await proveCommandVisualStates(page, 'desktop');
	});

	test('mobile command states stay notebook-native', async ({ page }) => {
		await page.setViewportSize({ width: 390, height: 844 });
		await setupNotebookPage(page);

		await proveCommandVisualStates(page, 'mobile');
	});
});
