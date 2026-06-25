import test from 'node:test';
import assert from 'node:assert/strict';
import { access, mkdtemp, rm, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { auditSceneAtoms } from './audit-scene-atoms.mjs';
import {
    generateVillageVariantPack,
    loadVillageVariantInputs,
    variantSignature,
} from './generate-village-variants.mjs';

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, '../../../..');
const modDir = path.join(repoRoot, 'mods/rundale');

function rectInBounds(rect) {
    assert.equal(Array.isArray(rect), true);
    assert.equal(rect.length, 4);
    const [x, y, width, height] = rect;
    assert.ok(x >= 0, `rect x ${x} is in bounds`);
    assert.ok(y >= 0, `rect y ${y} is in bounds`);
    assert.ok(width > 0, `rect width ${width} is positive`);
    assert.ok(height > 0, `rect height ${height} is positive`);
    assert.ok(x + width <= 100, `rect right ${x + width} is in bounds`);
    assert.ok(y + height <= 100, `rect bottom ${y + height} is in bounds`);
}

function reusableKitFamilies(scene, assetsById) {
    const usageByAsset = new Map();
    for (const layer of scene.layers) {
        const asset = assetsById.get(layer.asset);
        if (!asset?.image?.includes('/atoms/kit/')) {
            continue;
        }
        const usage = usageByAsset.get(layer.asset) || [];
        usage.push(layer);
        usageByAsset.set(layer.asset, usage);
    }
    const families = new Set();
    for (const [assetId, layers] of usageByAsset.entries()) {
        const positions = new Set(layers.map((layer) => `${layer.x},${layer.y}`));
        if (layers.length >= 3 && positions.size >= 3) {
            families.add(assetsById.get(assetId).kind);
        }
    }
    return families;
}

async function writeTempPack(pack) {
    const dir = await mkdtemp(path.join(os.tmpdir(), 'rundale-village-variants-'));
    const scenesPath = path.join(dir, 'generated-scenes.json');
    await writeFile(scenesPath, `${JSON.stringify(pack, null, 2)}\n`);
    return { dir, scenesPath };
}

test('village variant recipe generates ten compositor-compatible scenes', async () => {
    const inputs = await loadVillageVariantInputs();
    const pack = generateVillageVariantPack(inputs);
    const sourceScene = inputs.sceneIndex.scenes.find((scene) => scene.slug === inputs.recipe.source_slug);
    const assetsById = new Map(pack.assets.map((asset) => [asset.id, asset]));

    assert.equal(inputs.recipe.variants.length, 10);
    assert.equal(pack.summary.variant_count, 10);
    assert.equal(pack.scenes.length, 10);
    assert.equal(pack.source.source_slug, 'kilteevan-village');
    assert.equal(pack.source.source_location_id, 15);

    const slugs = new Set();
    const locationIds = new Set();
    const signatures = new Set();
    for (const scene of pack.scenes) {
        slugs.add(scene.slug);
        locationIds.add(scene.location_id);
        signatures.add(variantSignature(scene));

        assert.deepEqual(scene.native_size, [1280, 720], scene.slug);
        assert.equal(scene.plate, sourceScene.plate, `${scene.slug} keeps legacy plate`);
        assert.equal(scene.underlay, sourceScene.underlay, `${scene.slug} keeps legacy underlay`);
        assert.equal(scene.layers.length, sourceScene.layers.length, `${scene.slug} keeps layer count`);
        assert.equal(scene.hotspots.length, sourceScene.hotspots.length, `${scene.slug} keeps hotspot count`);
        assert.equal(scene.slots.length, sourceScene.slots.length, `${scene.slug} keeps slot count`);

        const zValues = new Map();
        let m2KitLayers = 0;
        for (const layer of scene.layers) {
            assert.equal(zValues.has(layer.z), false, `${scene.slug}/${layer.id} duplicate z=${layer.z}`);
            zValues.set(layer.z, layer.id);

            const asset = assetsById.get(layer.asset);
            assert.ok(asset, `${scene.slug}/${layer.id} has asset ${layer.asset}`);
            assert.equal(asset.image.endsWith('.png'), true, `${scene.slug}/${layer.id} is PNG`);
            assert.equal(asset.image.endsWith('.svg'), false, `${scene.slug}/${layer.id} is not SVG`);
            assert.match(asset.image, /^assets\/scenes\/kilteevan-village\/atoms\//);
            await access(path.join(modDir, asset.image));
            if (asset.image.includes('/atoms/kit/m2-')) {
                m2KitLayers += 1;
            }
        }
        assert.ok(m2KitLayers >= 40, `${scene.slug} has at least forty M2 kit layers`);

        for (const hotspot of scene.hotspots) {
            rectInBounds(hotspot.shape?.rect);
            assert.ok(hotspot.id, `${scene.slug} hotspot has id`);
            assert.ok(hotspot.label, `${scene.slug}/${hotspot.id} has label`);
            assert.ok(hotspot.action, `${scene.slug}/${hotspot.id} has action`);
        }

        for (const slot of scene.slots) {
            assert.ok(slot.id, `${scene.slug} slot has id`);
            assert.ok(slot.x >= 0 && slot.x <= 100, `${scene.slug}/${slot.id} x in bounds`);
            assert.ok(slot.y >= 0 && slot.y <= 100, `${scene.slug}/${slot.id} y in bounds`);
            assert.ok((slot.scale ?? 1) > 0, `${scene.slug}/${slot.id} scale positive`);
        }

        const families = reusableKitFamilies(scene, assetsById);
        assert.ok(families.has('water'), `${scene.slug} keeps reused water family`);
        assert.ok(families.has('wall'), `${scene.slug} keeps reused wall family`);
        assert.ok(families.has('foliage'), `${scene.slug} keeps reused foliage family`);
        assert.ok(families.has('terrain_patch'), `${scene.slug} keeps reused terrain family`);
    }

    assert.equal(slugs.size, 10, 'generated slugs are unique');
    assert.equal(locationIds.size, 10, 'generated location ids are unique');
    assert.equal(signatures.size, 10, 'generated composition signatures are unique');
    assert.ok(
        pack.summary.variants.every((variant) => variant.changed_layer_count >= 12),
        'each variant changes a meaningful number of compositor layers',
    );
});

test('generated village variants pass the scene atom audit hook', async () => {
    const inputs = await loadVillageVariantInputs();
    const pack = generateVillageVariantPack(inputs);
    const { dir, scenesPath } = await writeTempPack(pack);
    try {
        for (const variant of pack.summary.variants) {
            const result = await auditSceneAtoms({
                slug: variant.slug,
                scenesPath,
                modDir,
                requiredReusableKitKinds: ['water', 'wall', 'foliage', 'terrain_patch'],
                minKitLayers: 32,
                minReusedKitAssets: 4,
            });
            assert.equal(result.ok, true, `${variant.slug}: ${result.failures?.join('; ')}`);
            assert.equal(result.summary.blankAtoms.length, 0, `${variant.slug} has no blank atoms`);
            assert.equal(
                result.summary.suspiciousFullStageAtoms.length,
                0,
                `${variant.slug} has no suspicious full-stage atoms`,
            );
        }
    } finally {
        await rm(dir, { recursive: true, force: true });
    }
});
