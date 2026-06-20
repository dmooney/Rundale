import test from 'node:test';
import assert from 'node:assert/strict';

import { buildSceneDisplayModel } from './renderer.js';

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
    assert.deepEqual(model.hotspots[0].bounds, {
        x: 896,
        y: 259.2,
        width: 230.4,
        height: 230.4,
    });
    assert.equal(model.slots[0].id, 'roadside-left');
    assert.equal(model.npcs[0].label, 'A farmer');
});
