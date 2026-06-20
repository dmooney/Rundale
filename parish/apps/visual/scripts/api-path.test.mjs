import test from 'node:test';
import assert from 'node:assert/strict';
import { apiPathForRequestUrl } from './api-path.mjs';

test('keeps relative API paths and query strings', () => {
    assert.equal(apiPathForRequestUrl('/api/scene-state?fresh=1'), '/api/scene-state?fresh=1');
});

test('strips hostnames from absolute incoming API requests', () => {
    assert.equal(
        apiPathForRequestUrl('http://metadata.google.internal/api/scene-state?x=1'),
        '/api/scene-state?x=1',
    );
});

test('rejects non-API paths', () => {
    assert.equal(apiPathForRequestUrl('/src/main.js'), null);
});
