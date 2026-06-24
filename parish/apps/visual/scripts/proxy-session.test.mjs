import test from 'node:test';
import assert from 'node:assert/strict';

import { createProxySessionStore, parseCookieHeader } from './proxy-session.mjs';

test('parses cookie headers without treating attributes as cookies', () => {
    assert.deepEqual(parseCookieHeader('a=1; b=two words; empty='), {
        a: '1',
        b: 'two words',
        empty: '',
    });
});

test('keeps backend cookies isolated by proxy client session', () => {
    const store = createProxySessionStore();
    const first = store.sessionForRequest('');
    const second = store.sessionForRequest('');

    assert.notEqual(first.id, second.id);
    assert.equal(first.created, true);
    assert.equal(second.created, true);

    store.rememberBackendCookie(first.id, ['parish_sid=one; Secure; HttpOnly']);
    store.rememberBackendCookie(second.id, ['parish_sid=two; Secure; HttpOnly']);

    assert.equal(store.backendCookieFor(first.id), 'parish_sid=one');
    assert.equal(store.backendCookieFor(second.id), 'parish_sid=two');

    const returning = store.sessionForRequest(store.clientSetCookie(first.id));
    assert.deepEqual(returning, { id: first.id, created: false });
    assert.equal(store.backendCookieFor(returning.id), 'parish_sid=one');
});
