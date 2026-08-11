import { expect, test, installTauriMock, emitEvent } from './fixtures';

test('bundled glyphs stay same-origin across every MapLibre surface', async ({
	page,
}, testInfo) => {
	const glyphResponses: Array<{ url: string; status: number }> = [];
	const failedGlyphRequests: string[] = [];
	const relevantConsoleProblems: string[] = [];
	const demoRequests: string[] = [];

	page.on('request', (request) => {
		if (request.url().includes('demotiles.maplibre.org')) {
			demoRequests.push(request.url());
		}
	});
	page.on('response', (response) => {
		if (response.url().includes('/map-glyphs/')) {
			glyphResponses.push({ url: response.url(), status: response.status() });
		}
	});
	page.on('requestfailed', (request) => {
		if (/glyph|\.pbf(?:\?|$)/i.test(request.url())) {
			failedGlyphRequests.push(request.url());
		}
	});
	page.on('console', (message) => {
		if (
			['warning', 'error'].includes(message.type()) &&
			/(glyph|demotiles|404|failed to load)/i.test(message.text())
		) {
			relevantConsoleProblems.push(`${message.type()}: ${message.text()}`);
		}
	});

	const assertHealthy = async (surface: string) => {
		await expect.poll(() => glyphResponses.length).toBeGreaterThan(0);
		const appOrigin = new URL(page.url()).origin;
		expect(
			glyphResponses.every(({ url }) => new URL(url).origin === appOrigin),
			`${surface} glyph requests must be same-origin`,
		).toBe(true);
		expect(
			glyphResponses.filter(({ status }) => status >= 400),
			`${surface} glyph responses`,
		).toEqual([]);
		expect(failedGlyphRequests, `${surface} failed glyph requests`).toEqual([]);
		expect(relevantConsoleProblems, `${surface} console problems`).toEqual([]);
		expect(demoRequests, `${surface} demo-CDN requests`).toEqual([]);
	};

	await installTauriMock(page, 'morning');
	await page.goto('/');
	await expect(page.getByTestId('app-root')).toHaveAttribute(
		'data-controller-ready',
		'true',
	);
	await expect(page.getByTestId('map-panel').locator('canvas')).toBeVisible();
	await assertHealthy('minimap');

	await page.keyboard.press('m');
	const fullMap = page.getByTestId('full-map');
	await expect(fullMap.locator('canvas')).toBeVisible();
	await assertHealthy('full map');
	await testInfo.attach('bundled-glyphs-full-map', {
		body: await fullMap.screenshot(),
		contentType: 'image/png',
	});

	// Exercise MapController.setTileSource(), which rebuilds the whole style.
	await emitEvent(page, 'tiles-switch', { id: 'missing-test-source' });
	await expect(fullMap.locator('canvas')).toBeVisible();
	await assertHealthy('style reset');
	await page.keyboard.press('Escape');

	await page.getByRole('button', { name: 'Developer tools menu' }).click();
	await page.getByRole('menuitem', { name: 'Designer' }).click();
	await page.getByRole('button', { name: /Rundale/ }).click();
	await page.getByRole('button', { name: 'Locations' }).click();
	await page.getByRole('button', { name: /Kilteevan Crossroads/ }).click();
	const editorMap = page.locator('.map-frame');
	await expect(editorMap.locator('canvas')).toBeVisible();
	await assertHealthy('editor map');
	await testInfo.attach('bundled-glyphs-editor-map', {
		body: await editorMap.screenshot(),
		contentType: 'image/png',
	});
	await testInfo.attach('bundled-glyph-network-evidence', {
		body: Buffer.from(
			JSON.stringify(
				{
					glyphResponses,
					failedGlyphRequests,
					relevantConsoleProblems,
					demoRequests,
				},
				null,
				2,
			),
		),
		contentType: 'application/json',
	});
});
