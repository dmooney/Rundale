import test from 'node:test';
import assert from 'node:assert/strict';
import { backendOriginFromEnv, proxyTargetUrl } from './backend-origin.mjs';

test('defaults visual proxy to the local MCP backend', () => {
    assert.equal(backendOriginFromEnv({}), 'http://127.0.0.1:3030');
});

test('allows documented local Parish backend ports', () => {
    assert.equal(
        backendOriginFromEnv({ PARISH_BACKEND_URL: 'http://localhost:3001' }),
        'http://127.0.0.1:3001',
    );
    assert.equal(
        backendOriginFromEnv({ PARISH_BACKEND_PORT: '3030' }),
        'http://127.0.0.1:3030',
    );
});

test('rejects non-loopback visual proxy targets', () => {
    assert.throws(
        () => backendOriginFromEnv({ PARISH_BACKEND_URL: 'http://example.com:3030' }),
        /loopback/,
    );
});

test('rejects unexpected local ports for the visual proxy', () => {
    assert.throws(
        () => backendOriginFromEnv({ PARISH_BACKEND_URL: 'http://127.0.0.1:9' }),
        /3030 or 3001/,
    );
});

test('keeps incoming absolute request URLs on the configured backend origin', () => {
    const target = proxyTargetUrl(
        'http://metadata.google.internal/api/scene-state?x=1',
        'http://127.0.0.1:3030',
    );
    assert.equal(target.toString(), 'http://127.0.0.1:3030/api/scene-state?x=1');
});
