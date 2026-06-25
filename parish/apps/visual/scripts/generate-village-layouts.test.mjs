import test from 'node:test';
import assert from 'node:assert/strict';
import { access, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { auditSceneAtoms, parsePng, visibleContentSummary } from './audit-scene-atoms.mjs';
import {
    assertGeneratedPack,
    assertTerrainChunkMap,
    generateTerrainChunkMap,
    generateVillageLayoutPack,
    generateVillageLayoutPackWithChunkSprites,
    generateVillageLayoutPackWithTerrainChunks,
    generateVillageLayoutPackWithRasters,
    loadVillageLayoutInputs,
    sceneSignature,
    terrainChunkGrammarForRecipe,
    terrainSignature,
    topologySignature,
    validateOutdoorLayout,
    writeTerrainChunkSpriteAssets,
    writeTerrainRasterAssets,
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
    assert.equal(pack.summary.terrain_profile_count, 10);
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
    const terrainProfiles = new Set();
    const terrainSignatures = new Set();
    for (const [index, scene] of pack.scenes.entries()) {
        const layoutSummary = pack.summary.layouts[index];
        slugs.add(scene.slug);
        locationIds.add(scene.location_id);
        sceneSignatures.add(sceneSignature(scene));
        topologySignatures.add(layoutSummary.topology_signature);
        terrainProfiles.add(layoutSummary.terrain_profile);
        terrainSignatures.add(layoutSummary.terrain_signature);

        assert.deepEqual(scene.native_size, [1280, 720], scene.slug);
        assert.equal(scene.plate, sourceScene.plate, `${scene.slug} keeps legacy plate`);
        assert.equal(scene.underlay, sourceScene.underlay, `${scene.slug} keeps legacy underlay`);
        assert.ok(scene.layers.length >= 115, `${scene.slug} has enough layers to be a terrain compositor scene`);
        assert.ok(layoutSummary.kit_layer_count >= 60, `${scene.slug} uses many kit atoms`);
        assert.ok(layoutSummary.terrain_layer_count >= 70, `${scene.slug} has generated terrain layers`);
        assert.ok(
            layoutSummary.terrain_underpaint_layer_count >= 45,
            `${scene.slug} has enough generated terrain underpaint layers`,
        );
        assert.ok(layoutSummary.terrain_profile, `${scene.slug} has a terrain profile`);
        assert.ok(layoutSummary.terrain_profile_name, `${scene.slug} has a terrain profile name`);
        assert.equal(layoutSummary.terrain_signature, terrainSignature(inputs.recipe.layouts[index], {
            id: layoutSummary.terrain_profile,
            ...inputs.recipe.terrain_profiles[layoutSummary.terrain_profile],
        }));
        assert.ok(
            layoutSummary.shared_ground_base_opacity <= 0.16,
            `${scene.slug} demotes the shared ground base below visual dominance`,
        );
        assert.equal(layoutSummary.shared_ground_base_layer_count, 1, `${scene.slug} keeps at most one calibration ground base`);
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
        const calibrationBase = scene.layers.find((layer) => layer.asset === 'kilteevan-ground-base');
        assert.ok(calibrationBase, `${scene.slug} keeps a low-opacity calibration base`);
        assert.equal(calibrationBase.id, 'terrain-ground-calibration', `${scene.slug} names the base as calibration`);
        assert.ok((calibrationBase.opacity ?? 1) <= 0.16, `${scene.slug} calibration base opacity is low`);
        assert.ok(scene.layers.some((layer) => layer.id.startsWith('terrain-ground-')), `${scene.slug} has ground underpaint`);
        assert.ok(scene.layers.some((layer) => layer.id.startsWith('terrain-path-')), `${scene.slug} has path underpaint`);
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
    assert.equal(terrainProfiles.size, 10, 'generated terrain profiles are unique');
    assert.equal(terrainSignatures.size, 10, 'generated terrain signatures are unique');
    assert.ok(pack.summary.layouts.some((layout) => layout.topology.bridge_count === 0), 'some layouts are dry villages');
    assert.ok(pack.summary.layouts.some((layout) => layout.topology.bridge_count > 0), 'some layouts require bridges');
});

test('outdoor village terrain profiles reject samey or dominant-base generation', async () => {
    const inputs = await loadVillageLayoutInputs();

    const duplicateProfile = clone(inputs.recipe);
    duplicateProfile.layouts[1].terrain_profile = duplicateProfile.layouts[0].terrain_profile;
    assert.throws(
        () => generateVillageLayoutPack({ sceneIndex: inputs.sceneIndex, recipe: duplicateProfile }),
        /duplicate terrain profile/,
    );

    const dominantBase = clone(inputs.recipe);
    dominantBase.terrain_profiles[dominantBase.layouts[0].terrain_profile].base_opacity = 0.42;
    assert.throws(
        () => generateVillageLayoutPack({ sceneIndex: inputs.sceneIndex, recipe: dominantBase }),
        /base_opacity must be 0.05..0.28/,
    );

    const missingProfile = clone(inputs.recipe);
    missingProfile.layouts[0].terrain_profile = 'missing-terrain-profile';
    assert.throws(
        () => generateVillageLayoutPack({ sceneIndex: inputs.sceneIndex, recipe: missingProfile }),
        /missing terrain profile/,
    );
});

test('outdoor village raster generation writes deterministic full-stage terrain assets', async () => {
    const inputs = await loadVillageLayoutInputs();
    const first = generateVillageLayoutPackWithRasters(inputs, { terrainRasterBasePath: 'generated-assets' });
    const second = generateVillageLayoutPackWithRasters(inputs, { terrainRasterBasePath: 'generated-assets' });
    const { pack, terrainRasters } = first;
    const assetIds = new Set(pack.assets.map((asset) => asset.id));
    const tempDir = await mkdtemp(path.join(os.tmpdir(), 'rundale-village-rasters-'));
    const assetOutPath = path.join(tempDir, 'generated-assets');

    try {
        const writeResult = await writeTerrainRasterAssets(terrainRasters, assetOutPath);
        assert.equal(writeResult.count, 10);
        assert.equal(terrainRasters.length, 10);
        assert.equal(second.terrainRasters.length, terrainRasters.length);
        assert.equal(pack.summary.layout_count, 10);
        assert.equal(pack.assets.filter((asset) => asset.generated).length, 10);

        const rasterSignatures = new Set();
        for (const [index, raster] of terrainRasters.entries()) {
            const again = second.terrainRasters[index];
            const layoutSummary = pack.summary.layouts[index];
            const scene = pack.scenes[index];
            const filePath = path.join(assetOutPath, raster.fileName);
            const fileBuffer = await readFile(filePath);
            const image = parsePng(fileBuffer);
            const content = visibleContentSummary(image);

            assert.equal(raster.fileName, again.fileName, `${layoutSummary.slug} stable raster file name`);
            assert.equal(raster.pixelHash, again.pixelHash, `${layoutSummary.slug} stable raster hash`);
            assert.equal(raster.png.equals(again.png), true, `${layoutSummary.slug} stable raster bytes`);
            assert.equal(fileBuffer.equals(raster.png), true, `${layoutSummary.slug} writes expected PNG bytes`);
            assert.equal(assetIds.has(raster.asset.id), true, `${layoutSummary.slug} raster asset is in pack`);
            assert.equal(raster.asset.generated, true, `${layoutSummary.slug} marks raster asset as generated`);
            assert.match(raster.asset.image, /^generated-assets\/[a-z0-9-]+\.png$/, `${layoutSummary.slug} raster image is relative`);
            assert.equal(scene.layers[0].id, 'terrain-raster', `${layoutSummary.slug} uses raster as the first floor layer`);
            assert.equal(scene.layers[0].asset, raster.asset.id, `${layoutSummary.slug} first layer references generated raster`);
            assert.equal(scene.layers.some((layer) => layer.id === 'terrain-ground-calibration'), false);

            assert.equal(layoutSummary.terrain_raster_asset, raster.asset.id);
            assert.equal(layoutSummary.terrain_raster_layer_count, 1);
            assert.equal(layoutSummary.terrain_underpaint_layer_count, 0);
            assert.equal(layoutSummary.repeated_terrain_atom_count, 0);
            assert.equal(layoutSummary.shared_ground_base_opacity, 0);
            assert.equal(layoutSummary.shared_ground_base_layer_count, 0);
            assert.deepEqual(layoutSummary.terrain_raster_size, [1280, 720]);
            assert.equal(layoutSummary.terrain_pixel_hash, raster.pixelHash);
            assert.ok(layoutSummary.raster_path_pixel_count > 0, `${layoutSummary.slug} paints path pixels`);
            assert.ok(layoutSummary.raster_bank_pixel_count >= 0, `${layoutSummary.slug} reports bank pixels`);
            assert.ok(layoutSummary.raster_vegetation_pixel_count > 0, `${layoutSummary.slug} paints vegetation speckles`);
            assert.equal(layoutSummary.raster_path_coverage_cells, layoutSummary.topology.grid.road_cell_count);
            assert.equal(layoutSummary.raster_water_coverage_cells, layoutSummary.topology.grid.water_cell_count);
            if (layoutSummary.topology.waterway_count > 0) {
                assert.ok(layoutSummary.raster_water_pixel_count > 0, `${layoutSummary.slug} paints water pixels`);
            } else {
                assert.equal(layoutSummary.raster_water_pixel_count, 0, `${layoutSummary.slug} stays dry`);
            }
            assert.equal(image.width, 1280);
            assert.equal(image.height, 720);
            assert.equal(content.alphaCoverage, 1, `${layoutSummary.slug} raster is a complete floor image`);
            assert.deepEqual(content.bbox, { x: 0, y: 0, width: 1280, height: 720 });
            assert.equal(rasterSignatures.has(layoutSummary.terrain_raster_signature), false);
            rasterSignatures.add(layoutSummary.terrain_raster_signature);
        }
    } finally {
        await rm(tempDir, { recursive: true, force: true });
    }

    const missingAsset = clone(pack);
    missingAsset.assets = missingAsset.assets.filter((asset) => asset.id !== pack.summary.layouts[0].terrain_raster_asset);
    assert.throws(() => assertGeneratedPack(missingAsset), /missing .*asset/);

    const duplicateRasterSignature = clone(pack);
    duplicateRasterSignature.summary.layouts[1].terrain_raster_signature =
        duplicateRasterSignature.summary.layouts[0].terrain_raster_signature;
    assert.throws(() => assertGeneratedPack(duplicateRasterSignature), /duplicate terrain raster signature/);
});

test('outdoor village terrain chunk grammar emits deterministic connected chunk maps', async () => {
    const inputs = await loadVillageLayoutInputs();
    const first = generateVillageLayoutPackWithTerrainChunks(inputs);
    const second = generateVillageLayoutPackWithTerrainChunks(inputs);
    const grammar = terrainChunkGrammarForRecipe(inputs.recipe);
    const chunkSignatures = new Set();

    assert.equal(first.chunkMaps.length, 10);
    assert.deepEqual(first.chunkMaps, second.chunkMaps);
    assert.deepEqual(first.chunkGrammar, grammar);

    for (const [index, chunkMap] of first.chunkMaps.entries()) {
        const layoutSummary = first.pack.summary.layouts[index];
        const classCounts = chunkMap.summary.class_counts;

        assertTerrainChunkMap(chunkMap, { grammar });
        assert.equal(chunkMap.layout_id, inputs.recipe.layouts[index].id);
        assert.equal(chunkMap.grid.cols, 24);
        assert.equal(chunkMap.grid.rows, 18);
        assert.equal(classCounts.ground, chunkMap.grid.cols * chunkMap.grid.rows, `${chunkMap.layout_id} covers every ground cell`);
        assert.equal(classCounts.water || 0, layoutSummary.topology.grid.water_cell_count, `${chunkMap.layout_id} water chunk coverage`);
        assert.ok((classCounts.path || 0) > 0, `${chunkMap.layout_id} has path chunks`);
        assert.equal(chunkMap.summary.path_port_components, 1, `${chunkMap.layout_id} walkable chunks connect`);
        assert.equal(
            chunkMap.summary.water_port_components,
            layoutSummary.topology.waterway_count,
            `${chunkMap.layout_id} water chunks connect per waterway`,
        );
        assert.equal(chunkMap.summary.collision_count, 0, `${chunkMap.layout_id} chunk masks have no collisions`);
        assert.equal(layoutSummary.terrain_chunk_count, chunkMap.summary.chunk_count);
        assert.equal(layoutSummary.terrain_chunk_map_signature, chunkMap.chunk_map_signature);
        assert.equal(layoutSummary.terrain_chunk_grammar_signature, chunkMap.grammar_signature);
        assert.equal(layoutSummary.terrain_chunk_collision_count, 0);
        assert.equal(chunkSignatures.has(chunkMap.chunk_map_signature), false, `${chunkMap.layout_id} chunk signature is unique`);
        chunkSignatures.add(chunkMap.chunk_map_signature);

        if (layoutSummary.topology.bridge_count > 0) {
            assert.ok((classCounts.bridge || 0) > 0, `${chunkMap.layout_id} bridge layouts declare bridge chunks`);
            assert.ok(
                chunkMap.summary.bridge_under_span_cell_count >= layoutSummary.topology.bridge_count,
                `${chunkMap.layout_id} bridge records include water under-span cells`,
            );
        } else {
            assert.equal(classCounts.bridge || 0, 0, `${chunkMap.layout_id} dry layouts have no bridge chunks`);
        }

        for (const chunk of chunkMap.chunks) {
            assert.ok(chunk.id, `${chunkMap.layout_id} chunk has id`);
            assert.ok(chunk.template, `${chunkMap.layout_id}/${chunk.id} has template`);
            assert.ok(grammar.templates[chunk.template], `${chunkMap.layout_id}/${chunk.id} template exists`);
            assert.equal(Array.isArray(chunk.ports), true, `${chunkMap.layout_id}/${chunk.id} has ports`);
            assert.equal(typeof chunk.mask.water, 'boolean', `${chunkMap.layout_id}/${chunk.id} has water mask`);
            assert.equal(typeof chunk.mask.walkable, 'boolean', `${chunkMap.layout_id}/${chunk.id} has walkable mask`);
            assert.equal(typeof chunk.mask.blocks_objects, 'boolean', `${chunkMap.layout_id}/${chunk.id} has object mask`);
            assert.match(chunk.variant_seed, /^[a-f0-9]{16}$/);
        }
    }
});

test('outdoor village chunk sprite renderer writes deterministic reusable terrain assets', async () => {
    const inputs = await loadVillageLayoutInputs();
    const first = generateVillageLayoutPackWithChunkSprites(inputs, { terrainAssetBasePath: 'generated-assets' });
    const second = generateVillageLayoutPackWithChunkSprites(inputs, { terrainAssetBasePath: 'generated-assets' });
    const { pack, terrainGroundFills, chunkMaps, terrainChunkSprites } = first;
    const tempDir = await mkdtemp(path.join(os.tmpdir(), 'rundale-village-chunk-sprites-'));
    const assetOutPath = path.join(tempDir, 'generated-assets');
    const assetIds = new Set(pack.assets.map((asset) => asset.id));
    const totalChunkSpriteLayers = pack.summary.layouts.reduce((sum, layout) => sum + layout.terrain_chunk_sprite_layer_count, 0);

    try {
        const writeResult = await writeTerrainChunkSpriteAssets([...terrainGroundFills, ...terrainChunkSprites], assetOutPath);
        assert.equal(writeResult.count, terrainChunkSprites.length + terrainGroundFills.length);
        assert.equal(chunkMaps.length, 10);
        assert.equal(second.terrainChunkSprites.length, terrainChunkSprites.length);
        assert.equal(second.terrainGroundFills.length, terrainGroundFills.length);
        assert.equal(pack.summary.layout_count, 10);
        assert.equal(terrainGroundFills.length, 10);
        assert.ok(terrainChunkSprites.length > 20, 'chunk sprite catalog has many reusable assets');
        assert.ok(
            terrainChunkSprites.length < totalChunkSpriteLayers,
            'chunk sprite catalog is reused across more layers than assets',
        );
        assert.equal(pack.assets.filter((asset) => asset.generated).length, terrainChunkSprites.length + terrainGroundFills.length);

        const groundFillSignatures = new Set();
        for (const [index, fill] of terrainGroundFills.entries()) {
            const again = second.terrainGroundFills[index];
            const filePath = path.join(assetOutPath, fill.fileName);
            const fileBuffer = await readFile(filePath);
            const image = parsePng(fileBuffer);
            const content = visibleContentSummary(image);

            assert.equal(fill.asset.id, again.asset.id, `${fill.asset.id} has stable id`);
            assert.equal(fill.pixelHash, again.pixelHash, `${fill.asset.id} has stable hash`);
            assert.equal(fill.png.equals(again.png), true, `${fill.asset.id} has stable bytes`);
            assert.equal(fileBuffer.equals(fill.png), true, `${fill.asset.id} writes expected PNG bytes`);
            assert.equal(fill.asset.kind, 'ground');
            assert.equal(fill.asset.generated, true);
            assert.equal(fill.asset.terrain_ground_fill, true);
            assert.match(fill.asset.image, /^generated-assets\/ground\/[a-z0-9-]+\.png$/);
            assert.equal(image.width, 1280);
            assert.equal(image.height, 720);
            assert.equal(content.alphaCoverage, 1, `${fill.asset.id} is a complete floor image`);
            assert.equal(groundFillSignatures.has(fill.pixelHash), false, `${fill.asset.id} has unique ground fill hash`);
            groundFillSignatures.add(fill.pixelHash);
        }

        const spriteHashes = new Set();
        for (const [index, sprite] of terrainChunkSprites.entries()) {
            const again = second.terrainChunkSprites[index];
            const filePath = path.join(assetOutPath, sprite.fileName);
            const fileBuffer = await readFile(filePath);
            const image = parsePng(fileBuffer);
            const content = visibleContentSummary(image);

            assert.equal(sprite.asset.id, again.asset.id, `${sprite.asset.id} has stable id`);
            assert.equal(sprite.pixelHash, again.pixelHash, `${sprite.asset.id} has stable hash`);
            assert.equal(sprite.png.equals(again.png), true, `${sprite.asset.id} has stable bytes`);
            assert.equal(fileBuffer.equals(sprite.png), true, `${sprite.asset.id} writes expected PNG bytes`);
            assert.equal(assetIds.has(sprite.asset.id), true, `${sprite.asset.id} is in pack`);
            assert.equal(sprite.asset.generated, true, `${sprite.asset.id} marks asset as generated`);
            assert.match(sprite.asset.image, /^generated-assets\/chunks\/generated-terrain-chunk-[a-z0-9-]+\.png$/);
            assert.match(sprite.asset.kind, /^terrain_chunk_(bank|bridge|detail|path|water)$/);
            assert.deepEqual(sprite.asset.anchor, [50, 50]);
            assert.equal(image.width, 78);
            assert.equal(image.height, 54);
            assert.ok(content.alphaCoverage > 0.02, `${sprite.asset.id} has visible pixels`);
            spriteHashes.add(sprite.pixelHash);
        }
        assert.ok(spriteHashes.size > 20, 'chunk sprite catalog has many visually distinct PNGs');

        for (const [index, scene] of pack.scenes.entries()) {
            const layoutSummary = pack.summary.layouts[index];
            const chunkMap = chunkMaps[index];
            const chunkLayers = scene.layers.filter((layer) => layer.terrain_chunk_id);
            const chunkIds = new Set(chunkLayers.map((layer) => layer.terrain_chunk_id));

            assert.equal(layoutSummary.terrain_chunk_render_mode, 'sprites');
            assert.equal(scene.layers[0].id, 'terrain-ground-fill', `${scene.slug} starts with generated ground fill`);
            assert.equal(layoutSummary.terrain_ground_fill_layer_count, 1);
            assert.ok(layoutSummary.terrain_ground_fill_asset, `${scene.slug} records ground fill asset`);
            assert.ok(layoutSummary.terrain_ground_fill_signature, `${scene.slug} records ground fill signature`);
            assert.equal(scene.layers.some((layer) => layer.id === 'terrain-raster'), false, `${scene.slug} has no raster layer`);
            assert.equal(layoutSummary.terrain_raster_asset, undefined);
            assert.equal(layoutSummary.terrain_raster_layer_count, 0);
            assert.equal(layoutSummary.terrain_chunk_sprite_missing_assets, 0);
            assert.equal(chunkLayers.length, layoutSummary.terrain_chunk_sprite_layer_count);
            assert.ok(chunkLayers.length >= 45, `${scene.slug} has many visible chunk sprite layers`);
            assert.ok(
                chunkLayers.length < chunkMap.summary.chunk_count,
                `${scene.slug} does not render every ground-fill chunk as a sprite`,
            );
            assert.equal(chunkIds.size, chunkLayers.length, `${scene.slug} has one visible layer per non-ground chunk`);
            assert.equal(layoutSummary.terrain_chunk_sprite_path_coverage_cells, layoutSummary.terrain_chunk_class_counts.path || 0);
            assert.equal(layoutSummary.terrain_chunk_sprite_water_coverage_cells, layoutSummary.terrain_chunk_class_counts.water || 0);
            assert.equal(layoutSummary.terrain_chunk_sprite_collision_count, 0);
            assert.equal(layoutSummary.terrain_chunk_sprite_bridge_under_span_cell_count, layoutSummary.terrain_chunk_bridge_under_span_cell_count);
            assert.ok(layoutSummary.terrain_chunk_sprite_signature, `${scene.slug} has chunk sprite signature`);
            assert.equal(layoutSummary.terrain_chunk_map_signature, chunkMap.chunk_map_signature);

            const classCounts = {};
            for (const layer of chunkLayers) {
                classCounts[layer.terrain_chunk_class] = (classCounts[layer.terrain_chunk_class] || 0) + 1;
                const asset = pack.assets.find((candidate) => candidate.id === layer.asset);
                assert.ok(asset, `${scene.slug}/${layer.id} has generated chunk asset`);
                assert.equal(asset.kind, `terrain_chunk_${layer.terrain_chunk_class}`);
                assert.equal(asset.terrain_chunk_template, layer.terrain_chunk_template);
                assert.deepEqual(asset.terrain_chunk_ports, layer.terrain_chunk_ports);
                assert.equal(typeof layer.terrain_chunk_mask.water, 'boolean');
                assert.match(layer.terrain_chunk_variant_seed, /^[a-f0-9]{16}$/);
            }
            assert.deepEqual(classCounts, layoutSummary.terrain_chunk_sprite_class_counts);
        }
    } finally {
        await rm(tempDir, { recursive: true, force: true });
    }

    const missingChunkAsset = clone(pack);
    const firstChunkLayer = missingChunkAsset.scenes[0].layers.find((layer) => layer.terrain_chunk_id);
    missingChunkAsset.assets = missingChunkAsset.assets.filter((asset) => asset.id !== firstChunkLayer.asset);
    missingChunkAsset.summary.layouts[0].terrain_chunk_sprite_missing_assets = 1;
    assert.throws(() => assertGeneratedPack(missingChunkAsset), /missing terrain chunk sprite assets|references missing/);

    const duplicateChunkLayerSource = clone(pack);
    const duplicateLayers = duplicateChunkLayerSource.scenes[0].layers.filter((layer) => layer.terrain_chunk_id);
    duplicateLayers[1].terrain_chunk_id = duplicateLayers[0].terrain_chunk_id;
    assert.throws(() => assertGeneratedPack(duplicateChunkLayerSource), /duplicate terrain chunk layer source/);

    const brokenCoverage = clone(pack);
    brokenCoverage.summary.layouts[0].terrain_chunk_sprite_path_coverage_cells += 1;
    assert.throws(() => assertGeneratedPack(brokenCoverage), /chunk sprite path coverage mismatch/);
});

test('outdoor village terrain chunk validator rejects broken chunk contracts', async () => {
    const inputs = await loadVillageLayoutInputs();
    const grammar = terrainChunkGrammarForRecipe(inputs.recipe);
    const { chunkMaps } = generateVillageLayoutPackWithTerrainChunks(inputs);
    const validMap = chunkMaps.find((map) => map.bridge_records.length > 0);
    assert.ok(validMap, 'fixture has a bridge chunk map');

    const duplicateChunk = clone(validMap);
    duplicateChunk.chunks[1].id = duplicateChunk.chunks[0].id;
    assert.throws(() => assertTerrainChunkMap(duplicateChunk, { grammar }), /duplicate terrain chunk id/);

    const missingTemplateGrammar = clone(grammar);
    const usedTemplate = validMap.chunks.find((chunk) => chunk.class === 'path').template;
    delete missingTemplateGrammar.templates[usedTemplate];
    assert.throws(() => assertTerrainChunkMap(validMap, { grammar: missingTemplateGrammar }), /missing terrain chunk template/);

    const brokenPorts = clone(validMap);
    const portedChunk = brokenPorts.chunks.find((chunk) => chunk.class === 'water' && chunk.ports.length > 0);
    portedChunk.ports = [];
    assert.throws(() => assertTerrainChunkMap(brokenPorts, { grammar }), /port mismatch/);

    const brokenBridge = clone(validMap);
    brokenBridge.bridge_records[0].under_span_cells = [];
    assert.throws(() => assertTerrainChunkMap(brokenBridge, { grammar }), /water under-span/);

    const wetCartFootprint = clone(inputs.recipe.layouts.find((layout) => layout.id === 'bridge-hamlet'));
    wetCartFootprint.nodes.cart = [24, 74];
    wetCartFootprint.props = [{ id: 'cart', kind: 'cart', node: 'cart', flip: false }];
    assert.throws(
        () =>
            generateTerrainChunkMap({
                layout: wetCartFootprint,
                recipe: inputs.recipe,
                grid: inputs.recipe.grid,
                visualWaterExclusions: inputs.recipe.visual_water_exclusions,
            }),
        /grid footprint intersects rendered water|footprint is in water/,
    );
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
