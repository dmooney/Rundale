import test from 'node:test';
import assert from 'node:assert/strict';

import { hotspotActionLabel, npcActionLabel } from './action-list.js';

test('formats hotspot quick-action labels with action hints', () => {
    assert.equal(
        hotspotActionLabel({ label: "Lane to Darcy's Pub", action: 'travel:2' }),
        "Lane to Darcy's Pub (travel:2)",
    );
    assert.equal(
        hotspotActionLabel({ label: 'Weathered stone wall', action: 'inspect' }),
        'Weathered stone wall (inspect)',
    );
});

test('formats NPC quick-action labels with slot hints', () => {
    assert.equal(
        npcActionLabel({ label: 'A farmer', slotId: 'roadside-left' }),
        'A farmer at roadside-left',
    );
});

test('falls back for partial quick-action values', () => {
    assert.equal(hotspotActionLabel(null), 'Hotspot');
    assert.equal(npcActionLabel(null), 'Someone');
});
