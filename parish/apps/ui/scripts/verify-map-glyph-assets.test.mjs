import { describe, expect, it } from 'vitest';
import { verifyMapGlyphAssets } from './verify-map-glyph-assets.mjs';

describe('bundled MapLibre glyph assets', () => {
	it('contains every BMP range with its recorded hash and OFL', async () => {
		await expect(verifyMapGlyphAssets()).resolves.toEqual({ ranges: 256 });
	});
});
