import { describe, expect, it } from 'vitest';
import { PARISH_ASSET_URLS, PARISH_ASSETS } from './assets';

describe('fresh illustrated parish asset boundary', () => {
	it('uses only the v2 namespace', () => {
		expect(PARISH_ASSET_URLS.length).toBeGreaterThan(2);
		for (const url of PARISH_ASSET_URLS) {
			expect(url).toMatch(/^\/rundale\/illustrated-notebook-v2\//);
		}
	});

	it('keeps the period-correct sewn page and excludes rejected visual assets', () => {
		expect(PARISH_ASSETS.sewnPage).toContain('sewn-notebook-page.png');
		for (const url of PARISH_ASSET_URLS) {
			expect(url).not.toMatch(/spiral|ring|placeholder|stamp-frame|npc-marker/);
		}
	});
});
