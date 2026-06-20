import test from 'node:test';
import assert from 'node:assert/strict';

import { appendTurnEntry, createTurnEntry, responseSummary } from './turn-log.js';

test('summarizes the last non-empty backend response line', () => {
    assert.equal(
        responseSummary({
            outcome: 'moved',
            lines: [{ text: '' }, { text: 'You arrive at the pub.' }],
        }),
        'You arrive at the pub.',
    );
});

test('falls back to the response outcome when no text lines exist', () => {
    assert.equal(responseSummary({ outcome: 'moved', lines: [] }), 'moved');
});

test('creates compact typed turn entries', () => {
    assert.deepEqual(createTurnEntry('inspect', 'Inspect', '  A stone wall.  '), {
        kind: 'inspect',
        label: 'Inspect',
        text: 'A stone wall.',
    });
});

test('appends turn entries while trimming older history', () => {
    const entries = [
        createTurnEntry('player', 'You', 'one'),
        createTurnEntry('world', 'World', 'two'),
    ];
    assert.deepEqual(
        appendTurnEntry(entries, createTurnEntry('inspect', 'Inspect', 'three'), 2),
        [
            createTurnEntry('world', 'World', 'two'),
            createTurnEntry('inspect', 'Inspect', 'three'),
        ],
    );
});

test('drops blank entries from the turn log', () => {
    assert.deepEqual(
        appendTurnEntry([], createTurnEntry('system', 'System', '   ')),
        [],
    );
});
