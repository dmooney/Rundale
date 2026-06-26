import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, mkdir, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { deflateSync } from 'node:zlib';

import {
    alphaEdgeSummary,
    auditConfiguredSceneAtoms,
    auditCrossroadsAtoms,
    auditSceneAtoms,
    parsePng,
    visibleContentSummary,
} from './audit-scene-atoms.mjs';

const crcTable = new Uint32Array(256).map((_, index) => {
    let value = index;
    for (let bit = 0; bit < 8; bit += 1) {
        value = value & 1 ? 0xedb88320 ^ (value >>> 1) : value >>> 1;
    }
    return value >>> 0;
});

function crc32(buffer) {
    let crc = 0xffffffff;
    for (const byte of buffer) {
        crc = crcTable[(crc ^ byte) & 0xff] ^ (crc >>> 8);
    }
    return (crc ^ 0xffffffff) >>> 0;
}

function chunk(type, data) {
    const typeBuffer = Buffer.from(type, 'ascii');
    const body = Buffer.concat([typeBuffer, data]);
    const out = Buffer.alloc(12 + data.length);
    out.writeUInt32BE(data.length, 0);
    typeBuffer.copy(out, 4);
    data.copy(out, 8);
    out.writeUInt32BE(crc32(body), 8 + data.length);
    return out;
}

function rgbaPng(width, height, alphaFor) {
    const ihdr = Buffer.alloc(13);
    ihdr.writeUInt32BE(width, 0);
    ihdr.writeUInt32BE(height, 4);
    ihdr[8] = 8;
    ihdr[9] = 6;
    ihdr[10] = 0;
    ihdr[11] = 0;
    ihdr[12] = 0;

    const scanlines = [];
    for (let y = 0; y < height; y += 1) {
        const row = Buffer.alloc(1 + width * 4);
        row[0] = 0;
        for (let x = 0; x < width; x += 1) {
            const offset = 1 + x * 4;
            row[offset] = 120;
            row[offset + 1] = 110;
            row[offset + 2] = 90;
            row[offset + 3] = alphaFor(x, y);
        }
        scanlines.push(row);
    }

    return Buffer.concat([
        Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
        chunk('IHDR', ihdr),
        chunk('IDAT', deflateSync(Buffer.concat(scanlines))),
        chunk('IEND', Buffer.alloc(0)),
    ]);
}

async function writeFixtureScene({ scene, assets, pngs }) {
    const root = await mkdtemp(path.join(tmpdir(), 'rundale-atom-audit-'));
    for (const [relativePath, png] of Object.entries(pngs)) {
        const fullPath = path.join(root, relativePath);
        await mkdir(path.dirname(fullPath), { recursive: true });
        await writeFile(fullPath, png);
    }
    const scenesPath = path.join(root, 'scenes.json');
    await writeFile(
        scenesPath,
        JSON.stringify(
            {
                assets,
                scenes: [scene],
            },
            null,
            2,
        ),
    );
    return { root, scenesPath };
}

test('parses PNG dimensions and alpha edges', () => {
    const image = parsePng(rgbaPng(3, 2, (x) => (x === 0 ? 255 : 0)));

    assert.equal(image.width, 3);
    assert.equal(image.height, 2);
    assert.equal(image.colorType, 6);
    assert.equal(alphaEdgeSummary(image).left.strong, 2);
    assert.equal(alphaEdgeSummary(image).right.strong, 0);
});

test('summarizes visible PNG contribution metrics', () => {
    const image = parsePng(rgbaPng(4, 3, (x, y) => (x >= 1 && x <= 2 && y === 1 ? 255 : 0)));
    const summary = visibleContentSummary(image);

    assert.equal(summary.visiblePixels, 2);
    assert.equal(summary.alphaCoverage, 2 / 12);
    assert.deepEqual(summary.bbox, {
        x: 1,
        y: 1,
        width: 2,
        height: 1,
    });
    assert.equal(summary.bboxCoverage, 2 / 12);
    assert.equal(summary.bboxAlphaCoverage, 1);
});

test('audit fails blank PNG atoms and reports them in the scene summary', async () => {
    const { root, scenesPath } = await writeFixtureScene({
        assets: [
            {
                id: 'blank-prop',
                kind: 'prop',
                image: 'assets/test/blank.png',
                anchor: [50, 100],
            },
        ],
        scene: {
            slug: 'blank-fixture',
            native_size: [8, 8],
            layers: [
                {
                    id: 'blank-layer',
                    asset: 'blank-prop',
                    x: 50,
                    y: 50,
                    z: 0,
                    scale: 1,
                },
            ],
        },
        pngs: {
            'assets/test/blank.png': rgbaPng(4, 4, () => 0),
        },
    });

    const result = await auditSceneAtoms({
        slug: 'blank-fixture',
        scenesPath,
        modDir: root,
        minKitLayers: 0,
        minReusedKitAssets: 0,
    });

    assert.equal(result.ok, false);
    assert.equal(result.summary.meaningfulAtoms, 0);
    assert.equal(result.summary.blankAtoms.length, 1);
    assert.equal(result.summary.blankAtoms[0].reason, 'blank');
    assert.match(result.failures.join('\n'), /blank-layer: assets\/test\/blank\.png is blank/);
});

test('audit fails mis-sized full-stage shadow and lighting overlays', async () => {
    const { root, scenesPath } = await writeFixtureScene({
        assets: [
            {
                id: 'bad-shadow',
                kind: 'shadow',
                image: 'assets/test/bad-shadow.png',
                anchor: [50, 50],
            },
            {
                id: 'bad-lighting',
                kind: 'lighting',
                image: 'assets/test/bad-lighting.png',
                anchor: [50, 50],
            },
        ],
        scene: {
            slug: 'bad-effects-fixture',
            native_size: [8, 8],
            layers: [
                {
                    id: 'bad-shadow-layer',
                    asset: 'bad-shadow',
                    x: 50,
                    y: 50,
                    z: -10,
                    scale: 1,
                },
                {
                    id: 'bad-lighting-layer',
                    asset: 'bad-lighting',
                    x: 50,
                    y: 50,
                    z: 10,
                    scale: 1,
                },
            ],
        },
        pngs: {
            'assets/test/bad-shadow.png': rgbaPng(6, 8, (x, y) =>
                x > 0 && x < 5 && y > 0 && y < 7 ? 255 : 0,
            ),
            'assets/test/bad-lighting.png': rgbaPng(8, 6, (x, y) =>
                x > 0 && x < 7 && y > 0 && y < 5 ? 255 : 0,
            ),
        },
    });

    const result = await auditSceneAtoms({
        slug: 'bad-effects-fixture',
        scenesPath,
        modDir: root,
        minKitLayers: 0,
        minReusedKitAssets: 0,
    });

    assert.equal(result.ok, false);
    assert.equal(result.summary.fullStageEffectOverlays.length, 2);
    assert.equal(result.summary.suspiciousFullStageAtoms.length, 2);
    assert.match(
        result.failures.join('\n'),
        /bad-shadow-layer: shadow full-stage overlay must match native_size 8x8, got 6x8/,
    );
    assert.match(
        result.failures.join('\n'),
        /bad-lighting-layer: lighting full-stage overlay must match native_size 8x8, got 8x6/,
    );
});

test('Crossroads compositor atoms pass the local audit', async () => {
    const result = await auditCrossroadsAtoms();

    assert.deepEqual(result.failures, []);
    assert.equal(result.ok, true);
    assert.equal(result.summary.slug, 'the-crossroads');
    assert.ok(result.summary.layers >= 14);
    assert.ok(result.summary.kitLayers >= 4);
    assert.ok(result.summary.reusedKitAssets >= 1);
    assert.ok(result.summary.reusableKitFamilies >= 3);
    assert.equal(result.summary.meaningfulAtoms, result.summary.layers);
    assert.deepEqual(result.summary.blankAtoms, []);
    assert.deepEqual(result.summary.suspiciousFullStageAtoms, []);
});

test('Kilteevan generated full-scene plate passes the local audit', async () => {
    const result = await auditSceneAtoms({
        slug: 'kilteevan-village',
        requiredReusableKitKinds: [],
        minKitLayers: 0,
        minReusedKitAssets: 0,
    });

    assert.deepEqual(result.failures, []);
    assert.equal(result.ok, true);
    assert.equal(result.summary.slug, 'kilteevan-village');
    assert.equal(result.summary.layers, 1);
    assert.equal(result.summary.kitLayers, 0);
    assert.equal(result.summary.reusedKitAssets, 0);
    assert.equal(result.summary.reusableKitFamilies, 0);
    assert.equal(result.summary.meaningfulAtoms, result.summary.layers);
    assert.deepEqual(result.summary.blankAtoms, []);
    assert.deepEqual(result.summary.suspiciousFullStageAtoms, []);
    assert.equal(result.summary.atoms[0].assetId, 'kilteevan-m9-full-scene-base');
    assert.equal(result.summary.atoms[0].kind, 'plate');
    assert.deepEqual([result.summary.atoms[0].width, result.summary.atoms[0].height], [1280, 720]);
});

test("Darcy's Pub compositor atoms pass the local audit", async () => {
    const result = await auditSceneAtoms({
        slug: 'darcys-pub',
        requiredReusableKitKinds: ['vessel', 'wood', 'lighting'],
        minKitLayers: 10,
        minReusedKitAssets: 1,
        allowedFullStageAssetIds: [
            'pub-hearth',
            'pub-back-shelves',
            'pub-bar-counter',
            'pub-door-window',
            'pub-foreground-furniture',
        ],
    });

    assert.deepEqual(result.failures, []);
    assert.equal(result.ok, true);
    assert.equal(result.summary.slug, 'darcys-pub');
    assert.ok(result.summary.layers >= 24);
    assert.ok(result.summary.kitLayers >= 10);
    assert.ok(result.summary.reusedKitAssets >= 1);
    assert.ok(result.summary.reusableKitFamilies >= 3);
    assert.equal(result.summary.meaningfulAtoms, result.summary.layers);
    assert.deepEqual(result.summary.blankAtoms, []);
    assert.deepEqual(result.summary.suspiciousFullStageAtoms, []);
});

test('configured atom audit covers all three playable slice scenes', async () => {
    const result = await auditConfiguredSceneAtoms();

    assert.deepEqual(result.failures, []);
    assert.equal(result.ok, true);
    assert.deepEqual(
        result.results.map((sceneResult) => sceneResult.summary.slug),
        ['kilteevan-village', 'the-crossroads', 'darcys-pub'],
    );
    for (const sceneResult of result.results) {
        const { summary } = sceneResult;
        assert.equal(summary.meaningfulAtoms, summary.layers);
        assert.deepEqual(summary.blankAtoms, []);
        assert.deepEqual(summary.suspiciousFullStageAtoms, []);
        assert.equal(summary.atoms.length, summary.checkedPngs);
        if (summary.slug === 'kilteevan-village') {
            assert.equal(summary.fullStageEffectOverlays.length, 0);
        } else {
            assert.ok(summary.fullStageEffectOverlays.length >= 2);
        }
        for (const atom of summary.atoms) {
            assert.equal(typeof atom.visiblePixels, 'number');
            assert.equal(typeof atom.alphaCoverage, 'number');
            assert.equal(typeof atom.bboxCoverage, 'number');
            assert.ok(atom.visiblePixels > 0);
            assert.ok(atom.bbox);
        }
    }
});
