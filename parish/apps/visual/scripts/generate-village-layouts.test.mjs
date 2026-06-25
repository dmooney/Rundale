import test from 'node:test';
import assert from 'node:assert/strict';
import { access, mkdtemp, rm, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { auditSceneAtoms } from './audit-scene-atoms.mjs';
import {
    generateVillageLayoutPack,
    loadVillageLayoutInputs,
    sceneSignature,
    topologySignature,
    validateOutdoorLayout,
} from './generate-village-layouts.mjs';

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, '../../../..');
const modDir = path.join(repoRoot, 'mods/rundale');

function clone(value) {
    return JSON.parse(JSON.stringify(value));
}

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
    const dir = await mkdtemp(path.join(os.tmpdir(), 'rundale-village-layouts-'));
    const scenesPath = path.join(dir, 'generated-scenes.json');
    await writeFile(scenesPath, `${JSON.stringify(pack, null, 2)}\n`);
    return { dir, scenesPath };
}

test('outdoor village layout recipe generates ten physically coherent compositor scenes', async () => {
    const inputs = await loadVillageLayoutInputs();
    const pack = generateVillageLayoutPack(inputs);
    const assetsById = new Map(pack.assets.map((asset) => [asset.id, asset]));
    const sourceScene = inputs.sceneIndex.scenes.find((scene) => scene.slug === inputs.recipe.source_slug);

    assert.equal(inputs.recipe.layouts.length, 10);
    assert.equal(pack.summary.layout_count, 10);
    assert.equal(pack.scenes.length, 10);
    assert.equal(pack.source.source_slug, 'kilteevan-village');
    assert.equal(pack.source.source_location_id, 15);
    assert.match(pack.summary.ai_asset_strategy.anchor_contract, /anchor/);
    assert.match(pack.summary.ai_asset_strategy.npc_atom_families.join(' '), /shawl/);
    assert.deepEqual(pack.summary.grid.cols, 24);
    assert.deepEqual(pack.summary.grid.rows, 18);
    assert.ok(pack.summary.prefab_catalog_ids.includes('bridge-crossing'));
    assert.ok(pack.summary.prefab_catalog_ids.includes('cart-pullout'));
    assert.equal(pack.summary.visual_water_exclusion_count, 1);

    const slugs = new Set();
    const locationIds = new Set();
    const sceneSignatures = new Set();
    const topologySignatures = new Set();
    for (const [index, scene] of pack.scenes.entries()) {
        const layoutSummary = pack.summary.layouts[index];
        slugs.add(scene.slug);
        locationIds.add(scene.location_id);
        sceneSignatures.add(sceneSignature(scene));
        topologySignatures.add(layoutSummary.topology_signature);

        assert.deepEqual(scene.native_size, [1280, 720], scene.slug);
        assert.equal(scene.plate, sourceScene.plate, `${scene.slug} keeps legacy plate`);
        assert.equal(scene.underlay, sourceScene.underlay, `${scene.slug} keeps legacy underlay`);
        assert.ok(scene.layers.length >= 75, `${scene.slug} has enough layers to be a compositor scene`);
        assert.ok(layoutSummary.kit_layer_count >= 60, `${scene.slug} uses many kit atoms`);
        assert.equal(layoutSummary.topology.ok, true, `${scene.slug} topology validates`);
        assert.equal(layoutSummary.topology_signature, topologySignature(inputs.recipe.layouts[index]));
        assert.ok(layoutSummary.activation_hints.some((hint) => hint.kind === 'travel' && hint.command), `${scene.slug} has travel command hints`);
        assert.equal(layoutSummary.topology.grid.grid_cell_count, 432, `${scene.slug} validates on the hidden iso grid`);
        assert.equal(layoutSummary.topology.grid.road_components, 1, `${scene.slug} has connected walkable terrain cells`);
        assert.equal(
            layoutSummary.topology.grid.water_components,
            layoutSummary.topology.waterway_count,
            `${scene.slug} has connected water terrain cells per waterway`,
        );
        assert.equal(
            layoutSummary.topology.grid.invalid_freeform_placements,
            0,
            `${scene.slug} has no placements outside grid/prefab resolution`,
        );
        assert.equal(
            layoutSummary.topology.grid.rendered_water_collision_failures,
            0,
            `${scene.slug} has no prop footprint collisions with rendered water`,
        );
        assert.ok(layoutSummary.topology.grid.prefab_port_connections > 0, `${scene.slug} records prefab port connections`);

        const zValues = new Set();
        for (const layer of scene.layers) {
            assert.equal(zValues.has(layer.z), false, `${scene.slug}/${layer.id} duplicate z=${layer.z}`);
            zValues.add(layer.z);
            const asset = assetsById.get(layer.asset);
            assert.ok(asset, `${scene.slug}/${layer.id} has asset ${layer.asset}`);
            assert.equal(asset.image.endsWith('.png'), true, `${scene.slug}/${layer.id} is PNG`);
            assert.equal(asset.image.endsWith('.svg'), false, `${scene.slug}/${layer.id} is not SVG`);
            assert.match(asset.image, /^assets\/scenes\/kilteevan-village\/atoms\//);
            await access(path.join(modDir, asset.image));
        }

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
        assert.ok(families.has('road'), `${scene.slug} reuses road atoms`);
        assert.ok(families.has('wall'), `${scene.slug} reuses wall atoms`);
        assert.ok(families.has('foliage'), `${scene.slug} reuses foliage atoms`);
        assert.ok(families.has('terrain_patch'), `${scene.slug} reuses terrain atoms`);

        const layout = inputs.recipe.layouts[index];
        for (const waterway of layout.waterways || []) {
            for (let segmentIndex = 0; segmentIndex < waterway.points.length - 1; segmentIndex += 1) {
                assert.ok(
                    scene.layers.some((layer) => layer.id === `${waterway.id}-ribbon-${segmentIndex}`),
                    `${scene.slug}/${waterway.id} segment ${segmentIndex} has a visual continuity ribbon`,
                );
            }
        }

        for (const site of layout.cottage_sites) {
            assert.ok(site.chimney_opening, `${scene.slug}/${site.id} declares chimney opening socket`);
            const smoke = scene.layers.find((layer) => layer.id === `${site.id}-smoke`);
            assert.ok(smoke, `${scene.slug}/${site.id} has smoke layer`);
            assert.equal(smoke.x, site.chimney_opening[0], `${scene.slug}/${site.id} smoke x uses chimney opening`);
            assert.equal(smoke.y, site.chimney_opening[1], `${scene.slug}/${site.id} smoke y uses chimney opening`);
        }
    }

    assert.equal(slugs.size, 10, 'generated slugs are unique');
    assert.equal(locationIds.size, 10, 'generated location ids are unique');
    assert.equal(sceneSignatures.size, 10, 'generated scene signatures are unique');
    assert.equal(topologySignatures.size, 10, 'generated topology signatures are unique');
    assert.ok(pack.summary.layouts.some((layout) => layout.topology.bridge_count === 0), 'some layouts are dry villages');
    assert.ok(pack.summary.layouts.some((layout) => layout.topology.bridge_count > 0), 'some layouts require bridges');
});

test('outdoor village layout validator rejects impossible village topology', async () => {
    const inputs = await loadVillageLayoutInputs();
    const valid = inputs.recipe.layouts.find((layout) => layout.id === 'bridge-hamlet');
    assert.equal(validateOutdoorLayout(valid).ok, true);

    const disconnected = clone(valid);
    disconnected.paths = disconnected.paths.filter((pathDef) => pathDef.id !== 'crossroads-road');
    assert.throws(() => validateOutdoorLayout(disconnected), /unreachable from entry|disconnected/);

    const missingBridge = clone(valid);
    missingBridge.bridges = [];
    assert.throws(() => validateOutdoorLayout(missingBridge), /without a bridge/);

    const badBridge = clone(valid);
    badBridge.bridges[0].node = 'well';
    assert.throws(() => validateOutdoorLayout(badBridge), /bridge 'brook-bridge'/);

    const wetSlot = clone(valid);
    wetSlot.npc_slots = [{ id: 'bad-water-slot', node: 'bridge-center', scale: 1 }];
    assert.throws(() => validateOutdoorLayout(wetSlot), /npc slot 'bad-water-slot' is in water/);

    const wetCartFootprint = clone(valid);
    wetCartFootprint.nodes.cart = [24, 74];
    wetCartFootprint.props = [{ id: 'cart', kind: 'cart', node: 'cart', flip: false }];
    assert.throws(() => validateOutdoorLayout(wetCartFootprint), /prop 'cart' footprint is in water/);

    const npcInsideCart = clone(valid);
    npcInsideCart.npc_slots = [{ id: 'inside-cart', node: 'cart', scale: 1 }];
    assert.throws(() => validateOutdoorLayout(npcInsideCart), /npc slot 'inside-cart' intersects prop 'cart' footprint/);

    assert.throws(
        () =>
            validateOutdoorLayout(valid, {
                visualWaterExclusions: [{ id: 'temporary-rendered-water-mask', rect: [50, 40, 25, 35] }],
            }),
        /prop 'cart' footprint is in water|prop 'cart' grid footprint intersects rendered water/,
    );

    const truncatedRiver = clone(valid);
    truncatedRiver.waterways[0].points = [[0, 74], [25, 73], [35.5, 73]];
    assert.throws(() => validateOutdoorLayout(truncatedRiver), /continuous water/);

    const unknownPrefab = clone(valid);
    unknownPrefab.props[0].prefab = 'missing-prefab';
    assert.throws(() => validateOutdoorLayout(unknownPrefab), /missing prefab/);

    const missingChimneySocket = clone(valid);
    delete missingChimneySocket.cottage_sites[0].chimney_opening;
    assert.throws(() => validateOutdoorLayout(missingChimneySocket), /missing chimney_opening/);
});

test('generated outdoor village layouts pass the scene atom audit hook', async () => {
    const inputs = await loadVillageLayoutInputs();
    const pack = generateVillageLayoutPack(inputs);
    const { dir, scenesPath } = await writeTempPack(pack);
    try {
        for (const layout of pack.summary.layouts) {
            const result = await auditSceneAtoms({
                slug: layout.slug,
                scenesPath,
                modDir,
                requiredReusableKitKinds: ['road', 'wall', 'foliage', 'terrain_patch'],
                minKitLayers: 48,
                minReusedKitAssets: 5,
            });
            assert.equal(result.ok, true, `${layout.slug}: ${result.failures?.join('; ')}`);
            assert.equal(result.summary.blankAtoms.length, 0, `${layout.slug} has no blank atoms`);
            assert.equal(
                result.summary.suspiciousFullStageAtoms.length,
                0,
                `${layout.slug} has no suspicious full-stage atoms`,
            );
        }
    } finally {
        await rm(dir, { recursive: true, force: true });
    }
});
