import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { inflateSync } from 'node:zlib';
import { describe, expect, it } from 'vitest';
import {
	PARISH_ASSET_URLS,
	PARISH_ASSETS,
	PARISH_UI_ASSET_MANIFEST,
} from './assets';

interface ManifestAsset {
	key: string;
	file: string;
	width: number;
	height: number;
	alpha: 'opaque' | 'cutout';
	sha256: string;
}

interface UiAssetManifest {
	runtime_base: string;
	assets: ManifestAsset[];
}

function flattenUrls(value: unknown): string[] {
	if (typeof value === 'string') return [value];
	if (Array.isArray(value)) return value.flatMap(flattenUrls);
	if (value && typeof value === 'object') {
		return Object.values(value).flatMap(flattenUrls);
	}
	return [];
}

function paeth(left: number, above: number, upperLeft: number): number {
	const estimate = left + above - upperLeft;
	const leftDistance = Math.abs(estimate - left);
	const aboveDistance = Math.abs(estimate - above);
	const upperLeftDistance = Math.abs(estimate - upperLeft);
	if (leftDistance <= aboveDistance && leftDistance <= upperLeftDistance) {
		return left;
	}
	return aboveDistance <= upperLeftDistance ? above : upperLeft;
}

function alphaRange(png: Buffer): {
	min: number;
	max: number;
	corners: [number, number, number, number];
} {
	const width = png.readUInt32BE(16);
	const height = png.readUInt32BE(20);
	expect(png[24]).toBe(8);
	expect(png[25]).toBe(6);
	expect(png[28]).toBe(0);

	const compressed: Buffer[] = [];
	let offset = 8;
	while (offset < png.length) {
		const length = png.readUInt32BE(offset);
		const type = png.toString('ascii', offset + 4, offset + 8);
		if (type === 'IDAT') {
			compressed.push(png.subarray(offset + 8, offset + 8 + length));
		}
		offset += length + 12;
	}

	const raw = inflateSync(Buffer.concat(compressed));
	const bytesPerPixel = 4;
	const stride = width * bytesPerPixel;
	let cursor = 0;
	let previous = Buffer.alloc(stride);
	let min = 255;
	let max = 0;
	const corners: [number, number, number, number] = [0, 0, 0, 0];

	for (let y = 0; y < height; y += 1) {
		const filter = raw[cursor];
		cursor += 1;
		if (filter > 4) throw new Error(`Unsupported PNG filter ${filter}`);
		const row = Buffer.alloc(stride);
		for (let x = 0; x < stride; x += 1) {
			const source = raw[cursor];
			cursor += 1;
			const left = x >= bytesPerPixel ? row[x - bytesPerPixel] : 0;
			const above = previous[x];
			const upperLeft = x >= bytesPerPixel ? previous[x - bytesPerPixel] : 0;
			const predictor =
				filter === 0
					? 0
					: filter === 1
						? left
						: filter === 2
							? above
							: filter === 3
								? Math.floor((left + above) / 2)
								: filter === 4
									? paeth(left, above, upperLeft)
									: 0;
			row[x] = (source + predictor) & 0xff;
		}
		for (let x = 3; x < stride; x += bytesPerPixel) {
			min = Math.min(min, row[x]);
			max = Math.max(max, row[x]);
		}
		if (y === 0) {
			corners[0] = row[3];
			corners[1] = row[stride - 1];
		}
		if (y === height - 1) {
			corners[2] = row[3];
			corners[3] = row[stride - 1];
		}
		previous = row;
	}
	return { min, max, corners };
}

const manifestPath = resolve(
	process.cwd(),
	'static',
	PARISH_UI_ASSET_MANIFEST.slice(1),
);
const manifest = JSON.parse(
	readFileSync(manifestPath, 'utf8'),
) as UiAssetManifest;

describe('fresh illustrated parish asset boundary', () => {
	it('preloads every documented v2 raster exactly once', () => {
		const documentedUrls = manifest.assets.map(
			(asset) => `${manifest.runtime_base}${asset.file}`,
		);
		expect(PARISH_ASSET_URLS).toEqual(documentedUrls);
		expect(new Set(PARISH_ASSET_URLS).size).toBe(PARISH_ASSET_URLS.length);
		expect(new Set(flattenUrls(PARISH_ASSETS))).toEqual(
			new Set(PARISH_ASSET_URLS),
		);
		for (const url of PARISH_ASSET_URLS) {
			expect(url).toMatch(/^\/rundale\/illustrated-notebook-v2\//);
			expect(url).not.toMatch(/\.svg(?:$|\?)/);
		}
	});

	it('pins dimensions, hashes, and real alpha for every generated cutout', () => {
		for (const asset of manifest.assets) {
			const bytes = readFileSync(
				resolve(
					process.cwd(),
					'static',
					manifest.runtime_base.slice(1),
					asset.file,
				),
			);
			expect(bytes.subarray(0, 8)).toEqual(
				Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]),
			);
			expect(bytes.readUInt32BE(16)).toBe(asset.width);
			expect(bytes.readUInt32BE(20)).toBe(asset.height);
			expect(createHash('sha256').update(bytes).digest('hex')).toBe(
				asset.sha256,
			);
			if (asset.alpha === 'cutout') {
				const alpha = alphaRange(bytes);
				expect(alpha.min).toBe(0);
				expect(alpha.max).toBeGreaterThanOrEqual(250);
				expect(alpha.corners).toEqual([0, 0, 0, 0]);
			}
		}
	});

	it('keeps the period-correct sewn page and excludes rejected visual assets', () => {
		expect(PARISH_ASSETS.sewnPage).toContain('sewn-notebook-page.png');
		expect(PARISH_ASSETS.indexRail).toContain('notebook-index-rail.png');
		expect(PARISH_ASSETS.compassIcon).toContain('icon-compass.png');
		for (const url of PARISH_ASSET_URLS) {
			expect(url).not.toMatch(
				/spiral|ring|placeholder|stamp-frame|npc-marker|parchment-tab/,
			);
		}
	});
});
