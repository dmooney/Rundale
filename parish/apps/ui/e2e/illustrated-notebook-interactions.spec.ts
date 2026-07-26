import { emitEvent, expect, installTauriMock, test } from './fixtures';
import { SNAPSHOTS } from './mock-data';
import type { Locator, Page } from '@playwright/test';
import * as fs from 'fs';
import * as path from 'path';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const PROOF_DIR = path.resolve(
	__dirname,
	'../../../../.proofs/notebook-pixi-interactions',
);
const ASSIGNED_TASK = {
	id: 21,
	description: 'Help with the potato patch',
	assigned_by: 4,
	location_id: 9,
	status: 'assigned' as const,
	assigned_at: '1820-04-01T15:30:00Z',
	started_at: null,
	completed_at: null,
	last_matching_action: null,
};

function boxesOverlap(
	a: { x: number; y: number; width: number; height: number },
	b: { x: number; y: number; width: number; height: number },
): boolean {
	return (
		a.x < b.x + b.width &&
		a.x + a.width > b.x &&
		a.y < b.y + b.height &&
		a.y + a.height > b.y
	);
}

async function measuredBoxes(locator: Locator) {
	const count = await locator.count();
	const boxes = [];
	for (let index = 0; index < count; index += 1) {
		const box = await locator.nth(index).boundingBox();
		if (!box) throw new Error(`hit target ${index} was not measurable`);
		boxes.push(box);
	}
	return boxes;
}

function boxesAreBoundedAndDisjoint(
	boxes: Array<{ x: number; y: number; width: number; height: number }>,
	width: number,
	height: number,
): boolean {
	for (const box of boxes) {
		if (
			box.x < 0 ||
			box.y < 0 ||
			box.x + box.width > width ||
			box.y + box.height > height
		) {
			return false;
		}
	}
	for (let left = 0; left < boxes.length; left += 1) {
		for (let right = left + 1; right < boxes.length; right += 1) {
			if (boxesOverlap(boxes[left], boxes[right])) return false;
		}
	}
	return true;
}

async function setupNotebookPage(page: Page) {
	await installTauriMock(page, 'morning');
	await page.goto('/');
	await page.waitForLoadState('networkidle');
	await expect(
		page.locator('[data-testid="illustrated-notebook-game"]'),
	).toBeVisible();
	await expect(
		page.locator('[data-testid="illustrated-notebook-pixi-host"] canvas'),
	).toBeVisible();
	const host = page.getByTestId('illustrated-notebook-pixi-host');
	await expect(host).toHaveAttribute('data-scene-mode', 'neutral');
	await expect(host).toHaveAttribute('data-scene-plate', '');
	await expect(page.locator('.input-wrapper')).toHaveCount(0);
	await expect(page.locator('.input-form')).toHaveCount(0);
	await expect(page.locator('[data-testid="chat-panel"]')).toHaveCount(0);
	await expect(
		page.getByRole('button', { name: 'Ask action stamp' }),
	).toHaveCount(1);
}

test.describe('illustrated notebook interactions', () => {
	test.beforeAll(() => {
		fs.mkdirSync(PROOF_DIR, { recursive: true });
	});

	test('asset preload failure keeps a neutral interactive renderer available', async ({
		page,
	}) => {
		await page.route('**/rundale/notebook-ui/**', (route) =>
			route.fulfill({
				status: 503,
				contentType: 'text/plain',
				body: 'asset unavailable',
			}),
		);
		await installTauriMock(page, 'morning');
		await page.goto('/');
		const notebook = page.getByTestId('illustrated-notebook-game');
		const host = page.getByTestId('illustrated-notebook-pixi-host');

		await expect(notebook).toBeVisible();
		await expect(host.locator('canvas')).toBeVisible();
		await expect(host).toHaveAttribute('data-asset-mode', 'degraded');
		await expect(host).toHaveAttribute('data-scene-mode', 'neutral');
		await expect(host).toHaveAttribute('data-scene-plate', '');
		await expect(
			page.getByRole('button', { name: /^Select marker for / }),
		).toHaveCount(0);
		await expect(
			page.getByRole('button', { name: 'Ask action stamp' }),
		).toHaveCount(1);
	});

	test('desktop Pixi hit targets and keyboard routing stay notebook-native', async ({
		page,
	}) => {
		await page.setViewportSize({ width: 1440, height: 900 });
		await setupNotebookPage(page);

		const input = page.getByLabel('Player intent');

		await page.mouse.click(630, 758);
		await expect(input).toHaveValue(/ask /);
		await expect(input).toBeFocused();

		await page.getByRole('button', { name: 'Open time details' }).focus();
		await page.keyboard.press('Enter');
		await expect(page.getByLabel('time drawer')).toBeVisible();
		await expect(page.getByText('Clock')).toBeVisible();

		await page.getByRole('button', { name: 'Open parish map' }).focus();
		await page.keyboard.press('Enter');
		await expect(page.locator('[data-testid="full-map"]')).toBeVisible();

		await page.screenshot({
			path: path.join(PROOF_DIR, 'desktop.png'),
			fullPage: false,
		});
	});

	test('mobile viewport keeps notebook controls and old chrome absent', async ({
		page,
	}) => {
		await page.setViewportSize({ width: 390, height: 844 });
		await setupNotebookPage(page);

		await page.getByRole('button', { name: 'Ask action stamp' }).focus();
		await page.keyboard.press('Enter');
		await expect(page.getByLabel('Player intent')).toHaveValue(/ask /);

		await emitEvent(page, 'world-update', {
			...SNAPSHOTS.morning,
			active_tasks: [ASSIGNED_TASK],
		});
		const activeTaskControl = page.getByRole('button', {
			name: 'Open active tasks',
		});
		const actionControl = page.getByRole('button', {
			name: 'Ask action stamp',
		});
		const activeTaskBox = await activeTaskControl.boundingBox();
		const actionBox = await actionControl.boundingBox();
		expect(activeTaskBox).not.toBeNull();
		expect(actionBox).not.toBeNull();
		expect(activeTaskBox?.x).toBeGreaterThanOrEqual(0);
		expect(
			(activeTaskBox?.x ?? 390) + (activeTaskBox?.width ?? 1),
		).toBeLessThanOrEqual(390);
		expect(
			activeTaskBox && actionBox
				? boxesOverlap(activeTaskBox, actionBox)
				: true,
		).toBe(false);

		await page.screenshot({
			path: path.join(PROOF_DIR, 'mobile-active-task.png'),
			fullPage: false,
		});

		await activeTaskControl.focus();
		await page.keyboard.press('Enter');
		const drawer = page.getByLabel('active tasks drawer');
		await expect(drawer).toBeVisible();
		await expect(drawer.getByText(ASSIGNED_TASK.description)).toBeVisible();
		await expect(drawer.getByText('Assigned')).toBeVisible();

		await emitEvent(page, 'world-update', {
			...SNAPSHOTS.morning,
			active_tasks: [
				{
					...ASSIGNED_TASK,
					status: 'in_progress',
					started_at: '1820-04-01T15:40:00Z',
					last_matching_action: 'I set to work in the potato patch.',
				},
			],
		});
		await expect(drawer.getByText('In progress')).toBeVisible();

		await page.getByRole('button', { name: 'Close notebook drawer' }).click();
		await page.setViewportSize({ width: 320, height: 568 });
		await expect
			.poll(async () => {
				const compactTaskBox = await activeTaskControl.boundingBox();
				const compactActionBox = await actionControl.boundingBox();
				return Boolean(
					compactTaskBox &&
					compactActionBox &&
					compactTaskBox.x >= 0 &&
					compactTaskBox.x + compactTaskBox.width <= 320 &&
					compactTaskBox.y >= 0 &&
					compactTaskBox.y + compactTaskBox.height <= 568 &&
					!boxesOverlap(compactTaskBox, compactActionBox),
				);
			})
			.toBe(true);
		await page.screenshot({
			path: path.join(PROOF_DIR, 'mobile-short-active-task.png'),
			fullPage: false,
		});
	});

	test('constrained boundary layout keeps task, actions, and intent independently clickable', async ({
		page,
	}) => {
		await page.setViewportSize({ width: 760, height: 600 });
		await setupNotebookPage(page);

		const activeTask = page.getByRole('button', {
			name: 'Open active tasks',
		});
		const intent = page.getByRole('button', {
			name: 'Focus handwritten intent line',
		});
		const ask = page.getByRole('button', { name: 'Ask action stamp' });
		const [activeBox, intentBox, askBox] = await Promise.all([
			activeTask.boundingBox(),
			intent.boundingBox(),
			ask.boundingBox(),
		]);
		expect(activeBox).not.toBeNull();
		expect(intentBox).not.toBeNull();
		expect(askBox).not.toBeNull();
		expect(
			activeBox && intentBox ? boxesOverlap(activeBox, intentBox) : true,
		).toBe(false);
		expect(activeBox && askBox ? boxesOverlap(activeBox, askBox) : true).toBe(
			false,
		);
		expect(askBox && intentBox ? boxesOverlap(askBox, intentBox) : true).toBe(
			false,
		);

		if (!activeBox || !intentBox || !askBox) {
			throw new Error('notebook hit-target geometry was not measurable');
		}
		await page.mouse.click(
			activeBox.x + activeBox.width / 2,
			activeBox.y + activeBox.height / 2,
		);
		await expect(page.getByLabel('active tasks drawer')).toBeVisible();
		await page.getByRole('button', { name: 'Close notebook drawer' }).click();
		await page.mouse.click(
			intentBox.x + intentBox.width / 2,
			intentBox.y + intentBox.height / 2,
		);
		await expect(page.getByLabel('Player intent')).toBeFocused();
		await page.mouse.click(
			askBox.x + askBox.width / 2,
			askBox.y + askBox.height / 2,
		);
		await expect(page.getByLabel('Player intent')).toHaveValue(/ask /);
	});

	test('actual NPC portrait and authored-scene marker targets stay disjoint and clickable', async ({
		page,
	}) => {
		await page.setViewportSize({ width: 1200, height: 800 });
		await setupNotebookPage(page);
		await emitEvent(page, 'world-update', {
			...SNAPSHOTS.morning,
			location_id: 1,
			location_name: 'The Crossroads',
			location_description: 'Four roads meet beside the bramble wall.',
		});
		const host = page.getByTestId('illustrated-notebook-pixi-host');
		await expect(host).toHaveAttribute('data-scene-mode', 'authored');

		const portraits = page.getByRole('button', {
			name: /^Select nearby person /,
		});
		const markers = page.getByRole('button', {
			name: /^Select marker for /,
		});
		await expect(portraits).toHaveCount(2);
		await expect(markers).toHaveCount(2);

		for (const [width, height] of [
			[1200, 800],
			[1440, 900],
			[760, 600],
			[667, 375],
		] as const) {
			await page.setViewportSize({ width, height });
			await expect
				.poll(async () => {
					const [portraitBoxes, markerBoxes] = await Promise.all([
						measuredBoxes(portraits),
						measuredBoxes(markers),
					]);
					const portraitGeometryOk = boxesAreBoundedAndDisjoint(
						portraitBoxes,
						width,
						height,
					);
					const markerGeometryOk = boxesAreBoundedAndDisjoint(
						markerBoxes,
						width,
						height,
					);
					return portraitGeometryOk && markerGeometryOk
						? 'ok'
						: JSON.stringify({
								width,
								height,
								portraitGeometryOk,
								markerGeometryOk,
								portraitBoxes,
								markerBoxes,
							});
				})
				.toBe('ok');
		}

		await page.setViewportSize({ width: 1200, height: 800 });
		await expect
			.poll(async () =>
				boxesAreBoundedAndDisjoint(await measuredBoxes(markers), 1200, 800),
			)
			.toBe(true);
		const markerBoxes = await measuredBoxes(markers);
		await page.mouse.click(
			markerBoxes[1].x + markerBoxes[1].width / 2,
			markerBoxes[1].y + markerBoxes[1].height / 2,
		);
		await expect(host).toHaveAttribute(
			'data-selected-real-name',
			'Aoife Ní Cheallaigh',
		);

		const portraitBoxes = await measuredBoxes(portraits);
		await page.mouse.click(
			portraitBoxes[0].x + portraitBoxes[0].width / 2,
			portraitBoxes[0].y + portraitBoxes[0].height / 2,
		);
		await expect(host).toHaveAttribute(
			'data-selected-real-name',
			'Séamas Ó Briain',
		);
	});

	test('active tasks follow canonical world updates and never mirror an unsent draft', async ({
		page,
	}) => {
		await page.setViewportSize({ width: 1440, height: 900 });
		await setupNotebookPage(page);

		const input = page.getByLabel('Player intent');
		await page.getByRole('button', { name: 'Ask action stamp' }).focus();
		await page.keyboard.press('Enter');
		await expect(input).toHaveValue(/ask /);
		const unsentDraft = await input.inputValue();

		await page.getByRole('button', { name: 'Open active tasks' }).focus();
		await page.keyboard.press('Enter');
		const drawer = page.getByLabel('active tasks drawer');
		await expect(drawer).toBeVisible();
		await expect(drawer.getByText('No active task.')).toBeVisible();
		await expect(drawer.getByText(unsentDraft, { exact: true })).toHaveCount(0);

		await emitEvent(page, 'world-update', {
			...SNAPSHOTS.morning,
			active_tasks: [ASSIGNED_TASK],
		});
		await expect(drawer.getByText(ASSIGNED_TASK.description)).toBeVisible();
		await expect(drawer.getByText('Assigned')).toBeVisible();
		await expect(input).toHaveValue(unsentDraft);
		await expect(drawer.getByText(unsentDraft, { exact: true })).toHaveCount(0);

		await emitEvent(page, 'world-update', {
			...SNAPSHOTS.morning,
			active_tasks: [
				{
					...ASSIGNED_TASK,
					status: 'in_progress',
					started_at: '1820-04-01T15:40:00Z',
					last_matching_action: 'I set to work in the potato patch.',
				},
			],
		});
		await expect(drawer.getByText('In progress')).toBeVisible();
		await expect(input).toHaveValue(unsentDraft);
		await expect(drawer.getByText(unsentDraft, { exact: true })).toHaveCount(0);
	});

	test('Pixi keeps the latest command visible through long responses on desktop and mobile', async ({
		page,
	}) => {
		await page.setViewportSize({ width: 1440, height: 900 });
		await setupNotebookPage(page);
		const host = page.getByTestId('illustrated-notebook-pixi-host');

		await emitEvent(page, 'text-log', {
			id: 'latest-player-command',
			source: 'player',
			subtype: 'command',
			content: '/status',
		});
		for (let index = 1; index <= 6; index += 1) {
			await emitEvent(page, 'text-log', {
				id: `long-response-${index}`,
				source: 'system',
				content: `Status response line ${index}.`,
			});
		}

		await expect(host).toHaveAttribute(
			'data-visible-live-line-keys',
			/latest-player-command/,
		);
		await page.setViewportSize({ width: 320, height: 568 });
		await expect(host).toHaveAttribute(
			'data-visible-live-line-keys',
			/latest-player-command/,
		);
	});

	test('compact landscape pairs the current command with newest authoritative output', async ({
		page,
	}) => {
		await page.setViewportSize({ width: 667, height: 375 });
		await setupNotebookPage(page);
		const host = page.getByTestId('illustrated-notebook-pixi-host');

		await emitEvent(page, 'text-log', {
			id: 'compact-command',
			source: 'player',
			subtype: 'command',
			content: 'look toward the road',
		});
		for (const output of [
			{
				id: 'compact-system',
				source: 'system',
				content: 'Clouds gather over the eastern field.',
			},
			{
				id: 'compact-action',
				source: 'action',
				content: 'You step closer to the gate.',
			},
			{
				id: 'compact-location',
				source: 'system',
				subtype: 'location',
				content: 'The road bends beside the bramble wall.',
			},
			{
				id: 'compact-npc',
				source: 'Séamas Ó Briain',
				content: 'There was a cart here before dawn.',
			},
		]) {
			await emitEvent(page, 'text-log', output);
			await expect(host).toHaveAttribute(
				'data-visible-live-line-keys',
				`compact-command,${output.id}`,
			);
		}

		await emitEvent(page, 'text-log', {
			id: 'compact-active-stream',
			stream_turn_id: 77,
			source: 'Séamas Ó Briain',
			content: '',
		});
		await emitEvent(page, 'stream-token', {
			turn_id: 77,
			message_id: 'compact-active-stream',
			source: 'Séamas Ó Briain',
			token: 'I am still speaking ',
		});
		await expect(host).toHaveAttribute(
			'data-visible-live-line-keys',
			'compact-command,compact-active-stream',
		);
		await expect(host).toHaveAttribute(
			'data-visible-live-line-kinds',
			'command,npc',
		);

		// A later status line cannot evict the active streamed NPC output.
		await emitEvent(page, 'text-log', {
			id: 'compact-later-system',
			source: 'system',
			content: 'The clock advances.',
		});
		await expect(host).toHaveAttribute(
			'data-visible-live-line-keys',
			'compact-command,compact-active-stream',
		);
	});

	test('production action source remains Parish narration', async ({
		page,
	}) => {
		await setupNotebookPage(page);

		await emitEvent(page, 'text-log', {
			id: 'production-action',
			source: 'action',
			content: 'You turn over the first row of soil.',
		});

		const chronicle = page.getByLabel('Live chronicle');
		await expect(chronicle).toContainText(
			'Parish: You turn over the first row of soil.',
		);
		await expect(chronicle).not.toContainText(
			'Someone: You turn over the first row of soil.',
		);
	});

	test('canonical game-context reset replaces the previous branch chronicle', async ({
		page,
	}) => {
		await setupNotebookPage(page);
		const chronicle = page.getByLabel('Live chronicle');

		await emitEvent(page, 'text-log', {
			id: 'old-branch-line',
			source: 'player',
			content: 'A memory belonging only to the old branch.',
		});
		await expect(chronicle).toContainText(
			'You: A memory belonging only to the old branch.',
		);

		await emitEvent(page, 'game-context-reset', null);
		// Use the same location as the initial snapshot. The replacement scene
		// only appears if the reset also clears location-description dedup.
		await emitEvent(page, 'world-update', {
			...SNAPSHOTS.morning,
			location_description: 'A fresh branch opens beside the same gate.',
		});

		await expect(chronicle).not.toContainText(
			'A memory belonging only to the old branch.',
		);
		await expect(chronicle).toContainText(
			'Place: A fresh branch opens beside the same gate.',
		);
	});

	test('authoritative world movement changes the rendered scene while the map is stale', async ({
		page,
	}) => {
		await setupNotebookPage(page);
		const host = page.getByTestId('illustrated-notebook-pixi-host');

		await emitEvent(page, 'world-update', {
			...SNAPSHOTS.morning,
			location_id: 15,
			location_name: 'Kilteevan Village',
			location_description: 'Whitewashed cottages gather around the well.',
		});
		await expect(host).toHaveAttribute('data-scene-location-id', '15');
		await expect(host).toHaveAttribute(
			'data-scene-plate',
			'/rundale/notebook-ui/scene-kilteevan-village.png',
		);

		// No map update is emitted between the two authoritative world
		// snapshots: the mocked map remains at its unrelated Dublin id.
		await emitEvent(page, 'world-update', {
			...SNAPSHOTS.morning,
			location_id: 9,
			location_name: "Murphy's Farm",
			location_description: 'A muddy working yard opens before the byre.',
		});
		await expect(host).toHaveAttribute('data-scene-location-id', '9');
		await expect(host).toHaveAttribute(
			'data-scene-plate',
			'/rundale/notebook-ui/scene-murphys-farm.png',
		);
	});
});
