import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import {
	existsSync,
	mkdirSync,
	mkdtempSync,
	readFileSync,
	rmSync,
	statSync,
	utimesSync,
	writeFileSync,
} from 'node:fs';
import { createServer } from 'node:http';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';

import {
	PLAYWRIGHT_BUILD_LOCK_TIMEOUT_MS,
	PLAYWRIGHT_SERVER_SHUTDOWN_TIMEOUT_MS,
	PLAYWRIGHT_SERVER_TIMEOUT_MS,
	acquireBuildLock,
	assertServedCspCoherent,
	binaryContainsExpectedCsp,
	cargoExecutableFromMessages,
	cargoRustcArgs,
	collectInlineScriptHashes,
	inlineScriptHashesFromHtml,
	playwrightWebServerConfig,
	pruneOldCopies,
	uiDistFingerprint,
	worktreeKey,
} from './playwright-worktree-server.js';

const helperUrl = new URL('./playwright-worktree-server.js', import.meta.url)
	.href;

function backdate(path) {
	const old = new Date(Date.now() - 60_000);
	utimesSync(path, old, old);
}

test('worktree keys are stable and distinct', () => {
	assert.equal(worktreeKey('/tmp/worktree-a'), worktreeKey('/tmp/worktree-a'));
	assert.notEqual(
		worktreeKey('/tmp/worktree-a'),
		worktreeKey('/tmp/worktree-b'),
	);
});

test('managed config refuses to reuse a responsive stale local endpoint', async () => {
	const stale = createServer((_request, response) => {
		response.writeHead(200, { 'content-type': 'application/json' });
		response.end('{}');
	});
	stale.listen(0, '127.0.0.1');
	await once(stale, 'listening');
	try {
		const address = stale.address();
		assert.equal(typeof address, 'object');
		const config = playwrightWebServerConfig(address.port);
		assert.equal((await fetch(config.url)).status, 200);
		assert.equal(config.reuseExistingServer, false);
		assert.match(config.command, /playwright-worktree-server\.js/);
		assert.deepEqual(config.gracefulShutdown, {
			signal: 'SIGTERM',
			timeout: PLAYWRIGHT_SERVER_SHUTDOWN_TIMEOUT_MS,
		});
	} finally {
		stale.close();
		await once(stale, 'close');
	}
});

test('server timeout leaves a full minute beyond the build-lock wait', () => {
	assert.ok(
		PLAYWRIGHT_SERVER_TIMEOUT_MS >= PLAYWRIGHT_BUILD_LOCK_TIMEOUT_MS + 60_000,
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

test('missing dist returns no hashes so the launcher can report a helpful error', () => {
	assert.deepEqual(
		collectInlineScriptHashes('/definitely/missing/parish-dist'),
		[],
	);
});

test('preserved binary validation requires every expected CSP hash', () => {
	const hashes = ["'sha256-first='", "'sha256-second='"];
	const coherent = Buffer.from(`prefix ${hashes.join(' ')} suffix`);
	const stale = Buffer.from(`prefix ${hashes[0]} suffix`);

	assert.equal(binaryContainsExpectedCsp(coherent, hashes), true);
	assert.equal(binaryContainsExpectedCsp(stale, hashes), false);
	assert.equal(binaryContainsExpectedCsp(coherent, []), false);
});

test('served CSP must exactly match the dist and authorize served HTML', () => {
	const html = '<script>boot()</script>';
	const hashes = inlineScriptHashesFromHtml(html);
	const csp = `default-src 'self'; script-src 'self' ${hashes.join(' ')}`;
	assert.doesNotThrow(() => assertServedCspCoherent(html, csp, hashes));
	assert.throws(
		() => assertServedCspCoherent(html, `${csp} 'sha256-stale='`, hashes),
		/hashes do not match/,
	);
	assert.throws(
		() => assertServedCspCoherent(html, `${csp} 'unsafe-inline'`, hashes),
		/unsafe-inline/,
	);
});

test('UI dist fingerprints are stable and sensitive to the CSP hash set', () => {
	const first = uiDistFingerprint(["'sha256-first='"]);
	assert.equal(first, uiDistFingerprint(["'sha256-first='"]));
	assert.notEqual(first, uiDistFingerprint(["'sha256-second='"]));
});

test('cargo rustc reports the executable instead of assuming target/debug', () => {
	assert.deepEqual(cargoRustcArgs('abc123'), [
		'rustc',
		'--message-format=json-render-diagnostics',
		'-p',
		'parish-server',
		'--bin',
		'parish-server',
		'--',
		'-C',
		'metadata=playwright_abc123',
	]);
	const crossTargetPath = '/target/aarch64-unknown-linux/debug/parish-server';
	const output = JSON.stringify({
		reason: 'compiler-artifact',
		target: { kind: ['bin'], name: 'parish-server' },
		executable: crossTargetPath,
	});
	assert.equal(cargoExecutableFromMessages(output), crossTargetPath);
});

test('build lock serializes callers until the owner releases it', async () => {
	const root = mkdtempSync(join(tmpdir(), 'parish-playwright-lock-'));
	const lock = join(root, 'build.lock');

	try {
		const releaseFirst = await acquireBuildLock(lock, {
			pollMs: 5,
			timeoutMs: 1_000,
		});
		assert.equal(statSync(lock).isFile(), true);
		assert.doesNotThrow(() => JSON.parse(readFileSync(lock, 'utf8')));

		let secondAcquired = false;
		const second = acquireBuildLock(lock, {
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

test('recent ownerless lock is protected, then recovered after it is stale', async () => {
	const root = mkdtempSync(join(tmpdir(), 'parish-playwright-ownerless-'));
	const lock = join(root, 'build.lock');
	mkdirSync(lock);
	try {
		await assert.rejects(
			acquireBuildLock(lock, {
				pollMs: 5,
				staleGraceMs: 1_000,
				timeoutMs: 30,
			}),
			/timed out/,
		);
		assert.equal(existsSync(lock), true);

		backdate(lock);
		const release = await acquireBuildLock(lock, {
			pollMs: 5,
			staleGraceMs: 10,
			timeoutMs: 1_000,
		});
		release();
		assert.equal(existsSync(lock), false);
	} finally {
		rmSync(root, { force: true, recursive: true });
	}
});

test('stale corrupt owner is quarantined and later acquisition succeeds', async () => {
	const root = mkdtempSync(join(tmpdir(), 'parish-playwright-corrupt-'));
	const lock = join(root, 'build.lock');
	writeFileSync(lock, '{not-json');
	backdate(lock);
	try {
		const release = await acquireBuildLock(lock, {
			pollMs: 5,
			staleGraceMs: 10,
			timeoutMs: 1_000,
		});
		release();
		assert.equal(existsSync(lock), false);
	} finally {
		rmSync(root, { force: true, recursive: true });
	}
});

test('pruning continues when another process deletes one candidate', () => {
	const root = mkdtempSync(join(tmpdir(), 'parish-playwright-prune-'));
	const first = join(root, 'parish-server-first');
	const second = join(root, 'parish-server-second');
	writeFileSync(first, 'first');
	writeFileSync(second, 'second');
	backdate(first);
	backdate(second);
	try {
		pruneOldCopies(root, {
			now: Date.now() + 24 * 60 * 60 * 1000,
			statPath(path) {
				if (path === first) {
					rmSync(path);
					const error = new Error('deleted concurrently');
					error.code = 'ENOENT';
					throw error;
				}
				return statSync(path);
			},
		});
		assert.equal(existsSync(second), false);
	} finally {
		rmSync(root, { force: true, recursive: true });
	}
});

test('SIGTERM supervision removes the preserved binary before exit', async () => {
	const root = mkdtempSync(join(tmpdir(), 'parish-playwright-supervisor-'));
	const fakeServer = join(root, 'parish-server-test.mjs');
	const launcher = join(root, 'launcher.mjs');
	writeFileSync(
		fakeServer,
		"process.on('SIGTERM', () => process.exit(0)); console.log('fake-ready'); setInterval(() => {}, 1000);\n",
	);
	writeFileSync(
		launcher,
		`import { superviseServer } from ${JSON.stringify(helperUrl)};\nsuperviseServer(process.execPath, [${JSON.stringify(fakeServer)}], { cwd: ${JSON.stringify(root)}, preservedBinary: ${JSON.stringify(fakeServer)} });\n`,
	);

	const wrapper = spawn(process.execPath, [launcher], {
		stdio: ['ignore', 'pipe', 'pipe'],
	});
	let output = '';
	wrapper.stdout.setEncoding('utf8');
	wrapper.stdout.on('data', (chunk) => {
		output += chunk;
	});
	try {
		const deadline = Date.now() + 5_000;
		while (!output.includes('fake-ready') && Date.now() < deadline) {
			await new Promise((resolveDelay) => setTimeout(resolveDelay, 10));
		}
		assert.match(output, /fake-ready/);
		wrapper.kill('SIGTERM');
		const [code] = await once(wrapper, 'exit');
		assert.equal(code, 0);
		assert.equal(existsSync(fakeServer), false);
	} finally {
		if (wrapper.exitCode === null) wrapper.kill('SIGKILL');
		rmSync(root, { force: true, recursive: true });
	}
});
