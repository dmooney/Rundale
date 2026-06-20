import test from 'node:test';
import assert from 'node:assert/strict';

import { controlState, visualStatusLabel } from './client-status.js';

test('maps visual client status kinds to compact labels', () => {
    assert.equal(visualStatusLabel('loading'), 'Loading scene');
    assert.equal(visualStatusLabel('ready'), 'Scene ready');
    assert.equal(visualStatusLabel('empty'), 'No scene available');
    assert.equal(visualStatusLabel('sending'), 'Sending command');
    assert.equal(visualStatusLabel('error'), 'Connection error');
});

test('falls back to loading for unknown status kinds', () => {
    assert.equal(visualStatusLabel('unknown'), 'Loading scene');
});

test('disables visual controls while refresh or command work is in flight', () => {
    assert.deepEqual(controlState(), {
        busy: false,
        disableRefresh: false,
        disableCommand: false,
        disableActions: false,
    });
    assert.equal(controlState({ isRefreshing: true }).disableActions, true);
    assert.equal(controlState({ isSending: true }).disableCommand, true);
});
