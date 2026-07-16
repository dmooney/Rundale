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
		page.getByRole('button', { name: 'Ask action stamp' }),
	).toHaveCount(1);
	const input = page.getByLabel('Player intent');
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
	const input = page.getByLabel('Player intent');
	const stableBox = await notebookBox(page);

	await input.focus();
	await expect(input).toHaveAttribute('data-command-state', 'focused');
	await settleNotebookFrame(page);
	await page.screenshot({
		path: path.join(PROOF_DIR, `${viewport}-focused.png`),
		fullPage: false,
	});

	await emitEvent(page, 'loading', { active: true, phrase: 'Listening...' });
	await expect(input).toHaveAttribute('data-command-state', 'busy');
	await expect(input).toHaveAttribute('aria-disabled', 'true');
	await expect(input).not.toHaveAttribute('disabled', '');
	await expect(page.locator('#notebook-command-status')).toContainText(
		'Parish reply in progress',
	);
	await settleNotebookFrame(page);
	await expectNotebookNative(page);
	expect(await notebookBox(page)).toEqual(stableBox);
	await page.screenshot({
		path: path.join(PROOF_DIR, `${viewport}-busy.png`),
		fullPage: false,
	});

	await emitEvent(page, 'loading', { active: false });
	await expect(input).toHaveAttribute('data-command-state', 'focused');
	await installControlledSubmitFailure(page);
	await input.fill('ask Roisin what she saw');
	await input.press('Enter');
	await expect(input).toHaveAttribute('data-command-state', 'disabled');
	await expect(input).toHaveValue('ask Roisin what she saw');
	await expect(page.locator('#notebook-command-status')).toContainText(
		'Sending your line',
	);
	await settleNotebookFrame(page);
	expect(await notebookBox(page)).toEqual(stableBox);
	await page.screenshot({
		path: path.join(PROOF_DIR, `${viewport}-disabled.png`),
		fullPage: false,
	});

	await rejectControlledSubmit(page);
	await expect(input).toHaveAttribute('data-command-state', 'error');
	await expect(input).toHaveAttribute('aria-invalid', 'true');
	await expect(input).toHaveValue('ask Roisin what she saw');
	await expect(page.locator('#notebook-command-status')).toContainText(
		'Ink blotted — Could not send input: bridge unavailable',
	);
	await settleNotebookFrame(page);
	await expectNotebookNative(page);
	expect(await notebookBox(page)).toEqual(stableBox);
	await page.screenshot({
		path: path.join(PROOF_DIR, `${viewport}-error.png`),
		fullPage: false,
	});
}

test.describe('illustrated notebook interactions', () => {
	test.beforeAll(() => {
		fs.mkdirSync(PROOF_DIR, { recursive: true });
	});

	test('desktop Pixi hit targets and keyboard routing stay notebook-native', async ({
		page,
	}) => {
		await page.setViewportSize({ width: 1440, height: 900 });
		await setupNotebookPage(page);

		await proveCommandVisualStates(page, 'desktop');
		const input = page.getByLabel('Player intent');
		await input.fill('');

		await page.getByRole('button', { name: 'Open time details' }).focus();
		await page.keyboard.press('Enter');
		await expect(page.getByLabel('time drawer')).toBeVisible();
		await expect(page.getByText('Clock')).toBeVisible();

		await page.getByRole('button', { name: 'Open parish map' }).focus();
		await page.keyboard.press('Enter');
		await expect(page.locator('[data-testid="full-map"]')).toBeVisible();
	});

	test('mobile viewport keeps notebook controls and old chrome absent', async ({
		page,
	}) => {
		await page.setViewportSize({ width: 390, height: 844 });
		await setupNotebookPage(page);

		await proveCommandVisualStates(page, 'mobile');
	});
});
