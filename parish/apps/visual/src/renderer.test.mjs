import test from 'node:test';
import assert from 'node:assert/strict';

import {
    buildSceneDisplayModel,
    buildWorldDrawList,
    canvasPointToStage,
    computeLayerAnimationFrame,
    findHotspotAtStagePoint,
    findNpcAtStagePoint,
    findSceneTargetAtStagePoint,
    hotspotCommand,
    normalizeLayerAnimation,
    npcCommand,
    renderSceneModel,
} from './renderer.js';

const scene = {
    schema_version: 1,
    location_id: 1,
    location_name: 'The Crossroads',
    indoor: false,
    slug: 'the-crossroads',
    native_size: [1280, 720],
    plate_url: '/api/scene-asset/assets/scenes/the-crossroads/plate.png?v=1',
    variant: 'day',
    weather_overlay: null,
    layers: [
        {
            id: 'underlay',
            asset_id: 'crossroads-underlay',
            kind: 'underlay',
            asset_url: '/api/scene-asset/assets/scenes/the-crossroads/plate.png?v=1',
            x: 50,
            y: 50,
            z: 0,
            scale: 1,
            opacity: 1,
            flip: false,
            anchor: [50, 50],
            animation: {
                mode: 'shimmer',
                amplitude_x: 2,
                amplitude_y: 1,
                alpha: 0.06,
                period_ms: 2600,
                phase: 0.25,
            },
            labels: [],
        },
    ],
    hotspots: [
        {
            id: 'pub-lane',
            label: "Lane to Darcy's Pub",
            shape: { rect: [70, 36, 18, 32] },
            action: { travel_to: 2 },
            activation: {
                kind: 'travel',
                target_location_id: 2,
                target_label: "Darcy's Pub",
                command: "go to Darcy's Pub",
            },
        },
        {
            id: 'stone-wall',
            label: 'Weathered stone wall',
            shape: { rect: [7, 42, 22, 24] },
            action: { inspect: 'The wall is dark with rain.' },
            activation: {
                kind: 'inspect',
                text: 'The wall is dark with rain.',
            },
        },
    ],
    slots: [
        {
            id: 'roadside-left',
            x: 29,
            y: 61,
            scale: 1,
            prefer_npc: null,
            occupied_npc_id: 4,
        },
    ],
    npcs: [
        {
            npc_id: 4,
            slot_id: 'roadside-left',
            display_name: 'A farmer',
            real_name: null,
            introduced: false,
            mood: 'calm',
            mood_emoji: '',
            sprite_url: '/api/scene-asset/assets/scenes/sprites/generic-villager.png?v=1',
            x: 29,
            y: 61,
            scale: 1,
            flip: false,
        },
    ],
    overflow_npcs: [],
};

function makeRecordingCanvas(width = 1280, height = 720) {
    const calls = [];
    const record = (name, args) => calls.push({ name, args: [...args] });
    const ctx = {
        setTransform(...args) {
            record('setTransform', args);
        },
        clearRect(...args) {
            record('clearRect', args);
        },
        fillRect(...args) {
            record('fillRect', args);
        },
        strokeRect(...args) {
            record('strokeRect', args);
        },
        drawImage(...args) {
            record('drawImage', args);
        },
        fillText(...args) {
            record('fillText', args);
        },
        save(...args) {
            record('save', args);
        },
        restore(...args) {
            record('restore', args);
        },
        beginPath(...args) {
            record('beginPath', args);
        },
        ellipse(...args) {
            record('ellipse', args);
        },
        arc(...args) {
            record('arc', args);
        },
        fill(...args) {
            record('fill', args);
        },
        stroke(...args) {
            record('stroke', args);
        },
        translate(...args) {
            record('translate', args);
        },
        scale(...args) {
            record('scale', args);
        },
    };
    const canvas = {
        width,
        height,
        clientWidth: width,
        clientHeight: height,
        getContext: () => ctx,
    };
    return { canvas, calls };
}

function renderedText(calls) {
    return calls.filter((call) => call.name === 'fillText').map((call) => String(call.args[0]));
}

function completeImage(width = 1280, height = 720) {
    return { complete: true, naturalWidth: width, naturalHeight: height };
}

test('builds a disabled scene model for null backend responses', () => {
    const model = buildSceneDisplayModel(null);
    assert.equal(model.kind, 'empty');
    assert.equal(model.hotspots.length, 0);
    assert.equal(model.location, '-');
});

test('maps backend scene-state into graphics display geometry', () => {
    const model = buildSceneDisplayModel(scene);
    assert.equal(model.kind, 'scene');
    assert.equal(model.title, 'The Crossroads');
    assert.equal(model.layers[0].id, 'underlay');
    assert.equal(model.layers[0].assetId, 'crossroads-underlay');
    assert.deepEqual(model.layers[0].animation, {
        mode: 'shimmer',
        amplitudeX: 2,
        amplitudeY: 1,
        alpha: 0.06,
        periodMs: 2600,
        phase: 0.25,
    });
    assert.equal(model.hotspots[0].label, "Lane to Darcy's Pub");
    assert.equal(model.hotspots[0].action, 'travel:2');
    assert.deepEqual(model.hotspots[0].rawAction, { travel_to: 2 });
    assert.equal(model.hotspots[0].activation.command, "go to Darcy's Pub");
    assert.deepEqual(model.hotspots[0].bounds, {
        x: 896,
        y: 259.2,
        width: 230.4,
        height: 230.4,
    });
    assert.equal(model.slots[0].id, 'roadside-left');
    assert.equal(model.npcs[0].label, 'A farmer');
    assert.equal(
        model.npcs[0].spriteUrl,
        '/api/scene-asset/assets/scenes/sprites/generic-villager.png?v=1',
    );
    assert.deepEqual(model.npcs[0].bounds, {
        x: 347.2,
        y: 367.2,
        width: 48,
        height: 72,
    });
});

test('preserves named NPC sprite URLs from scene state', () => {
    const namedScene = {
        ...scene,
        npcs: [
            ['1', 'an older man behind the bar', 'padraig-darcy.png'],
            ['8', 'a young woman', 'niamh-darcy.png'],
            ['22', 'a sharp-eyed old woman', 'peig-hannigan.png'],
        ].map(([id, label, sprite], index) => ({
            ...scene.npcs[0],
            npc_id: Number(id),
            slot_id: `slot-${id}`,
            display_name: label,
            sprite_url: `/api/scene-asset/assets/scenes/sprites/${sprite}?v=1`,
            x: 20 + index * 20,
        })),
    };

    const model = buildSceneDisplayModel(namedScene);

    assert.deepEqual(
        model.npcs.map((npc) => npc.spriteUrl),
        [
            '/api/scene-asset/assets/scenes/sprites/padraig-darcy.png?v=1',
            '/api/scene-asset/assets/scenes/sprites/niamh-darcy.png?v=1',
            '/api/scene-asset/assets/scenes/sprites/peig-hannigan.png?v=1',
        ],
    );
    for (const npc of model.npcs) {
        assert.equal(npc.spriteUrl.includes('generic-villager'), false);
        assert.equal(npc.spriteUrl.includes('.svg'), false);
    }
});

test('normalizes layer animation metadata for Pixi sprite updates', () => {
    assert.deepEqual(
        normalizeLayerAnimation({
            mode: 'DRIFT',
            amplitude_x: 80,
            amplitude_y: -80,
            alpha: 2,
            period_ms: 120,
            phase: 2,
        }),
        {
            mode: 'drift',
            amplitudeX: 24,
            amplitudeY: -24,
            alpha: 0.5,
            periodMs: 250,
            phase: 1,
        },
    );
    assert.equal(normalizeLayerAnimation({ mode: 'spin' }), null);
});

test('computes deterministic ambient animation frames', () => {
    const driftStart = computeLayerAnimationFrame(
        {
            mode: 'drift',
            amplitude_x: 4,
            amplitude_y: -2,
            alpha: 0.1,
            period_ms: 1000,
            phase: 0,
        },
        250,
    );
    assert.ok(Math.abs(driftStart.x - 4) < 0.0001);
    assert.ok(Math.abs(driftStart.y + 2) < 0.0001);
    assert.ok(Math.abs(driftStart.alpha - 0.1) < 0.0001);

    const flickerA = computeLayerAnimationFrame(
        { mode: 'flicker', alpha: 0.1, period_ms: 1000, phase: 0 },
        100,
    );
    const flickerB = computeLayerAnimationFrame(
        { mode: 'flicker', alpha: 0.1, period_ms: 1000, phase: 0 },
        600,
    );
    assert.notEqual(flickerA.alpha, flickerB.alpha);
});

test('computes equivalent frames for backend and display-model animation shapes', () => {
    const raw = {
        mode: 'shimmer',
        amplitude_x: 3,
        amplitude_y: 1,
        alpha: 0.07,
        period_ms: 1800,
        phase: 0.2,
    };
    const normalized = normalizeLayerAnimation(raw);

    assert.deepEqual(
        computeLayerAnimationFrame(normalized, 450),
        computeLayerAnimationFrame(raw, 450),
    );
});

test('builds one ordered draw list for scene layers and NPCs', () => {
    const layeredScene = {
        ...scene,
        layers: [
            {
                ...scene.layers[0],
                id: 'ground',
                asset_id: 'ground',
                kind: 'ground',
                z: -100,
            },
            {
                ...scene.layers[0],
                id: 'well',
                asset_id: 'well',
                kind: 'prop',
                z: 40,
                x: 60,
                y: 70,
            },
            {
                ...scene.layers[0],
                id: 'foreground-wall',
                asset_id: 'wall',
                kind: 'wall',
                z: 70,
                x: 62,
                y: 76,
            },
        ],
    };
    const model = buildSceneDisplayModel(layeredScene);
    const drawList = buildWorldDrawList(model);

    assert.deepEqual(
        drawList.map((drawable) => drawable.id),
        ['layer:ground', 'layer:well', 'npc:4', 'layer:foreground-wall'],
    );
    assert.equal(drawList[2].z, 50);
});

test('keeps full-stage effect atoms as ordered compositor layers', () => {
    const layeredScene = {
        ...scene,
        layers: [
            {
                ...scene.layers[0],
                id: 'room-base',
                asset_id: 'pub-room-base',
                kind: 'ground',
                asset_url: '/api/scene-asset/assets/scenes/darcys-pub/atoms/room-base.png?v=1',
                z: -100,
            },
            {
                ...scene.layers[0],
                id: 'contact-shadows',
                asset_id: 'pub-contact-shadows',
                kind: 'shadow',
                asset_url:
                    '/api/scene-asset/assets/scenes/darcys-pub/atoms/contact-shadows.png?v=1',
                z: -60,
                opacity: 0.72,
            },
            {
                ...scene.layers[0],
                id: 'hearth-glow',
                asset_id: 'pub-hearth-glow',
                kind: 'lighting',
                asset_url:
                    '/api/scene-asset/assets/scenes/darcys-pub/atoms/hearth-glow.png?v=1',
                z: 55,
                opacity: 0.68,
            },
        ],
        npcs: [],
    };
    const model = buildSceneDisplayModel(layeredScene);
    const drawList = buildWorldDrawList(model);

    assert.deepEqual(
        drawList.map((drawable) => [drawable.id, drawable.source.kind, drawable.z]),
        [
            ['layer:room-base', 'ground', -100],
            ['layer:contact-shadows', 'shadow', -60],
            ['layer:hearth-glow', 'lighting', 55],
        ],
    );
    assert.equal(model.layers[1].assetUrl.includes('/atoms/contact-shadows.png?v='), true);
    assert.equal(model.layers[2].opacity, 0.68);
});

test('hit-tests hotspots in authored stage coordinates', () => {
    const model = buildSceneDisplayModel(scene);
    assert.equal(
        findHotspotAtStagePoint(model, { x: 900, y: 300 })?.id,
        'pub-lane',
    );
    assert.equal(
        findHotspotAtStagePoint(model, { x: 120, y: 330 })?.id,
        'stone-wall',
    );
    assert.equal(findHotspotAtStagePoint(model, { x: 640, y: 360 }), null);
});

test('converts canvas client coordinates to the stage coordinate system', () => {
    const canvas = {
        getBoundingClientRect: () => ({ left: 10, top: 20, width: 640, height: 360 }),
    };
    assert.deepEqual(canvasPointToStage(canvas, 330, 200), { x: 640, y: 360 });
});

test('derives travel and inspect commands from backend hotspot activation hints', () => {
    const model = buildSceneDisplayModel(scene);
    assert.deepEqual(hotspotCommand(model.hotspots[0]), {
        kind: 'travel',
        command: "go to Darcy's Pub",
        label: "Lane to Darcy's Pub",
    });
    assert.deepEqual(hotspotCommand(model.hotspots[1]), {
        kind: 'inspect',
        text: 'The wall is dark with rain.',
        label: 'Weathered stone wall',
    });
});

test('hit-tests NPC sprites in authored stage coordinates', () => {
    const model = buildSceneDisplayModel(scene);
    assert.equal(findNpcAtStagePoint(model, { x: 371.2, y: 400 })?.id, 4);
    assert.equal(findNpcAtStagePoint(model, { x: 371.2, y: 450 }), null);
});

test('prefers NPC sprite hits over hotspot hits', () => {
    const overlappingScene = {
        ...scene,
        npcs: [
            {
                ...scene.npcs[0],
                x: 75,
                y: 50,
            },
        ],
    };
    const model = buildSceneDisplayModel(overlappingScene);
    const target = findSceneTargetAtStagePoint(model, { x: 960, y: 350 });
    assert.equal(target.kind, 'npc');
    assert.equal(target.value.id, 4);
});

test('derives talk commands from NPC sprite clicks', () => {
    const model = buildSceneDisplayModel(scene);
    assert.deepEqual(npcCommand(model.npcs[0]), {
        kind: 'talk',
        command: 'talk to A farmer',
        label: 'A farmer',
    });
});

test('canvas fallback keeps inactive interaction geometry invisible on first read', () => {
    const model = buildSceneDisplayModel({
        ...scene,
        npcs: [{ ...scene.npcs[0], mood_emoji: 'calm-icon' }],
    });
    const { canvas, calls } = makeRecordingCanvas();

    renderSceneModel(canvas, model, {
        plateImage: completeImage(),
        spriteImages: new Map([[4, completeImage(48, 72)]]),
    });

    const text = renderedText(calls);
    assert.equal(calls.some((call) => call.name === 'strokeRect'), false);
    assert.equal(calls.some((call) => call.name === 'ellipse'), false);
    assert.equal(calls.some((call) => call.name === 'arc'), false);
    assert.equal(text.includes('The Crossroads'), false);
    assert.equal(text.includes(model.plate), false);
    assert.equal(text.includes('Loading plate image'), false);
    assert.equal(text.includes("Lane to Darcy's Pub"), false);
    assert.equal(text.includes('Weathered stone wall'), false);
    assert.equal(text.includes('roadside-left'), false);
    assert.equal(text.includes('A farmer'), false);
    assert.equal(text.includes('calm-icon'), false);
});

test('canvas fallback draws only active or selected game cues', () => {
    const model = buildSceneDisplayModel(scene);
    const { canvas, calls } = makeRecordingCanvas();

    renderSceneModel(canvas, model, {
        plateImage: completeImage(),
        spriteImages: new Map([[4, completeImage(48, 72)]]),
        activeHotspotId: 'pub-lane',
        selectedHotspotId: 'stone-wall',
        activeNpcId: 4,
    });

    const text = renderedText(calls);
    assert.equal(calls.some((call) => call.name === 'strokeRect'), false);
    assert.ok(calls.filter((call) => call.name === 'ellipse').length >= 3);
    assert.ok(calls.filter((call) => call.name === 'arc').length >= 4);
    assert.equal(text.includes('A farmer'), true);
    assert.equal(text.includes("Lane to Darcy's Pub"), false);
    assert.equal(text.includes('Weathered stone wall'), false);
    assert.equal(text.includes('roadside-left'), false);
    assert.equal(text.includes('The Crossroads'), false);
});
