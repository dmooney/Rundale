import test from 'node:test';
import assert from 'node:assert/strict';

import {
    buildSceneDisplayModel,
    canvasPointToStage,
    findHotspotAtStagePoint,
    findNpcAtStagePoint,
    findSceneTargetAtStagePoint,
    hotspotCommand,
    npcCommand,
} from './renderer.js';

const scene = {
    schema_version: 1,
    location_id: 1,
    location_name: 'The Crossroads',
    indoor: false,
    slug: 'the-crossroads',
    plate_url: '/api/scene-asset/assets/scenes/the-crossroads/plate.png?v=1',
    variant: 'day',
    weather_overlay: null,
    hotspots: [
        {
            id: 'pub-lane',
            label: "Lane to Darcy's Pub",
            shape: { rect: [70, 36, 18, 32] },
            action: { travel_to: 2 },
        },
        {
            id: 'stone-wall',
            label: 'Weathered stone wall',
            shape: { rect: [7, 42, 22, 24] },
            action: { inspect: 'The wall is dark with rain.' },
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
    assert.equal(model.hotspots[0].label, "Lane to Darcy's Pub");
    assert.equal(model.hotspots[0].action, 'travel:2');
    assert.deepEqual(model.hotspots[0].rawAction, { travel_to: 2 });
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

test('derives travel and inspect commands from hotspot actions', () => {
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
