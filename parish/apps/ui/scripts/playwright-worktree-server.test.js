import assert from 'node:assert/strict';
import { execFileSync, spawn } from 'node:child_process';
import { EventEmitter, once } from 'node:events';
import {
	existsSync,
	mkdirSync,
	mkdtempSync,
	realpathSync,
	readdirSync,
	readFileSync,
	rmSync,
	statSync,
	utimesSync,
	writeFileSync,
} from 'node:fs';
import { createServer } from 'node:http';
import { hostname, tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';

import {
	PLAYWRIGHT_ACTIVE_USE_STALE_GRACE_MS,
	PLAYWRIGHT_ARTIFACT_MAX_AGE_MS,
	PLAYWRIGHT_BUILD_ID_HEADER,
	PLAYWRIGHT_BUILD_LOCK_TIMEOUT_MS,
	PLAYWRIGHT_SERVER_SHUTDOWN_TIMEOUT_MS,
	PLAYWRIGHT_SERVER_TIMEOUT_MS,
	acquireBuildLock,
	allocateLoopbackPort,
	assertServedCspCoherent,
	binaryContainsExpectedBuildIdentity,
	binaryContainsExpectedCsp,
	binaryContentDigest,
	buildUiDist,
	captureUiDist,
	cargoBuildArgs,
	cargoExecutableFromMessages,
	collectInlineScriptHashes,
	inlineScriptHashesFromHtml,
	npmBuildCommand,
	playwrightBuildIdentity,
	playwrightWebServerConfig,
	prepareManagedServer,
	pruneLegacyLockCandidates,
	pruneServerArtifacts,
	publishActiveUseLease,
	publishCachedBinary,
	publishReadyMarker,
	publishUiSnapshot,
	resolvePlaywrightPort,
	runManagedServerLifecycle,
	superviseServer,
	uiDistFingerprint,
	waitForServedCsp,
	worktreeKey,
} from './playwright-worktree-server.js';

const helperUrl = new URL('./playwright-worktree-server.js', import.meta.url)
	.href;

function backdate(path, milliseconds = 60_000) {
	const old = new Date(Date.now() - milliseconds);
	utimesSync(path, old, old);
}

async function closeServer(server) {
	server.close();
	server.closeAllConnections?.();
	await once(server, 'close');
}

test('worktree and build identities are stable and distinct', () => {
	assert.equal(worktreeKey('/tmp/worktree-a'), worktreeKey('/tmp/worktree-a'));
	assert.notEqual(
		worktreeKey('/tmp/worktree-a'),
		worktreeKey('/tmp/worktree-b'),
	);
	assert.equal(
		playwrightBuildIdentity('/tmp/worktree-a', 'ui-a'),
		playwrightBuildIdentity('/tmp/worktree-a', 'ui-a'),
	);
	assert.notEqual(
		playwrightBuildIdentity('/tmp/worktree-a', 'ui-a'),
		playwrightBuildIdentity('/tmp/worktree-b', 'ui-a'),
	);
});

test('managed config uses per-run readiness and a Windows-correct shutdown policy', async () => {
	const stale = createServer((_request, response) => {
		response.writeHead(200, { 'content-type': 'application/json' });
		response.end('{}');
	});
	stale.listen(0, '127.0.0.1');
	await once(stale, 'listening');
	try {
		const address = stale.address();
		assert.equal(typeof address, 'object');
		const windows = playwrightWebServerConfig(address.port, {
			platform: 'win32',
			runId: '0123456789abcdef',
		});
		const posix = playwrightWebServerConfig(address.port, {
			platform: 'linux',
			runId: 'fedcba9876543210',
		});
		assert.equal((await fetch(windows.url)).status, 200);
		assert.equal(windows.reuseExistingServer, false);
		assert.match(windows.command, /playwright-worktree-server\.js/);
		assert.match(windows.url, /playwright-ready\/0123456789abcdef$/);
		assert.equal(windows.env.PARISH_PLAYWRIGHT_RUN_ID, '0123456789abcdef');
		assert.equal('gracefulShutdown' in windows, false);
		assert.deepEqual(posix.gracefulShutdown, {
			signal: 'SIGTERM',
			timeout: PLAYWRIGHT_SERVER_SHUTDOWN_TIMEOUT_MS,
		});
		assert.notEqual(posix.url, windows.url);
	} finally {
		await closeServer(stale);
	}
});

test('UI build executes npm JavaScript entry point through Node without a shell', () => {
	const root = mkdtempSync(join(tmpdir(), 'parish ui build '));
	const cwd = join(root, 'worktree with spaces');
	const npmExecPath = join(root, 'npm tooling with spaces', 'npm-cli.mjs');
	const capturePath = join(root, 'invocation.json');
	try {
		mkdirSync(cwd, { recursive: true });
		mkdirSync(join(root, 'npm tooling with spaces'), { recursive: true });
		writeFileSync(
			npmExecPath,
			`import { writeFileSync } from 'node:fs';
writeFileSync(process.env.PARISH_PLAYWRIGHT_BUILD_TEST_CAPTURE, JSON.stringify({ args: process.argv.slice(2), cwd: process.cwd() }));
`,
		);
		buildUiDist({
			cwd,
			environment: {
				...process.env,
				npm_execpath: npmExecPath,
				PARISH_PLAYWRIGHT_BUILD_TEST_CAPTURE: capturePath,
			},
		});
		const invocation = JSON.parse(readFileSync(capturePath, 'utf8'));
		assert.deepEqual(invocation.args, ['run', 'build']);
		assert.equal(realpathSync(invocation.cwd), realpathSync(cwd));
	} finally {
		rmSync(root, { force: true, recursive: true });
	}
});

test('UI build validates its npm JavaScript entry point and explicit override', () => {
	const root = mkdtempSync(join(tmpdir(), 'parish-npm-entrypoint-'));
	const npmExecPath = join(root, 'npm tooling with spaces', 'npm-cli.js');
	const override = 'PARISH_PLAYWRIGHT_NPM_EXEC_PATH';
	try {
		mkdirSync(join(root, 'npm tooling with spaces'), { recursive: true });
		writeFileSync(npmExecPath, 'process.exitCode = 0;\n');
		const environment = {
			[override]: npmExecPath,
			npm_execpath: 'relative/npm-cli.js',
		};
		let invocation;
		buildUiDist({
			cwd: root,
			environment,
			execPath: '/node runtime with spaces/node',
			runCommand(command, args, options) {
				invocation = { args, command, options };
			},
		});
		assert.deepEqual(invocation.args, [npmExecPath, 'run', 'build']);
		assert.equal(invocation.command, '/node runtime with spaces/node');
		assert.equal(invocation.options.cwd, root);
		assert.equal(invocation.options.env, environment);
		assert.equal(invocation.options.shell, false);
		assert.equal(invocation.options.stdio, 'inherit');
		assert.equal(invocation.options.windowsHide, true);

		assert.throws(
			() => npmBuildCommand({ environment: {} }),
			/npm_execpath is unavailable/,
		);
		assert.throws(
			() => npmBuildCommand({ environment: { npm_execpath: 'npm-cli.js' } }),
			/must be an absolute path/,
		);
		assert.throws(
			() =>
				npmBuildCommand({
					environment: { npm_execpath: join(root, 'npm.cmd') },
				}),
			/must name a JavaScript entry point/,
		);
		assert.throws(
			() =>
				npmBuildCommand({
					environment: { npm_execpath: join(root, 'missing-npm-cli.js') },
				}),
			/does not exist/,
		);
	} finally {
		rmSync(root, { force: true, recursive: true });
	}
});

test('managed launcher rebuilds missing or stale dist before snapshot capture', async (t) => {
	const config = playwrightWebServerConfig(34567, {
		runId: '0123456789abcdef-managed-build',
	});
	assert.match(config.command, /playwright-worktree-server\.js/);
	assert.match(
		readFileSync(new URL(helperUrl), 'utf8'),
		/const prepared = await prepareManagedServer\(\)/,
	);
	for (const initialState of ['missing', 'stale']) {
		await t.test(initialState, async () => {
			const root = mkdtempSync(join(tmpdir(), 'parish-ui-build-order-'));
			const distDir = join(root, 'dist');
			const indexPath = join(distDir, 'index.html');
			const events = [];
			try {
				if (initialState === 'stale') {
					mkdirSync(distDir, { recursive: true });
					writeFileSync(indexPath, 'stale UI');
				}
				const prepared = await prepareManagedServer({
					buildUi() {
						events.push(
							`build:${existsSync(indexPath) ? readFileSync(indexPath, 'utf8') : 'missing'}`,
						);
						mkdirSync(distDir, { recursive: true });
						writeFileSync(indexPath, 'fresh UI');
					},
					prepare() {
						const captured = readFileSync(indexPath, 'utf8');
						events.push(`capture:${captured}`);
						assert.equal(captured, 'fresh UI');
						return { captured };
					},
				});
				assert.deepEqual(events, [
					`build:${initialState === 'stale' ? 'stale UI' : 'missing'}`,
					'capture:fresh UI',
				]);
				assert.deepEqual(prepared, { captured: 'fresh UI' });
			} finally {
				rmSync(root, { force: true, recursive: true });
			}
		});
	}
});

test('direct, package, baseline, and screenshot runs share the managed launcher', () => {
	const packageJson = JSON.parse(
		readFileSync(new URL('../package.json', import.meta.url), 'utf8'),
	);
	assert.equal(packageJson.scripts['test:e2e'], 'playwright test');
	assert.equal(
		packageJson.scripts['test:e2e:update'],
		'playwright test --update-snapshots',
	);

	const configSource = readFileSync(
		new URL('../playwright.config.ts', import.meta.url),
		'utf8',
	);
	assert.match(
		configSource,
		/webServer:\s*playwrightWebServerConfig\(testPort\)/,
	);

	const justfile = readFileSync(
		new URL('../../../justfile', import.meta.url),
		'utf8',
	);
	const updateRecipe = justfile.match(/^ui-e2e-update:\n(?: {4}.*\n)+/m)?.[0];
	const screenshotsRecipe = justfile.match(
		/^screenshots:\n(?: {4}.*\n)+/m,
	)?.[0];
	assert.match(updateRecipe ?? '', /npx playwright test --update-snapshots/);
	assert.match(
		screenshotsRecipe ?? '',
		/npx playwright test e2e\/screenshots\.spec\.ts/,
	);
});

test('GitHub-hosted and self-hosted Actions retain the identity helper path', () => {
	const helperSource = readFileSync(new URL(helperUrl), 'utf8');
	assert.doesNotMatch(helperSource, /GITHUB_ACTIONS/);
	assert.doesNotMatch(helperSource, /\blinkSync\b/);

	for (const runnerEnvironment of ['github-hosted', 'self-hosted']) {
		const config = playwrightWebServerConfig(34567, {
			platform: 'linux',
			runId: `0123456789abcdef-${runnerEnvironment}`,
		});
		assert.match(config.command, /playwright-worktree-server\.js/);
		assert.equal(
			config.env.PARISH_PLAYWRIGHT_RUN_ID.includes(runnerEnvironment),
			true,
		);
	}
});

test('default port allocation returns a bindable loopback port', async () => {
	const port = await allocateLoopbackPort();
	const server = createServer((_request, response) => response.end('ok'));
	server.listen(port, '127.0.0.1');
	await once(server, 'listening');
	try {
		assert.equal(server.address().port, port);
	} finally {
		await closeServer(server);
	}
});

test('repeated config evaluation reuses the allocated port', async () => {
	const environment = {};
	let allocations = 0;
	const allocate = async () => {
		allocations += 1;
		return 34567;
	};
	assert.equal(await resolvePlaywrightPort(environment, allocate), '34567');
	assert.equal(await resolvePlaywrightPort(environment, allocate), '34567');
	assert.equal(environment.PARISH_TEST_PORT, '34567');
	assert.equal(allocations, 1);
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
	assert.equal(captureUiDist('/definitely/missing/parish-dist'), undefined);
});

test('UI capture and published snapshot are content-addressed and immutable', () => {
	const root = mkdtempSync(join(tmpdir(), 'parish-playwright-dist-'));
	const source = join(root, 'source');
	const cache = join(root, 'cache');
	mkdirSync(join(source, 'assets'), { recursive: true });
	mkdirSync(cache);
	writeFileSync(join(source, 'index.html'), '<script>bootA()</script>');
	writeFileSync(join(source, 'assets', 'app.js'), 'asset-a');
	try {
		const first = captureUiDist(source);
		assert.equal(first.expectedHashes.length, 1);
		const snapshot = publishUiSnapshot(cache, first);
		assert.equal(captureUiDist(snapshot).fingerprint, first.fingerprint);

		writeFileSync(join(source, 'index.html'), '<script>bootB()</script>');
		const second = captureUiDist(source);
		assert.notEqual(second.fingerprint, first.fingerprint);
		assert.match(readFileSync(join(snapshot, 'index.html'), 'utf8'), /bootA/);
		assert.equal(publishUiSnapshot(cache, first), snapshot);
	} finally {
		rmSync(root, { force: true, recursive: true });
	}
});

test('binary validation requires CSP and embedded build identity', () => {
	const hashes = ["'sha256-first='", "'sha256-second='"];
	const buildId = 'pw-worktree-ui';
	const coherent = Buffer.from(`prefix ${buildId} ${hashes.join(' ')} suffix`);
	const stale = Buffer.from(`prefix ${hashes[0]} suffix`);

	assert.equal(binaryContainsExpectedCsp(coherent, hashes), true);
	assert.equal(binaryContainsExpectedCsp(stale, hashes), false);
	assert.equal(binaryContainsExpectedCsp(coherent, []), false);
	assert.equal(binaryContainsExpectedBuildIdentity(coherent, buildId), true);
	assert.equal(binaryContainsExpectedBuildIdentity(stale, buildId), false);
	assert.equal(binaryContentDigest(coherent), binaryContentDigest(coherent));
});

test('content-addressed server cache reuses an identical binary', () => {
	const root = mkdtempSync(join(tmpdir(), 'parish-playwright-binary-'));
	const binary = Buffer.from('fake executable');
	try {
		const first = publishCachedBinary(root, binary, 0o755);
		const second = publishCachedBinary(root, binary, 0o755);
		assert.equal(first, second);
		assert.deepEqual(readFileSync(first), binary);
		assert.equal(
			readdirSync(root).filter((name) => name.startsWith('parish-server-'))
				.length,
			1,
		);
	} finally {
		rmSync(root, { force: true, recursive: true });
	}
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

test('UI hash fingerprints are stable and sensitive to the CSP hash set', () => {
	const first = uiDistFingerprint(["'sha256-first='"]);
	assert.equal(first, uiDistFingerprint(["'sha256-first='"]));
	assert.notEqual(first, uiDistFingerprint(["'sha256-second='"]));
});

test('cargo build reports the executable instead of inventing a final filename', () => {
	assert.deepEqual(cargoBuildArgs(), [
		'build',
		'--message-format=json-render-diagnostics',
		'-p',
		'parish-server',
		'--bin',
		'parish-server',
	]);
	const sharedPath = '/target/debug/parish-server';
	const output = JSON.stringify({
		reason: 'compiler-artifact',
		target: { kind: ['bin'], name: 'parish-server' },
		executable: sharedPath,
	});
	assert.equal(cargoExecutableFromMessages(output), sharedPath);
});

test('build lock serializes callers until the owner releases it', async () => {
	const root = mkdtempSync(join(tmpdir(), 'parish-playwright-lock-'));
	const lock = join(root, 'build.lock');
	try {
		const first = await acquireBuildLock(lock, {
			heartbeatMs: 5,
			pollMs: 5,
			staleGraceMs: 50,
			timeoutMs: 1_000,
		});
		assert.equal(statSync(lock).isFile(), true);
		assert.doesNotThrow(() => JSON.parse(readFileSync(lock, 'utf8')));

		let secondAcquired = false;
		const secondPromise = acquireBuildLock(lock, {
			heartbeatMs: 5,
			pollMs: 5,
			staleGraceMs: 50,
			timeoutMs: 1_000,
		}).then((lease) => {
			secondAcquired = true;
			return lease;
		});
		await new Promise((resolveDelay) => setTimeout(resolveDelay, 25));
		assert.equal(secondAcquired, false);
		first.release();

		const second = await secondPromise;
		assert.equal(secondAcquired, true);
		second.release();
	} finally {
		rmSync(root, { force: true, recursive: true });
	}
});

test('heartbeat keeps a valid lock fresh beyond the stale threshold', async () => {
	const root = mkdtempSync(join(tmpdir(), 'parish-playwright-heartbeat-'));
	const lock = join(root, 'build.lock');
	try {
		const first = await acquireBuildLock(lock, {
			heartbeatMs: 5,
			pollMs: 5,
			staleGraceMs: 40,
			timeoutMs: 1_000,
		});
		await new Promise((resolveDelay) => setTimeout(resolveDelay, 80));
		await assert.rejects(
			acquireBuildLock(lock, {
				heartbeatMs: 5,
				pollMs: 5,
				staleGraceMs: 40,
				timeoutMs: 30,
			}),
			/timed out/,
		);
		first.assertOwned();
		first.release();
	} finally {
		rmSync(root, { force: true, recursive: true });
	}
});

test('recent ownerless lock is protected, then recovered after it is stale', async () => {
	const root = mkdtempSync(join(tmpdir(), 'parish-playwright-ownerless-'));
	const lock = join(root, 'build.lock');
	writeFileSync(lock, '');
	try {
		await assert.rejects(
			acquireBuildLock(lock, {
				heartbeatMs: 10,
				pollMs: 5,
				staleGraceMs: 1_000,
				timeoutMs: 30,
			}),
			/timed out/,
		);
		assert.equal(existsSync(lock), true);

		backdate(lock);
		const lease = await acquireBuildLock(lock, {
			heartbeatMs: 2,
			pollMs: 5,
			staleGraceMs: 10,
			timeoutMs: 1_000,
		});
		lease.release();
		assert.equal(existsSync(lock), false);
	} finally {
		rmSync(root, { force: true, recursive: true });
	}
});

test('stale valid lock is bounded despite PID reuse or another hostname', async () => {
	const root = mkdtempSync(join(tmpdir(), 'parish-playwright-reused-pid-'));
	const lock = join(root, 'build.lock');
	writeFileSync(
		lock,
		JSON.stringify({
			hostname: `${hostname()}-other`,
			pid: process.pid,
			token: 'old',
		}),
	);
	backdate(lock);
	try {
		const lease = await acquireBuildLock(lock, {
			heartbeatMs: 2,
			pollMs: 5,
			staleGraceMs: 10,
			timeoutMs: 1_000,
		});
		lease.release();
		assert.equal(existsSync(lock), false);
	} finally {
		rmSync(root, { force: true, recursive: true });
	}
});

test('an abruptly killed lock owner is recoverable after its bounded lease', async () => {
	const root = mkdtempSync(join(tmpdir(), 'parish-playwright-killed-owner-'));
	const lock = join(root, 'build.lock');
	const childScript = join(root, 'owner.mjs');
	writeFileSync(
		childScript,
		`import { acquireBuildLock } from ${JSON.stringify(helperUrl)};\nawait acquireBuildLock(${JSON.stringify(lock)}, { heartbeatMs: 5, staleGraceMs: 40, timeoutMs: 1000 });\nconsole.log('locked');\nsetInterval(() => {}, 1000);\n`,
	);
	const owner = spawn(process.execPath, [childScript], {
		stdio: ['ignore', 'pipe', 'inherit'],
	});
	owner.stdout.setEncoding('utf8');
	let output = '';
	owner.stdout.on('data', (chunk) => {
		output += chunk;
	});
	try {
		const deadline = Date.now() + 5_000;
		while (!output.includes('locked') && Date.now() < deadline) {
			await new Promise((resolveDelay) => setTimeout(resolveDelay, 10));
		}
		assert.match(output, /locked/);
		owner.kill('SIGKILL');
		await once(owner, 'exit');
		await new Promise((resolveDelay) => setTimeout(resolveDelay, 60));
		const recovered = await acquireBuildLock(lock, {
			heartbeatMs: 5,
			pollMs: 5,
			staleGraceMs: 40,
			timeoutMs: 1_000,
		});
		recovered.release();
		assert.equal(existsSync(lock), false);
	} finally {
		if (owner.exitCode === null) owner.kill('SIGKILL');
		rmSync(root, { force: true, recursive: true });
	}
});

test('pruning continues when another process deletes one artifact', () => {
	const root = mkdtempSync(join(tmpdir(), 'parish-playwright-prune-'));
	const first = join(root, 'parish-server-first');
	const second = join(root, 'parish-server-second');
	writeFileSync(first, 'first');
	writeFileSync(second, 'second');
	backdate(first);
	backdate(second);
	try {
		pruneServerArtifacts(root, {
			maxArtifacts: 0,
			now: Date.now(),
			staleGraceMs: 10,
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

test('pruning bounds reusable binaries and removes abrupt candidates', () => {
	const root = mkdtempSync(join(tmpdir(), 'parish-playwright-cache-prune-'));
	const worktree = join(root, 'worktree');
	mkdirSync(worktree);
	for (let index = 0; index < 5; index += 1) {
		const binary = join(worktree, `parish-server-${index}`);
		writeFileSync(binary, String(index));
		backdate(binary, 60_000 + index * 1_000);
	}
	const candidate = join(worktree, '.ui-dist-x.candidate-dead');
	mkdirSync(candidate);
	writeFileSync(join(candidate, 'partial'), 'partial');
	backdate(candidate);
	try {
		pruneServerArtifacts(root, {
			maxArtifacts: 2,
			staleGraceMs: 10,
		});
		assert.equal(existsSync(candidate), false);
		assert.equal(
			readdirSync(worktree).filter((name) => name.startsWith('parish-server-'))
				.length,
			2,
		);
	} finally {
		rmSync(root, { force: true, recursive: true });
	}
});

test('pruning limits each worktree and artifact group while age-bounding residue', () => {
	const root = mkdtempSync(join(tmpdir(), 'parish-playwright-cache-groups-'));
	for (const worktreeName of ['worktree-a', 'worktree-b']) {
		const worktree = join(root, worktreeName);
		mkdirSync(worktree);
		for (let index = 0; index < 4; index += 1) {
			const binary = join(worktree, `parish-server-${index}`);
			const snapshot = join(worktree, `ui-dist-${index}`);
			writeFileSync(binary, String(index));
			mkdirSync(snapshot);
			writeFileSync(join(snapshot, 'index.html'), String(index));
			backdate(binary, 60_000 + index * 1_000);
			backdate(snapshot, 60_000 + index * 1_000);
		}
	}
	try {
		pruneServerArtifacts(root, {
			maxArtifacts: 3,
			staleGraceMs: 0,
		});
		for (const worktreeName of ['worktree-a', 'worktree-b']) {
			const entries = readdirSync(join(root, worktreeName));
			assert.equal(
				entries.filter((name) => name.startsWith('parish-server-')).length,
				3,
			);
			assert.equal(
				entries.filter((name) => name.startsWith('ui-dist-')).length,
				3,
			);
		}

		const agedBinary = join(root, 'worktree-a', 'parish-server-0');
		const agedSnapshot = join(root, 'worktree-a', 'ui-dist-0');
		backdate(agedBinary, PLAYWRIGHT_ARTIFACT_MAX_AGE_MS + 1_000);
		backdate(agedSnapshot, PLAYWRIGHT_ARTIFACT_MAX_AGE_MS + 1_000);
		pruneServerArtifacts(root, {
			maxArtifacts: 3,
			staleGraceMs: 0,
		});
		assert.equal(existsSync(agedBinary), false);
		assert.equal(existsSync(agedSnapshot), false);
	} finally {
		rmSync(root, { force: true, recursive: true });
	}
});

test('pruning removes empty per-worktree cache directories', () => {
	const root = mkdtempSync(join(tmpdir(), 'parish-playwright-empty-cache-'));
	const worktree = join(root, 'retired-worktree');
	mkdirSync(worktree);
	const binary = join(worktree, 'parish-server-retired');
	writeFileSync(binary, 'retired');
	backdate(binary);
	try {
		pruneServerArtifacts(root, {
			maxArtifacts: 0,
			staleGraceMs: 0,
		});
		assert.equal(existsSync(worktree), false);
		assert.equal(existsSync(root), true);
	} finally {
		rmSync(root, { force: true, recursive: true });
	}
});

test('fresh active-use lease keeps the oldest cache until abrupt-owner expiry', async () => {
	const root = mkdtempSync(join(tmpdir(), 'parish-playwright-active-cache-'));
	const activeBinary = join(root, 'parish-server-0');
	const activeSnapshot = join(root, 'ui-dist-0');
	for (let index = 0; index < 4; index += 1) {
		const binary = join(root, `parish-server-${index}`);
		const snapshot = join(root, `ui-dist-${index}`);
		writeFileSync(binary, String(index));
		mkdirSync(snapshot);
		writeFileSync(join(snapshot, 'index.html'), String(index));
		backdate(binary, 120_000 - index * 10_000);
		backdate(snapshot, 120_000 - index * 10_000);
	}

	const ownerScript = join(root, 'active-owner.mjs');
	writeFileSync(
		ownerScript,
		`import { publishActiveUseLease } from ${JSON.stringify(helperUrl)};\nconst lease = publishActiveUseLease(${JSON.stringify(root)}, [${JSON.stringify(activeBinary)}, ${JSON.stringify(activeSnapshot)}], { heartbeatMs: 5, staleGraceMs: 60 });\nconsole.log(lease.path);\nsetInterval(() => {}, 1000);\n`,
	);
	const owner = spawn(process.execPath, [ownerScript], {
		stdio: ['ignore', 'pipe', 'inherit'],
	});
	owner.stdout.setEncoding('utf8');
	let output = '';
	owner.stdout.on('data', (chunk) => {
		output += chunk;
	});
	try {
		const deadline = Date.now() + 5_000;
		while (!output.includes('\n') && Date.now() < deadline) {
			await new Promise((resolveDelay) => setTimeout(resolveDelay, 10));
		}
		const leasePath = output.trim();
		assert.match(leasePath, /\.playwright-server-active-[a-f0-9]{32}\.json$/);

		// The owner remains live beyond a full stale grace; only its heartbeat
		// keeps the oldest artifacts protected.
		await new Promise((resolveDelay) => setTimeout(resolveDelay, 80));
		pruneServerArtifacts(root, {
			leaseStaleGraceMs: 60,
			maxArtifacts: 3,
			staleGraceMs: 0,
		});
		assert.equal(existsSync(activeBinary), true);
		assert.equal(existsSync(activeSnapshot), true);
		assert.equal(existsSync(leasePath), true);
		assert.equal(
			readdirSync(root).filter((name) => name.startsWith('parish-server-'))
				.length,
			3,
		);
		assert.equal(
			readdirSync(root).filter((name) => name.startsWith('ui-dist-')).length,
			3,
		);

		const ownerExit = once(owner, 'exit');
		if (process.platform === 'win32') {
			execFileSync('taskkill.exe', ['/pid', String(owner.pid), '/T', '/F'], {
				stdio: 'pipe',
			});
		} else {
			owner.kill('SIGKILL');
		}
		await ownerExit;
		// Force-tree termination cannot run the publisher's release hook. The
		// lease must remain for bounded retirement rather than claim a clean exit.
		assert.equal(existsSync(leasePath), true);
		await new Promise((resolveDelay) => setTimeout(resolveDelay, 80));
		const newestBinary = join(root, 'parish-server-newest');
		const newestSnapshot = join(root, 'ui-dist-newest');
		writeFileSync(newestBinary, 'newest');
		mkdirSync(newestSnapshot);
		writeFileSync(join(newestSnapshot, 'index.html'), 'newest');
		pruneServerArtifacts(root, {
			leaseStaleGraceMs: 60,
			maxArtifacts: 3,
			staleGraceMs: 0,
		});
		assert.equal(existsSync(leasePath), false);
		assert.equal(existsSync(activeBinary), true);
		assert.equal(existsSync(activeSnapshot), true);
		const tombstone = readdirSync(root).find((name) =>
			name.startsWith('.playwright-server-retired-'),
		);
		assert.ok(tombstone);
		pruneServerArtifacts(root, {
			leaseStaleGraceMs: 10_000,
			maxArtifacts: 3,
			staleGraceMs: 0,
		});
		assert.equal(existsSync(join(root, tombstone)), true);
		assert.equal(existsSync(activeBinary), true);
		assert.equal(existsSync(activeSnapshot), true);
		const reclaimNow = Date.now() + 10_001;
		pruneServerArtifacts(root, {
			leaseStaleGraceMs: 60,
			maxArtifacts: 3,
			now: reclaimNow,
			staleGraceMs: 0,
		});
		assert.equal(existsSync(join(root, tombstone)), false);
		assert.equal(existsSync(activeBinary), true);
		assert.equal(existsSync(activeSnapshot), true);
		const finalBinary = join(root, 'parish-server-final');
		const finalSnapshot = join(root, 'ui-dist-final');
		writeFileSync(finalBinary, 'final');
		mkdirSync(finalSnapshot);
		writeFileSync(join(finalSnapshot, 'index.html'), 'final');
		pruneServerArtifacts(root, {
			leaseStaleGraceMs: 60,
			maxArtifacts: 3,
			now: reclaimNow,
			staleGraceMs: 0,
		});
		assert.equal(existsSync(activeBinary), false);
		assert.equal(existsSync(activeSnapshot), false);
	} finally {
		if (owner.exitCode === null) owner.kill('SIGKILL');
		rmSync(root, { force: true, recursive: true });
	}
});

test('valid-name malformed leases fail closed through retirement grace', () => {
	const cases = [
		{
			contents: '{',
			name: 'truncated JSON',
			token: 'a'.repeat(32),
		},
		{
			contents: JSON.stringify({
				artifacts: ['not-a-pair'],
				hostname: hostname(),
				pid: process.pid,
				token: 'b'.repeat(32),
				version: 1,
			}),
			name: 'invalid schema',
			token: 'b'.repeat(32),
		},
	];

	for (const fixture of cases) {
		const root = mkdtempSync(join(tmpdir(), 'parish-playwright-bad-active-'));
		for (let index = 0; index < 4; index += 1) {
			const binary = join(root, `parish-server-${index}`);
			writeFileSync(binary, String(index));
			backdate(binary, 60_000 + index * 1_000);
		}
		const malformed = join(
			root,
			`.playwright-server-active-${fixture.token}.json`,
		);
		writeFileSync(malformed, fixture.contents);
		try {
			pruneServerArtifacts(root, {
				leaseStaleGraceMs: 1_000,
				maxArtifacts: 3,
				staleGraceMs: 0,
			});
			assert.equal(
				readdirSync(root).filter((name) => name.startsWith('parish-server-'))
					.length,
				4,
				fixture.name,
			);

			backdate(malformed);
			pruneServerArtifacts(root, {
				leaseStaleGraceMs: 10,
				maxArtifacts: 3,
				staleGraceMs: 0,
			});
			assert.equal(existsSync(malformed), false, fixture.name);
			const tombstone = readdirSync(root).find((name) =>
				name.startsWith('.playwright-server-retired-'),
			);
			assert.ok(tombstone, fixture.name);
			assert.equal(
				readdirSync(root).filter((name) => name.startsWith('parish-server-'))
					.length,
				4,
				fixture.name,
			);
			pruneServerArtifacts(root, {
				leaseStaleGraceMs: 10_000,
				maxArtifacts: 3,
				staleGraceMs: 0,
			});
			assert.equal(existsSync(join(root, tombstone)), true, fixture.name);
			assert.equal(
				readdirSync(root).filter((name) => name.startsWith('parish-server-'))
					.length,
				4,
				fixture.name,
			);

			const reclaimNow = Date.now() + 10_001;
			pruneServerArtifacts(root, {
				leaseStaleGraceMs: 10,
				maxArtifacts: 3,
				now: reclaimNow,
				staleGraceMs: 0,
			});
			assert.equal(existsSync(join(root, tombstone)), false, fixture.name);
			assert.equal(
				readdirSync(root).filter((name) => name.startsWith('parish-server-'))
					.length,
				4,
				fixture.name,
			);
			pruneServerArtifacts(root, {
				leaseStaleGraceMs: 10,
				maxArtifacts: 3,
				now: reclaimNow,
				staleGraceMs: 0,
			});
			assert.equal(
				readdirSync(root).filter((name) => name.startsWith('parish-server-'))
					.length,
				3,
				fixture.name,
			);
		} finally {
			rmSync(root, { force: true, recursive: true });
		}
	}
});

test('retirement filename preserves a full grace if tombstone mtime refresh fails', () => {
	const root = mkdtempSync(
		join(tmpdir(), 'parish-playwright-retirement-time-'),
	);
	const binary = join(root, 'parish-server-retiring');
	const snapshot = join(root, 'ui-dist-retiring');
	writeFileSync(binary, 'binary');
	mkdirSync(snapshot);
	backdate(binary, 1_000);
	backdate(snapshot, 1_000);
	const lease = publishActiveUseLease(root, [binary, snapshot]);
	backdate(lease.path, 1_000);
	const retiredAtMs = Date.now();
	try {
		pruneServerArtifacts(root, {
			leaseStaleGraceMs: 100,
			maxArtifacts: 0,
			now: retiredAtMs,
			staleGraceMs: 0,
			touchPath() {
				const error = new Error('simulated Windows mtime sharing failure');
				error.code = 'EPERM';
				throw error;
			},
		});
		const tombstone = readdirSync(root).find((name) =>
			name.startsWith('.playwright-server-retired-'),
		);
		assert.ok(tombstone);
		const tombstonePath = join(root, tombstone);
		assert.equal(existsSync(binary), true);
		assert.equal(existsSync(snapshot), true);

		pruneServerArtifacts(root, {
			leaseStaleGraceMs: 100,
			maxArtifacts: 0,
			now: retiredAtMs + 50,
			staleGraceMs: 0,
		});
		assert.equal(existsSync(tombstonePath), true);
		assert.equal(existsSync(binary), true);
		assert.equal(existsSync(snapshot), true);

		pruneServerArtifacts(root, {
			leaseStaleGraceMs: 100,
			maxArtifacts: 0,
			now: retiredAtMs + 101,
			staleGraceMs: 0,
		});
		assert.equal(existsSync(tombstonePath), false);
		assert.equal(existsSync(binary), true);
		assert.equal(existsSync(snapshot), true);
		pruneServerArtifacts(root, {
			leaseStaleGraceMs: 100,
			maxArtifacts: 0,
			now: retiredAtMs + 101,
			staleGraceMs: 0,
		});
		assert.equal(existsSync(binary), false);
		assert.equal(existsSync(snapshot), false);
	} finally {
		lease.release();
		rmSync(root, { force: true, recursive: true });
	}
});

test('releasing one active-use lease cannot remove another run lease', () => {
	const root = mkdtempSync(join(tmpdir(), 'parish-playwright-owned-active-'));
	const firstBinary = join(root, 'parish-server-first');
	const firstSnapshot = join(root, 'ui-dist-first');
	const secondBinary = join(root, 'parish-server-second');
	const secondSnapshot = join(root, 'ui-dist-second');
	for (const path of [firstBinary, secondBinary]) writeFileSync(path, path);
	for (const path of [firstSnapshot, secondSnapshot]) mkdirSync(path);
	for (const path of [
		firstBinary,
		firstSnapshot,
		secondBinary,
		secondSnapshot,
	]) {
		backdate(path, 1_000);
	}
	const first = publishActiveUseLease(root, [firstBinary, firstSnapshot]);
	const second = publishActiveUseLease(root, [secondBinary, secondSnapshot]);
	try {
		pruneServerArtifacts(root, {
			leaseStaleGraceMs: PLAYWRIGHT_ACTIVE_USE_STALE_GRACE_MS,
			maxArtifacts: 0,
			staleGraceMs: 0,
		});
		for (const path of [
			firstBinary,
			firstSnapshot,
			secondBinary,
			secondSnapshot,
			first.path,
			second.path,
		]) {
			assert.equal(existsSync(path), true);
		}

		first.release();
		assert.equal(existsSync(first.path), false);
		assert.equal(existsSync(second.path), true);
		pruneServerArtifacts(root, {
			leaseStaleGraceMs: PLAYWRIGHT_ACTIVE_USE_STALE_GRACE_MS,
			maxArtifacts: 0,
			staleGraceMs: 0,
		});
		assert.equal(existsSync(firstBinary), false);
		assert.equal(existsSync(firstSnapshot), false);
		assert.equal(existsSync(secondBinary), true);
		assert.equal(existsSync(secondSnapshot), true);
		assert.equal(existsSync(second.path), true);
	} finally {
		first.release();
		second.release();
		rmSync(root, { force: true, recursive: true });
	}
});

test('server supervision releases the active-use lease on child exit', async () => {
	const root = mkdtempSync(join(tmpdir(), 'parish-playwright-supervised-'));
	const binary = join(root, 'parish-server-supervised');
	const snapshot = join(root, 'ui-dist-supervised');
	writeFileSync(binary, 'binary');
	mkdirSync(snapshot);
	const lease = publishActiveUseLease(root, [binary, snapshot]);
	try {
		const supervision = superviseServer(process.execPath, ['-e', ''], {
			cwd: root,
			stdio: 'ignore',
		});
		await runManagedServerLifecycle({
			activeUseLease: lease,
			supervision,
		});
		assert.equal(existsSync(lease.path), false);
	} finally {
		lease.release();
		rmSync(root, { force: true, recursive: true });
	}
});

test(
	'lease loss force-fences a child that ignores graceful SIGTERM',
	{ skip: process.platform === 'win32' },
	async () => {
		const root = mkdtempSync(join(tmpdir(), 'parish-playwright-force-fence-'));
		const binary = join(root, 'parish-server-force-fence');
		const snapshot = join(root, 'ui-dist-force-fence');
		writeFileSync(binary, 'binary');
		mkdirSync(snapshot);
		const lease = publishActiveUseLease(root, [binary, snapshot], {
			heartbeatMs: 5,
			staleGraceMs: 60,
		});
		const supervision = superviseServer(
			process.execPath,
			[
				'-e',
				"process.on('SIGTERM', () => {}); console.log('ready'); setInterval(() => {}, 1000);",
			],
			{
				cwd: root,
				shutdownTimeoutMs: 50,
				stdio: ['ignore', 'pipe', 'inherit'],
			},
		);
		const lifecycleProcess = new EventEmitter();
		try {
			await once(supervision.server.stdout, 'data');
			const lifecycle = runManagedServerLifecycle({
				activeUseLease: lease,
				processRef: lifecycleProcess,
				supervision,
			});
			const invalidated = JSON.parse(readFileSync(lease.path, 'utf8'));
			invalidated.artifacts = [binary];
			writeFileSync(lease.path, JSON.stringify(invalidated));

			await assert.rejects(lifecycle, /artifact lease was lost/);
			const exit = await supervision.exited;
			assert.equal(exit.signal, 'SIGKILL');
			assert.equal(lifecycleProcess.exitCode, 1);
		} finally {
			if (
				supervision.server.exitCode === null &&
				supervision.server.signalCode === null
			) {
				await supervision.stop('SIGKILL');
			}
			lease.release();
			rmSync(root, { force: true, recursive: true });
		}
	},
);

test(
	'POSIX detached process-group shutdown releases the lease after child exit',
	{ skip: process.platform === 'win32' },
	async (t) => {
		for (const signal of ['SIGTERM', 'SIGINT']) {
			await t.test(signal, async () => {
				const root = mkdtempSync(
					join(tmpdir(), 'parish-playwright-process-group-'),
				);
				const binary = join(root, 'parish-server-group');
				const snapshot = join(root, 'ui-dist-group');
				const launcherScript = join(root, 'launcher.mjs');
				writeFileSync(binary, 'binary');
				mkdirSync(snapshot);
				writeFileSync(
					launcherScript,
					`import { publishActiveUseLease, runManagedServerLifecycle, superviseServer } from ${JSON.stringify(helperUrl)};\nconst root = ${JSON.stringify(root)};\nconst lease = publishActiveUseLease(root, [${JSON.stringify(binary)}, ${JSON.stringify(snapshot)}], { heartbeatMs: 10, staleGraceMs: 100 });\nconst supervision = superviseServer(process.execPath, ['-e', 'setInterval(() => {}, 1000)'], { cwd: root, stdio: 'ignore' });\nawait runManagedServerLifecycle({ activeUseLease: lease, supervision, waitUntilReady: async () => console.log(lease.path) });\n`,
				);
				const launcher = spawn(process.execPath, [launcherScript], {
					detached: true,
					stdio: ['ignore', 'pipe', 'inherit'],
				});
				launcher.stdout.setEncoding('utf8');
				let output = '';
				launcher.stdout.on('data', (chunk) => {
					output += chunk;
				});
				try {
					const deadline = Date.now() + 5_000;
					while (!output.includes('\n') && Date.now() < deadline) {
						await new Promise((resolveDelay) => setTimeout(resolveDelay, 10));
					}
					const leasePath = output.trim();
					assert.match(
						leasePath,
						/\.playwright-server-active-[a-f0-9]{32}\.json$/,
					);
					const launcherExit = once(launcher, 'exit');
					process.kill(-launcher.pid, signal);
					const [, exitSignal] = await launcherExit;
					assert.equal(exitSignal, null);
					assert.equal(existsSync(leasePath), false);
				} finally {
					if (launcher.exitCode === null && launcher.signalCode === null) {
						try {
							process.kill(-launcher.pid, 'SIGKILL');
						} catch (error) {
							assert.equal(error?.code, 'ESRCH');
						}
						await once(launcher, 'exit');
					}
					rmSync(root, { force: true, recursive: true });
				}
			});
		}
	},
);

test('legacy hard-link candidates are pruned after the crash grace', () => {
	const root = mkdtempSync(
		join(tmpdir(), 'parish-playwright-legacy-candidate-'),
	);
	const stale = join(
		root,
		'.playwright-parish-server-build.lock.candidate-dead',
	);
	const recent = join(
		root,
		'.playwright-parish-server-build.lock.candidate-active',
	);
	const abandoned = join(
		root,
		'.playwright-parish-server-build.lock.abandoned-dead',
	);
	writeFileSync(stale, 'stale');
	writeFileSync(recent, 'recent');
	writeFileSync(abandoned, 'abandoned');
	backdate(stale);
	backdate(abandoned);
	try {
		pruneLegacyLockCandidates(root, { staleGraceMs: 1_000 });
		assert.equal(existsSync(stale), false);
		assert.equal(existsSync(abandoned), false);
		assert.equal(existsSync(recent), true);
	} finally {
		rmSync(root, { force: true, recursive: true });
	}
});

test('validated readiness marker gates the live build identity and CSP', async () => {
	const root = mkdtempSync(join(tmpdir(), 'parish-playwright-ready-'));
	const readyFile = join(root, '.playwright-ready-run');
	const runId = '0123456789abcdef';
	const buildId = 'pw-worktree-ui';
	const html = '<script>boot()</script>';
	const hashes = inlineScriptHashesFromHtml(html);
	const csp = `default-src 'self'; script-src 'self' ${hashes.join(' ')}`;
	const server = createServer((request, response) => {
		if (request.url === `/api/playwright-ready/${runId}`) {
			response.setHeader(PLAYWRIGHT_BUILD_ID_HEADER, buildId);
			response.statusCode = existsSync(readyFile) ? 200 : 503;
			response.end();
			return;
		}
		response.setHeader('content-security-policy', csp);
		response.end(html);
	});
	server.listen(0, '127.0.0.1');
	await once(server, 'listening');
	try {
		await waitForServedCsp({
			buildId,
			expectedHashes: hashes,
			port: server.address().port,
			readyFile,
			runId,
			server: { exitCode: null },
			timeoutMs: 1_000,
		});
		assert.equal(readFileSync(readyFile, 'utf8'), `${runId}\n${buildId}\n`);
	} finally {
		await closeServer(server);
		rmSync(root, { force: true, recursive: true });
	}
});

test('another run on the same port cannot satisfy readiness', async () => {
	const root = mkdtempSync(join(tmpdir(), 'parish-playwright-owner-gate-'));
	const readyFile = join(root, '.playwright-ready-b');
	const ownerRun = '0123456789abcdef';
	const waitingRun = 'fedcba9876543210';
	const server = createServer((request, response) => {
		response.statusCode = request.url?.endsWith(ownerRun) ? 503 : 404;
		response.end();
	});
	server.listen(0, '127.0.0.1');
	await once(server, 'listening');
	try {
		await assert.rejects(
			waitForServedCsp({
				buildId: 'pw-waiting',
				expectedHashes: ["'sha256-unused='"],
				port: server.address().port,
				readyFile,
				runId: waitingRun,
				server: { exitCode: null },
				timeoutMs: 100,
			}),
			/another Playwright run owns the listener/,
		);
		assert.equal(existsSync(readyFile), false);
	} finally {
		await closeServer(server);
		rmSync(root, { force: true, recursive: true });
	}
});

test('a hanging response is aborted at the validation deadline', async () => {
	const root = mkdtempSync(join(tmpdir(), 'parish-playwright-hanging-'));
	const server = createServer(() => {});
	server.listen(0, '127.0.0.1');
	await once(server, 'listening');
	const started = Date.now();
	try {
		await assert.rejects(
			waitForServedCsp({
				buildId: 'pw-hanging',
				expectedHashes: ["'sha256-unused='"],
				port: server.address().port,
				readyFile: join(root, '.ready'),
				runId: '0123456789abcdef',
				server: { exitCode: null },
				timeoutMs: 75,
			}),
			/timed out validating/,
		);
		assert.ok(Date.now() - started < 1_000);
	} finally {
		await closeServer(server);
		rmSync(root, { force: true, recursive: true });
	}
});

test('ready-marker publication is idempotent but rejects an identity collision', () => {
	const root = mkdtempSync(join(tmpdir(), 'parish-playwright-marker-'));
	const marker = join(root, '.playwright-ready-test');
	try {
		publishReadyMarker(marker, '0123456789abcdef', 'build-a');
		publishReadyMarker(marker, '0123456789abcdef', 'build-a');
		assert.throws(
			() => publishReadyMarker(marker, '0123456789abcdef', 'build-b'),
			/unexpected identity/,
		);
	} finally {
		rmSync(root, { force: true, recursive: true });
	}
});

test('artifact age policy eventually removes force-kill residue', () => {
	const root = mkdtempSync(join(tmpdir(), 'parish-playwright-expiry-'));
	const binary = join(root, 'parish-server-expired');
	writeFileSync(binary, 'expired');
	try {
		pruneServerArtifacts(root, {
			now: Date.now() + PLAYWRIGHT_ARTIFACT_MAX_AGE_MS + 1,
		});
		assert.equal(existsSync(binary), false);
	} finally {
		rmSync(root, { force: true, recursive: true });
	}
});
