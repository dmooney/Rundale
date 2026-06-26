import test from 'node:test';
import assert from 'node:assert/strict';

import {
    buildPlateManifest,
    buildPlatePrompt,
    readPlateSpec,
    validatePlateSpec,
} from './generate-kilteevan-plate-manifest.mjs';

test('Kilteevan generated plate spec validates topology and prompt constraints', async () => {
    const spec = await readPlateSpec();
    const validation = validatePlateSpec(spec);
    const prompt = buildPlatePrompt(spec);

    assert.equal(validation.ok, true, validation.errors.join('\n'));
    assert.equal(validation.road_node_count, 8);
    assert.equal(validation.road_edge_count, 7);
    assert.equal(validation.stream_point_count, 6);
    assert.equal(validation.hotspot_count, 4);
    assert.match(prompt, /high 3\/4 isometric/i);
    assert.match(prompt, /continuous stream/i);
    assert.match(prompt, /bridge directly over the stream/i);
    assert.match(prompt, /cart fully on dry ground/i);
    assert.match(prompt, /smoke emerges from chimney openings/i);
    assert.match(prompt, /prominent wooden wayfinding signpost/i);
    assert.match(prompt, /dynamic NPC sprites overlaid later/i);
    assert.match(prompt, /no props over water/i);
    assert.match(prompt, /no baked people or NPCs/i);
});

test('Kilteevan generated plate manifest references the committed 1280x720 PNG', async () => {
    const spec = await readPlateSpec();
    const manifest = await buildPlateManifest(spec);

    assert.equal(manifest.id, 'visual-generated-kilteevan-plate-m9');
    assert.equal(manifest.asset.id, 'kilteevan-m9-full-scene-base');
    assert.equal(manifest.asset.kind, 'plate');
    assert.equal(manifest.asset.image, 'assets/scenes/kilteevan-village/generated/m9-full-scene-base.png');
    assert.equal(
        manifest.image_validation.path,
        'mods/rundale/assets/scenes/kilteevan-village/generated/m9-full-scene-base.png',
    );
    assert.equal(manifest.image_validation.width, 1280);
    assert.equal(manifest.image_validation.height, 720);
    assert.equal(manifest.image_validation.native_size_matches, true);
    assert.match(manifest.image_validation.sha256, /^[a-f0-9]{64}$/);
    assert.match(manifest.spec_sha256, /^[a-f0-9]{64}$/);
    assert.equal(manifest.validation.ok, true, manifest.validation.errors.join('\n'));
});
