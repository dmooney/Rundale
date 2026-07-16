import assert from 'node:assert/strict';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';

import {
	acquireDirectoryLock,
	binaryContainsExpectedCsp,
	cargoRustcArgs,
	inlineScriptHashesFromHtml,
	uiDistFingerprint,
	worktreeKey,
} from './playwright-worktree-server.js';

test('worktree keys are stable and distinct', () => {
	assert.equal(worktreeKey('/tmp/worktree-a'), worktreeKey('/tmp/worktree-a'));
	assert.notEqual(
		worktreeKey('/tmp/worktree-a'),
		worktreeKey('/tmp/worktree-b'),
	);
});

test('inline script hashing matches CSP semantics and skips attributed scripts', () => {
	const html = [
		'<script>first()</script>',
		'<script type="module">skip()</script>',
		'<script\n>second()</script>',
	].join('');
	const hashes = inlineScriptHashesFromHtml(html);

	assert.equal(hashes.length, 2);
	assert.match(hashes[0], /^'sha256-[A-Za-z0-9+/]+=*'$/);
	assert.match(hashes[1], /^'sha256-[A-Za-z0-9+/]+=*'$/);
	assert.notEqual(hashes[0], hashes[1]);
});

test('preserved binary validation requires every expected CSP hash', () => {
	const hashes = ["'sha256-first='", "'sha256-second='"];
	const coherent = Buffer.from(`prefix ${hashes.join(' ')} suffix`);
	const stale = Buffer.from(`prefix ${hashes[0]} suffix`);

	assert.equal(binaryContainsExpectedCsp(coherent, hashes), true);
	assert.equal(binaryContainsExpectedCsp(stale, hashes), false);
	assert.equal(binaryContainsExpectedCsp(coherent, []), false);
});

test('UI dist fingerprints are stable and sensitive to the CSP hash set', () => {
	const first = uiDistFingerprint(["'sha256-first='"]);
	assert.equal(first, uiDistFingerprint(["'sha256-first='"]));
	assert.notEqual(first, uiDistFingerprint(["'sha256-second='"]));
});

test('cargo rustc receives a unique final-crate metadata key', () => {
	assert.deepEqual(cargoRustcArgs('abc123'), [
		'rustc',
		'-p',
		'parish-server',
		'--bin',
		'parish-server',
		'--',
		'-C',
		'metadata=playwright_abc123',
	]);
});

test('directory lock serializes callers until the owner releases it', async () => {
	const root = mkdtempSync(join(tmpdir(), 'parish-playwright-lock-'));
	const lock = join(root, 'build.lock');

	try {
		const releaseFirst = await acquireDirectoryLock(lock, {
			pollMs: 5,
			timeoutMs: 1_000,
		});
		let secondAcquired = false;
		const second = acquireDirectoryLock(lock, {
			pollMs: 5,
			timeoutMs: 1_000,
		}).then((release) => {
			secondAcquired = true;
			return release;
		});

		await new Promise((resolveDelay) => setTimeout(resolveDelay, 25));
		assert.equal(secondAcquired, false);
		releaseFirst();

		const releaseSecond = await second;
		assert.equal(secondAcquired, true);
		releaseSecond();
	} finally {
		rmSync(root, { force: true, recursive: true });
	}
});
