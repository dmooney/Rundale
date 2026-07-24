import type { Page } from '@playwright/test';
import { PNG } from 'pngjs';
import {
	expect,
	installTauriMock,
	test,
	waitForTextureCompleteNotebookFrame,
} from './fixtures';

const PIXI_CANVAS = '[data-testid="illustrated-notebook-pixi-host"] canvas';
// The bottom vignette intentionally darkens the Map card by roughly 20%.
// Keep the tolerance bounded well below the distances between sentinel colors.
const COLOR_TOLERANCE = 64;

type Rgb = readonly [number, number, number];

interface RasterSentinel {
	assetUrl: string;
	targetName: string;
	color: Rgb;
}

const RASTER_SENTINELS: readonly RasterSentinel[] = [
	{
		assetUrl: '/rundale/illustrated-notebook-v2/icon-ask.png',
		targetName: 'Ask action',
		color: [251, 17, 183],
	},
	{
		assetUrl: '/rundale/illustrated-notebook-v2/icon-tab-notes.png',
		targetName: 'Open Notes notebook tab',
		color: [17, 241, 47],
	},
	{
		assetUrl: '/rundale/illustrated-notebook-v2/icon-map.png',
		targetName: 'Open parish map',
		color: [19, 73, 251],
	},
] as const;

function solidPng(color: Rgb): Buffer {
	const image = new PNG({ width: 32, height: 32 });
	for (let index = 0; index < image.data.length; index += 4) {
		image.data[index] = color[0];
		image.data[index + 1] = color[1];
		image.data[index + 2] = color[2];
		image.data[index + 3] = 255;
	}
	return PNG.sync.write(image);
}

async function installRasterSentinels(
	page: Page,
): Promise<Map<string, number>> {
	const requestCounts = new Map(
		RASTER_SENTINELS.map(({ assetUrl }) => [assetUrl, 0]),
	);

	for (const sentinel of RASTER_SENTINELS) {
		const body = solidPng(sentinel.color);
		await page.route(`**${sentinel.assetUrl}`, (route) => {
			requestCounts.set(
				sentinel.assetUrl,
				(requestCounts.get(sentinel.assetUrl) ?? 0) + 1,
			);
			return route.fulfill({
				status: 200,
				contentType: 'image/png',
				body,
			});
		});
	}

	return requestCounts;
}

function isNearColor(pixels: Buffer, index: number, color: Rgb): boolean {
	return (
		pixels[index + 3] >= 240 &&
		Math.abs(pixels[index] - color[0]) <= COLOR_TOLERANCE &&
		Math.abs(pixels[index + 1] - color[1]) <= COLOR_TOLERANCE &&
		Math.abs(pixels[index + 2] - color[2]) <= COLOR_TOLERANCE
	);
}

async function expectSentinelsInsideSemanticTargets(page: Page): Promise<void> {
	const canvas = page.locator(PIXI_CANVAS);
	const canvasBox = await canvas.boundingBox();
	if (!canvasBox) throw new Error('Pixi canvas has no layout bounds');

	const screenshot = PNG.sync.read(await canvas.screenshot());
	const scaleX = screenshot.width / canvasBox.width;
	const scaleY = screenshot.height / canvasBox.height;

	for (const sentinel of RASTER_SENTINELS) {
		const target = page.getByRole('button', {
			name: sentinel.targetName,
			exact: true,
		});
		await expect(target).toHaveCount(1);
		const targetBox = await target.boundingBox();
		if (!targetBox) {
			throw new Error(`${sentinel.targetName} has no semantic target bounds`);
		}

		const left = Math.max(0, Math.floor((targetBox.x - canvasBox.x) * scaleX));
		const top = Math.max(0, Math.floor((targetBox.y - canvasBox.y) * scaleY));
		const right = Math.min(
			screenshot.width,
			Math.ceil((targetBox.x + targetBox.width - canvasBox.x) * scaleX),
		);
		const bottom = Math.min(
			screenshot.height,
			Math.ceil((targetBox.y + targetBox.height - canvasBox.y) * scaleY),
		);
		const regionPixels = Math.max(0, right - left) * Math.max(0, bottom - top);
		expect(
			regionPixels,
			`${sentinel.targetName} must overlap the Pixi canvas`,
		).toBeGreaterThan(0);

		let matchingPixels = 0;
		let nearestColor: Rgb = [0, 0, 0];
		let nearestDelta = Number.POSITIVE_INFINITY;
		for (let y = top; y < bottom; y += 1) {
			for (let x = left; x < right; x += 1) {
				const index = (y * screenshot.width + x) * 4;
				if (isNearColor(screenshot.data, index, sentinel.color)) {
					matchingPixels += 1;
				}
				if (screenshot.data[index + 3] >= 240) {
					const candidate: Rgb = [
						screenshot.data[index],
						screenshot.data[index + 1],
						screenshot.data[index + 2],
					];
					const delta = Math.max(
						Math.abs(candidate[0] - sentinel.color[0]),
						Math.abs(candidate[1] - sentinel.color[1]),
						Math.abs(candidate[2] - sentinel.color[2]),
					);
					if (delta < nearestDelta) {
						nearestDelta = delta;
						nearestColor = candidate;
					}
				}
			}
		}

		expect(
			matchingPixels,
			`${sentinel.assetUrl} must be painted inside "${sentinel.targetName}" (nearest opaque pixel: rgb(${nearestColor.join(', ')}), max-channel delta ${nearestDelta})`,
		).toBeGreaterThan(Math.max(24, Math.floor(regionPixels * 0.02)));
	}
}

test('Pixi paints v2 raster icons inside their semantic controls', async ({
	page,
}) => {
	await page.setViewportSize({ width: 1280, height: 800 });
	const requestCounts = await installRasterSentinels(page);
	await installTauriMock(page, 'morning');

	await page.goto('/');
	await page.waitForLoadState('networkidle');
	await expect(page.locator(PIXI_CANVAS)).toBeVisible();
	await waitForTextureCompleteNotebookFrame(page);

	for (const { assetUrl } of RASTER_SENTINELS) {
		expect(
			requestCounts.get(assetUrl),
			`${assetUrl} must be requested by the active renderer`,
		).toBeGreaterThan(0);
	}
	await expectSentinelsInsideSemanticTargets(page);
});
