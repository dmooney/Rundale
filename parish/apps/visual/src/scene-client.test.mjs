import test from 'node:test';
import assert from 'node:assert/strict';

import {
    fetchSceneState,
    normalizeBackendUrl,
    postCommand,
    resolveApiUrl,
} from './scene-client.js';

test('normalizes backend URLs', () => {
    assert.equal(normalizeBackendUrl(' http://127.0.0.1:3030/// '), 'http://127.0.0.1:3030');
    assert.equal(normalizeBackendUrl(''), '');
});

test('resolves same-origin and absolute API URLs', () => {
    assert.equal(resolveApiUrl('', '/api/scene-state'), '/api/scene-state');
    assert.equal(
        resolveApiUrl('http://127.0.0.1:3030/', '/api/scene-state'),
        'http://127.0.0.1:3030/api/scene-state',
    );
});

test('fetches null scene-state without treating it as an error', async () => {
    const scene = await fetchSceneState({
        fetchImpl: async (url) => {
            assert.equal(url, '/api/scene-state');
            return {
                ok: true,
                status: 200,
                json: async () => null,
            };
        },
    });
    assert.equal(scene, null);
});

test('throws on non-2xx scene-state responses', async () => {
    await assert.rejects(
        fetchSceneState({
            fetchImpl: async () => ({
                ok: false,
                status: 500,
                json: async () => ({}),
            }),
        }),
        /HTTP 500/,
    );
});

test('posts commands to the synchronous command endpoint', async () => {
    const response = await postCommand({
        text: 'go to The Crossroads',
        backendUrl: 'http://127.0.0.1:3030',
        fetchImpl: async (url, init) => {
            assert.equal(url, 'http://127.0.0.1:3030/api/command');
            assert.equal(init.method, 'POST');
            assert.equal(init.headers['content-type'], 'application/json');
            assert.deepEqual(JSON.parse(init.body), {
                text: 'go to The Crossroads',
                includeState: true,
                includeMap: false,
                timeoutMs: 60000,
            });
            return {
                ok: true,
                status: 200,
                json: async () => ({ outcome: 'ok', kind: 'moved', lines: [] }),
            };
        },
    });
    assert.equal(response.kind, 'moved');
});
