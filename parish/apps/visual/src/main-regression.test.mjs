import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

function pngDimensions(buffer) {
    assert.equal(buffer.toString('ascii', 1, 4), 'PNG');
    return {
        width: buffer.readUInt32BE(16),
        height: buffer.readUInt32BE(20),
    };
}

test('visual client does not mutate the diorama flag on scene refresh', async () => {
    const source = await readFile(new URL('./main.js', import.meta.url), 'utf8');
    assert.equal(source.includes('/flag enable diorama'), false);
});

test('visual client hides dashboard control words on first read', async () => {
    const html = await readFile(new URL('../index.html', import.meta.url), 'utf8');
    assert.equal(/>\s*(Settings|Server|Connect|Refresh)\s*</.test(html), false);
    assert.match(html, /<details class="log-panel">/);
    assert.match(html, /<details class="settings-panel">/);
});

test('visual client keeps text commands behind a fallback drawer', async () => {
    const html = await readFile(new URL('../index.html', import.meta.url), 'utf8');
    assert.match(html, /<details id="command-panel" class="command-panel">/);
    assert.match(html, /<summary aria-label="Command fallback"/);
    assert.match(
        html,
        /<details id="command-panel" class="command-panel">[\s\S]*<form id="command-form"/,
    );
    assert.equal(html.includes('\n                <form id="command-form"'), false);
});

test('visual client exposes a diegetic action prompt', async () => {
    const html = await readFile(new URL('../index.html', import.meta.url), 'utf8');
    const source = await readFile(new URL('./main.js', import.meta.url), 'utf8');
    const css = await readFile(new URL('./styles.css', import.meta.url), 'utf8');

    assert.match(html, /<div id="action-prompt" class="action-prompt" hidden>/);
    assert.match(html, /<button id="action-button" type="button"><\/button>/);
    assert.match(source, /verb:\s*'Go'/);
    assert.match(source, /verb:\s*'Look'/);
    assert.match(source, /verb:\s*'Talk'/);
    assert.match(source, /target\.value\.activation\?\.target_label/);
    assert.match(css, /\.action-prompt\s*\{/);
    assert.match(css, /\.command-panel\s*\{/);
});

test('visual client keeps the story log in a compact drawer', async () => {
    const css = await readFile(new URL('./styles.css', import.meta.url), 'utf8');
    assert.match(css, /\.log-panel\s*\{/);
    assert.match(css, /\.turn-log\s*\{[^}]*position:\s*absolute;/s);
    assert.equal(css.includes('minmax(260px, 360px)'), false);
});

test('visual client uses raster hotspot cue sprites instead of debug geometry', async () => {
    const source = await readFile(new URL('./pixi-renderer.js', import.meta.url), 'utf8');
    const travelCue = pngDimensions(
        await readFile(new URL('../assets/cues/travel-hover.png', import.meta.url)),
    );
    const inspectCue = pngDimensions(
        await readFile(new URL('../assets/cues/inspect-hover.png', import.meta.url)),
    );

    assert.deepEqual(travelCue, { width: 96, height: 42 });
    assert.deepEqual(inspectCue, { width: 64, height: 64 });
    assert.match(source, /HOTSPOT_CUE_ASSETS/);
    assert.match(source, /new PIXI\.Sprite\(texture\)/);
    assert.match(source, /hotspotCuePlacement/);
    assert.match(source, /focusX = this\.model\.slug === 'kilteevan-village' \? stageWidth \* 0\.42/);
    assert.equal(source.includes('function drawHotspotShape'), false);
    assert.equal(source.includes('function drawTravelHotspotCue'), false);
    assert.equal(source.includes('function drawTravelPathGlint'), false);
    assert.equal(source.includes('function drawInspectHotspotCue'), false);
    assert.equal(source.includes('const alpha = selected ? 0.28 : active ? 0.22 : 0'), false);
    assert.equal(source.includes('quadraticCurveTo'), false);
    assert.equal(source.includes('glintCount'), false);
    assert.equal(source.includes('markerY + size * 0.45'), false);
    assert.equal(source.includes('width * 0.28, height * 0.42'), false);
});

test('visual client uses a raster NPC selection cue instead of drawn highlight geometry', async () => {
    const source = await readFile(new URL('./pixi-renderer.js', import.meta.url), 'utf8');
    const npcCue = pngDimensions(
        await readFile(new URL('../assets/cues/npc-select.png', import.meta.url)),
    );

    assert.deepEqual(npcCue, { width: 80, height: 36 });
    assert.match(source, /NPC_CUE_ASSET/);
    assert.match(source, /loadNpcCueTexture/);
    assert.match(source, /new PIXI\.Sprite\(this\.npcCueTexture\)/);
    assert.equal(source.includes('const ring = new PIXI.Graphics'), false);
    assert.equal(source.includes('.ellipse(0, -7, 28, 10)'), false);
    assert.equal(source.includes('NPC_HIGHLIGHT_FILL'), false);
    assert.equal(source.includes('NPC_HIGHLIGHT_STROKE'), false);
});

test('canvas fallback first read does not expose debug scene overlays', async () => {
    const source = await readFile(new URL('./renderer.js', import.meta.url), 'utf8');

    assert.match(source, /function drawCanvasHotspotCue/);
    assert.match(source, /function drawCanvasNpcCue/);
    assert.match(source, /selectedHotspotId/);
    assert.match(source, /selectedNpcId/);
    assert.equal(source.includes('function drawSlots'), false);
    assert.equal(source.includes('ctx.fillText(slot.id'), false);
    assert.equal(source.includes('ctx.fillText(hotspot.label'), false);
    assert.equal(source.includes('ctx.strokeRect(bounds.x - 4'), false);
    assert.equal(source.includes('ctx.fillText(npc.moodEmoji'), false);
    assert.equal(source.includes('ctx.fillText(model.plate ||'), false);
    assert.equal(source.includes('Loading plate image'), false);
    assert.equal(source.includes('ctx.fillText(model.title, width * 0.12'), false);
    assert.equal(source.includes('ctx.fillRect(bounds.x, bounds.y, bounds.width, bounds.height)'), false);
});

test('visual client exposes invisible atom-only compositor proof telemetry', async () => {
    const html = await readFile(new URL('../index.html', import.meta.url), 'utf8');
    const mainSource = await readFile(new URL('./main.js', import.meta.url), 'utf8');
    const pixiSource = await readFile(new URL('./pixi-renderer.js', import.meta.url), 'utf8');

    assert.match(mainSource, /queryParams\.get\('visualProofMode'\) === 'atom-only'/);
    assert.match(mainSource, /queryParams\.get\('compositor'\) === 'atom-only'/);
    assert.match(mainSource, /proofAtomOnly,/);
    assert.match(pixiSource, /COMPOSITOR_TELEMETRY_KEY = '__rundaleVisualCompositor'/);
    assert.match(pixiSource, /mode: this\.proofAtomOnly \? 'atom-only' : 'normal'/);
    assert.match(pixiSource, /layerSprites\.push/);
    assert.match(pixiSource, /npcSprites\.push/);
    assert.match(pixiSource, /fallbackPlateUsed = true/);
    assert.match(pixiSource, /fallbackUnderlayUsed = true/);
    assert.match(pixiSource, /if \(!this\.proofAtomOnly && this\.worldContainer\.children\.length === 0 && model\.plate\)/);
    assert.match(pixiSource, /globalThis\[COMPOSITOR_TELEMETRY_KEY\]/);
    assert.equal(/atom-only|atomProof|Compositor|Telemetry|Debug/i.test(html), false);
});

test('visual client exposes invisible world-interaction proof telemetry', async () => {
    const html = await readFile(new URL('../index.html', import.meta.url), 'utf8');
    const source = await readFile(new URL('./main.js', import.meta.url), 'utf8');

    assert.match(source, /INTERACTION_TELEMETRY_KEY = '__rundaleVisualInteraction'/);
    assert.match(source, /targetTelemetry/);
    assert.match(source, /submittedCommands/);
    assert.match(source, /recordInteractionEvent\('hover'/);
    assert.match(source, /recordInteractionEvent\('activate-hotspot'/);
    assert.match(source, /recordInteractionEvent\('inspect-hotspot'/);
    assert.match(source, /recordInteractionEvent\('transition-start'/);
    assert.match(source, /recordInteractionEvent\('select-npc'/);
    assert.match(source, /recordInteractionEvent\('submit-command'/);
    assert.match(source, /globalThis\[INTERACTION_TELEMETRY_KEY\]/);
    assert.match(source, /target\.value\.activation\?\.target_label \|\| ''/);
    assert.equal(/rundaleVisualInteraction|Interaction Telemetry|Hover Target|Selected Target/i.test(html), false);
});

test('three-scene slice can render from PNG atom stacks without legacy plates', async () => {
    const scenes = JSON.parse(
        await readFile(new URL('../../../../mods/rundale/scenes.json', import.meta.url), 'utf8'),
    );
    const assetsById = new Map(scenes.assets.map((asset) => [asset.id, asset]));
    const expectations = [
        ['kilteevan-village', 30, ['ground-base', 'left-cottage', 'right-cottage', 'well', 'damp-vignette']],
        ['the-crossroads', 20, ['ground-base', 'church-rise', 'pub-building', 'damp-vignette']],
        ['darcys-pub', 25, ['room-base', 'hearth', 'bar-counter', 'warm-vignette']],
    ];

    for (const [slug, minLayers, requiredLayerIds] of expectations) {
        const scene = scenes.scenes.find((candidate) => candidate.slug === slug);
        assert.ok(scene.plate, `${slug} keeps legacy plate compatibility`);
        assert.ok(scene.underlay, `${slug} keeps legacy underlay compatibility`);
        assert.ok(scene.layers.length >= minLayers, `${slug} should be a multi-layer compositor scene`);
        for (const id of requiredLayerIds) {
            assert.ok(scene.layers.some((layer) => layer.id === id), `${slug} has ${id}`);
        }
        for (const layer of scene.layers) {
            const asset = assetsById.get(layer.asset);
            assert.ok(asset, `${slug}/${layer.id} has an asset`);
            assert.equal(asset.image.endsWith('.png'), true, `${slug}/${layer.id} is PNG`);
            assert.match(asset.image, /assets\/scenes\/.+\/atoms\//, `${slug}/${layer.id} is an atom`);
        }
    }
});

test('three-scene slice declares ambient PNG layer animations', async () => {
    const scenes = JSON.parse(
        await readFile(new URL('../../../../mods/rundale/scenes.json', import.meta.url), 'utf8'),
    );
    const expectations = [
        ['kilteevan-village', 'drift'],
        ['the-crossroads', 'shimmer'],
        ['darcys-pub', 'flicker'],
    ];

    for (const [slug, mode] of expectations) {
        const scene = scenes.scenes.find((candidate) => candidate.slug === slug);
        const animated = scene.layers.filter((layer) => layer.animation?.mode === mode);
        assert.ok(animated.length >= 1, `${slug} should declare ${mode} animation`);
        for (const layer of animated) {
            assert.ok(layer.animation.period_ms >= 250, `${layer.id} has valid period`);
            assert.ok(layer.animation.period_ms <= 60000, `${layer.id} has valid period`);
            assert.ok(Math.abs(layer.animation.amplitude_x || 0) <= 24, `${layer.id} x amplitude`);
            assert.ok(Math.abs(layer.animation.amplitude_y || 0) <= 24, `${layer.id} y amplitude`);
            assert.ok((layer.animation.alpha || 0) <= 0.5, `${layer.id} alpha`);
        }
    }
});

test('Darcy pub named NPC slots are positioned for readable composition', async () => {
    const scenes = JSON.parse(
        await readFile(new URL('../../../../mods/rundale/scenes.json', import.meta.url), 'utf8'),
    );
    const pub = scenes.scenes.find((scene) => scene.slug === 'darcys-pub');
    const benchLeft = pub.slots.find((slot) => slot.id === 'bench-left');
    const behindBar = pub.slots.find((slot) => slot.id === 'behind-bar');

    assert.equal(behindBar.prefer_npc, 1);
    assert.ok(benchLeft.x >= 74 && benchLeft.x <= 78);
    assert.ok(benchLeft.y >= 62 && benchLeft.y <= 66);
    assert.ok(benchLeft.scale >= 0.95);
});

test('Crossroads uses local sprite atoms for visible scene objects', async () => {
    const scenes = JSON.parse(
        await readFile(new URL('../../../../mods/rundale/scenes.json', import.meta.url), 'utf8'),
    );
    const assetsById = new Map(scenes.assets.map((asset) => [asset.id, asset]));
    const crossroads = scenes.scenes.find((scene) => scene.slug === 'the-crossroads');
    const localLayerIds = [
        'church-rise',
        'pub-building',
        'crooked-signpost',
        'stone-walls',
        'foreground-brambles',
        'road-wetness',
    ];

    for (const id of localLayerIds) {
        const layer = crossroads.layers.find((candidate) => candidate.id === id);
        const asset = assetsById.get(layer.asset);
        const dims = pngDimensions(
            await readFile(new URL(`../../../../mods/rundale/${asset.image}`, import.meta.url)),
        );

        assert.notDeepEqual([layer.x, layer.y], [50, 50], id);
        assert.match(asset.image, /assets\/scenes\/the-crossroads\/atoms\/local\//);
        assert.ok(dims.width < 1280, `${id} should be narrower than the full stage`);
        assert.ok(dims.height < 720, `${id} should be shorter than the full stage`);
    }
});

test('Crossroads repeats small kit atoms as reusable sprite layers', async () => {
    const scenes = JSON.parse(
        await readFile(new URL('../../../../mods/rundale/scenes.json', import.meta.url), 'utf8'),
    );
    const assetsById = new Map(scenes.assets.map((asset) => [asset.id, asset]));
    const crossroads = scenes.scenes.find((scene) => scene.slug === 'the-crossroads');
    const kitLayers = crossroads.layers
        .map((layer) => ({ layer, asset: assetsById.get(layer.asset) }))
        .filter(({ asset }) =>
            asset?.image?.startsWith('assets/scenes/the-crossroads/atoms/kit/'),
        );
    const usageByAsset = new Map();

    for (const { layer, asset } of kitLayers) {
        const dims = pngDimensions(
            await readFile(new URL(`../../../../mods/rundale/${asset.image}`, import.meta.url)),
        );
        const existing = usageByAsset.get(layer.asset) || [];
        existing.push(layer);
        usageByAsset.set(layer.asset, existing);

        assert.notDeepEqual([layer.x, layer.y], [50, 50], layer.id);
        assert.equal(asset.image.endsWith('.svg'), false, layer.id);
        assert.ok(dims.width < 360, `${layer.id} should be a small reusable sprite`);
        assert.ok(dims.height < 240, `${layer.id} should be a small reusable sprite`);
    }

    assert.ok(kitLayers.length >= 4, 'expected several small Crossroads kit layers');
    assert.ok(
        [...usageByAsset.values()].some((layers) => {
            const distinctPositions = new Set(layers.map((layer) => `${layer.x},${layer.y}`));
            return layers.length >= 3 && distinctPositions.size >= 3;
        }),
        'expected one kit atom asset to be reused three or more times',
    );
});
