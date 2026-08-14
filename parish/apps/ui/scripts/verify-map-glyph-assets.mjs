import { createHash } from 'node:crypto';
import { readFile, readdir } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const UI_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');

function sha256(bytes) {
	return createHash('sha256').update(bytes).digest('hex');
}

export async function verifyMapGlyphAssets(
	assetDir = resolve(UI_ROOT, 'static/map-glyphs'),
) {
	const fontDir = resolve(assetDir, 'Open Sans Regular');
	const files = (await readdir(fontDir)).filter((name) =>
		name.endsWith('.pbf'),
	);
	const expected = Array.from(
		{ length: 256 },
		(_, index) => `${index * 256}-${index * 256 + 255}.pbf`,
	);
	if (files.length !== expected.length) {
		throw new Error(`expected 256 glyph ranges, found ${files.length}`);
	}
	for (const name of expected) {
		if (!files.includes(name)) throw new Error(`missing glyph range ${name}`);
	}

	const manifest = (await readFile(resolve(assetDir, 'SHA256SUMS'), 'utf8'))
		.trim()
		.split('\n');
	if (manifest.length !== expected.length) {
		throw new Error(`expected 256 glyph hashes, found ${manifest.length}`);
	}

	const manifestPaths = new Set();
	for (const line of manifest) {
		const match = /^([a-f0-9]{64}) {2}(Open Sans Regular\/\d+-\d+\.pbf)$/.exec(
			line,
		);
		if (!match) throw new Error(`invalid glyph hash entry: ${line}`);
		if (manifestPaths.has(match[2])) {
			throw new Error(`duplicate glyph hash entry: ${match[2]}`);
		}
		manifestPaths.add(match[2]);
		const actual = sha256(await readFile(resolve(assetDir, match[2])));
		if (actual !== match[1])
			throw new Error(`glyph hash mismatch: ${match[2]}`);
	}
	for (const name of expected) {
		const path = `Open Sans Regular/${name}`;
		if (!manifestPaths.has(path))
			throw new Error(`missing glyph hash: ${path}`);
	}

	const ofl = await readFile(resolve(assetDir, 'OFL.txt'), 'utf8');
	if (!ofl.includes('SIL OPEN FONT LICENSE Version 1.1')) {
		throw new Error('bundled Open Sans OFL.txt is missing or invalid');
	}
	return { ranges: files.length };
}

if (
	process.argv[1] &&
	resolve(process.argv[1]) === fileURLToPath(import.meta.url)
) {
	const requestedDir = process.argv[2]
		? resolve(UI_ROOT, process.argv[2])
		: undefined;
	await verifyMapGlyphAssets(requestedDir);
}
