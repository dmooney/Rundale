import { execFileSync, spawn, spawnSync } from 'node:child_process';
import { createHash, randomBytes } from 'node:crypto';
import {
	chmodSync,
	copyFileSync,
	linkSync,
	mkdirSync,
	readdirSync,
	readFileSync,
	realpathSync,
	renameSync,
	rmSync,
	statSync,
	writeFileSync,
} from 'node:fs';
import { hostname } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const UI_DIR = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const PARISH_DIR = resolve(UI_DIR, '../..');
const EXECUTABLE_SUFFIX = process.platform === 'win32' ? '.exe' : '';

export const PLAYWRIGHT_BUILD_LOCK_TIMEOUT_MS = 300_000;
export const PLAYWRIGHT_SERVER_TIMEOUT_MS = 420_000;
export const PLAYWRIGHT_SERVER_SHUTDOWN_TIMEOUT_MS = 10_000;
export const PLAYWRIGHT_LOCK_STALE_GRACE_MS = 15_000;

export function playwrightWebServerConfig(port) {
	return {
		command: `node scripts/playwright-worktree-server.js --port ${port}`,
		url: `http://localhost:${port}/api/world-snapshot`,
		timeout: PLAYWRIGHT_SERVER_TIMEOUT_MS,
		// Without this, Playwright force-kills the process group and the launcher
		// cannot remove its preserved worktree-specific server binary.
		gracefulShutdown: {
			signal: 'SIGTERM',
			timeout: PLAYWRIGHT_SERVER_SHUTDOWN_TIMEOUT_MS,
		},
		// A process from another worktree must never satisfy this test run.
		reuseExistingServer: false,
		env: { PARISH_MAX_SESSIONS: '500' },
	};
}

function commandOutput(command, args, cwd) {
	return execFileSync(command, args, {
		cwd,
		encoding: 'utf8',
		env: process.env,
	}).trim();
}

function delay(milliseconds) {
	return new Promise((resolveDelay) => setTimeout(resolveDelay, milliseconds));
}

export function worktreeKey(worktreeRoot) {
	return createHash('sha256')
		.update(resolve(worktreeRoot))
		.digest('hex')
		.slice(0, 16);
}

export function inlineScriptHashesFromHtml(html) {
	return [...html.matchAll(/<script[\t\n\r ]*>([\s\S]*?)<\/script>/gi)].map(
		(match) =>
			`'sha256-${createHash('sha256').update(match[1]).digest('base64')}'`,
	);
}

export function collectInlineScriptHashes(distDir) {
	const hashes = new Set();
	try {
		if (!statSync(distDir).isDirectory()) return [];
	} catch (error) {
		if (error?.code === 'ENOENT') return [];
		throw error;
	}
	const pending = [distDir];

	while (pending.length > 0) {
		const directory = pending.pop();
		let entries;
		try {
			entries = readdirSync(directory, { withFileTypes: true });
		} catch (error) {
			if (error?.code === 'ENOENT') continue;
			throw error;
		}
		for (const entry of entries) {
			const path = join(directory, entry.name);
			if (entry.isDirectory()) {
				pending.push(path);
			} else if (entry.isFile() && entry.name.endsWith('.html')) {
				for (const hash of inlineScriptHashesFromHtml(
					readFileSync(path, 'utf8'),
				)) {
					hashes.add(hash);
				}
			}
		}
	}

	return [...hashes].sort();
}

export function binaryContainsExpectedCsp(binary, expectedHashes) {
	return (
		expectedHashes.length > 0 &&
		expectedHashes.every((hash) => binary.includes(Buffer.from(hash)))
	);
}

export function uiDistFingerprint(expectedHashes) {
	return createHash('sha256').update(expectedHashes.join('\n')).digest('hex');
}

export function cargoRustcArgs(buildNonce) {
	return [
		'rustc',
		'--message-format=json-render-diagnostics',
		'-p',
		'parish-server',
		'--bin',
		'parish-server',
		'--',
		'-C',
		`metadata=playwright_${buildNonce}`,
	];
}

export function cargoExecutableFromMessages(output) {
	let executable;
	for (const line of output.split('\n')) {
		if (!line.trim()) continue;
		let message;
		try {
			message = JSON.parse(line);
		} catch {
			continue;
		}
		if (message.reason === 'compiler-message' && message.message?.rendered) {
			process.stderr.write(message.message.rendered);
		}
		if (
			message.reason === 'compiler-artifact' &&
			message.target?.name === 'parish-server' &&
			message.target?.kind?.includes('bin') &&
			message.executable
		) {
			executable = message.executable;
		}
	}
	return executable;
}

function processIsAlive(pid) {
	try {
		process.kill(pid, 0);
		return true;
	} catch (error) {
		return error?.code !== 'ESRCH';
	}
}

function readLockOwner(lockPath, lockStat) {
	const ownerPath = lockStat.isDirectory()
		? join(lockPath, 'owner.json')
		: lockPath;
	const owner = JSON.parse(readFileSync(ownerPath, 'utf8'));
	if (
		typeof owner.hostname !== 'string' ||
		!Number.isInteger(owner.pid) ||
		typeof owner.token !== 'string' ||
		owner.token.length === 0
	) {
		return undefined;
	}
	return owner;
}

function quarantineLock(lockPath) {
	const abandoned = `${lockPath}.abandoned-${process.pid}-${randomBytes(4).toString('hex')}`;
	try {
		renameSync(lockPath, abandoned);
		rmSync(abandoned, { force: true, recursive: true });
		return true;
	} catch (error) {
		if (error?.code === 'ENOENT') return true;
		throw error;
	}
}

export function removeAbandonedLock(
	lockPath,
	{ staleGraceMs = PLAYWRIGHT_LOCK_STALE_GRACE_MS, now = Date.now() } = {},
) {
	let lockStat;
	try {
		lockStat = statSync(lockPath);
	} catch (error) {
		if (error?.code === 'ENOENT') return true;
		throw error;
	}

	let owner;
	try {
		owner = readLockOwner(lockPath, lockStat);
	} catch (error) {
		if (error?.code !== 'ENOENT' && !(error instanceof SyntaxError))
			throw error;
		// Missing and corrupt owner metadata get a grace period before recovery.
		owner = undefined;
	}

	if (owner) {
		if (owner.hostname !== hostname()) return false;
		if (processIsAlive(owner.pid)) return false;
		return quarantineLock(lockPath);
	}

	if (now - lockStat.mtimeMs < staleGraceMs) return false;
	return quarantineLock(lockPath);
}

function tryCreateLock(lockPath, token) {
	const candidate = `${lockPath}.candidate-${process.pid}-${randomBytes(4).toString('hex')}`;
	writeFileSync(
		candidate,
		JSON.stringify({ hostname: hostname(), pid: process.pid, token }),
		{ flag: 'wx', mode: 0o600 },
	);
	try {
		// Hard-linking a fully-written candidate is an atomic create-if-absent.
		// There is never a visible lock with missing or partial owner metadata.
		linkSync(candidate, lockPath);
		return true;
	} catch (error) {
		if (error?.code === 'EEXIST') return false;
		throw error;
	} finally {
		rmSync(candidate, { force: true });
	}
}

/** Hold an outer lock through Cargo build, validation, and binary copy. */
export async function acquireBuildLock(
	lockPath,
	{
		timeoutMs = PLAYWRIGHT_BUILD_LOCK_TIMEOUT_MS,
		pollMs = 100,
		staleGraceMs = PLAYWRIGHT_LOCK_STALE_GRACE_MS,
	} = {},
) {
	mkdirSync(dirname(lockPath), { recursive: true });
	const deadline = Date.now() + timeoutMs;
	const token = randomBytes(16).toString('hex');

	while (Date.now() < deadline) {
		if (tryCreateLock(lockPath, token)) {
			return () => {
				try {
					const lockStat = statSync(lockPath);
					const owner = readLockOwner(lockPath, lockStat);
					if (owner?.token === token) rmSync(lockPath, { force: true });
				} catch (error) {
					if (error?.code !== 'ENOENT') throw error;
				}
			};
		}
		if (!removeAbandonedLock(lockPath, { staleGraceMs })) {
			await delay(pollMs);
		}
	}

	throw new Error(
		`timed out waiting for Playwright server build lock: ${lockPath}`,
	);
}

function effectiveCargoTarget() {
	const metadata = JSON.parse(
		commandOutput(
			'cargo',
			['metadata', '--format-version', '1', '--no-deps'],
			PARISH_DIR,
		),
	);
	return resolve(metadata.target_directory);
}

export function pruneOldCopies(
	directory,
	{
		now = Date.now(),
		readDirectory = readdirSync,
		statPath = statSync,
		removePath = rmSync,
	} = {},
) {
	const oldestAllowed = now - 24 * 60 * 60 * 1000;
	const pending = [directory];
	while (pending.length > 0) {
		const current = pending.pop();
		let entries;
		try {
			entries = readDirectory(current, { withFileTypes: true });
		} catch (error) {
			if (error?.code === 'ENOENT') continue;
			throw error;
		}
		for (const entry of entries) {
			const path = join(current, entry.name);
			if (entry.isDirectory()) {
				pending.push(path);
				continue;
			}
			if (!entry.isFile() || !entry.name.startsWith('parish-server-')) continue;
			try {
				if (statPath(path).mtimeMs < oldestAllowed) {
					removePath(path, { force: true });
				}
			} catch (error) {
				if (error?.code !== 'ENOENT') throw error;
			}
		}
	}
}

export async function prepareIsolatedServerBinary() {
	const worktreeRoot = realpathSync(
		commandOutput('git', ['rev-parse', '--show-toplevel'], PARISH_DIR),
	);
	const targetDir = effectiveCargoTarget();
	const expectedHashes = collectInlineScriptHashes(join(UI_DIR, 'dist'));
	if (expectedHashes.length === 0) {
		throw new Error(
			'UI dist has no inline script hashes; run the UI build before Playwright',
		);
	}

	const serverRoot = join(targetDir, 'playwright-servers');
	const outputDir = join(serverRoot, worktreeKey(worktreeRoot));
	pruneOldCopies(serverRoot);
	mkdirSync(outputDir, { recursive: true });
	const releaseLock = await acquireBuildLock(
		join(targetDir, '.playwright-parish-server-build.lock'),
	);

	try {
		for (let attempt = 1; attempt <= 3; attempt += 1) {
			const nonce = `${process.pid}_${Date.now()}_${randomBytes(6).toString('hex')}`;
			const build = spawnSync('cargo', cargoRustcArgs(nonce), {
				cwd: PARISH_DIR,
				encoding: 'utf8',
				env: {
					...process.env,
					PARISH_UI_DIST_DIGEST: uiDistFingerprint(expectedHashes),
				},
				maxBuffer: 64 * 1024 * 1024,
				stdio: ['ignore', 'pipe', 'inherit'],
			});
			if (build.error) throw build.error;
			const source = cargoExecutableFromMessages(build.stdout ?? '');
			if (build.status !== 0) {
				throw new Error(`cargo rustc failed with status ${build.status}`);
			}
			if (!source) {
				throw new Error(
					'cargo rustc did not report a parish-server executable',
				);
			}
			const destination = join(
				outputDir,
				`parish-server-${nonce}${EXECUTABLE_SUFFIX}`,
			);
			const temporary = `${destination}.tmp`;
			copyFileSync(source, temporary);
			chmodSync(temporary, statSync(source).mode);
			renameSync(temporary, destination);

			if (
				binaryContainsExpectedCsp(readFileSync(destination), expectedHashes)
			) {
				return { expectedHashes, path: destination };
			}

			rmSync(destination, { force: true });
			if (attempt === 3) {
				throw new Error(
					"preserved parish-server binary did not contain this worktree's CSP hashes",
				);
			}
		}
	} finally {
		releaseLock();
	}

	throw new Error('unreachable: parish-server preparation exhausted retries');
}

export function assertServedCspCoherent(html, csp, expectedHashes) {
	const scriptDirective = csp
		.split(';')
		.map((directive) => directive.trim())
		.find((directive) => directive.startsWith('script-src '));
	if (!scriptDirective)
		throw new Error('served response has no script-src CSP');
	if (scriptDirective.includes("'unsafe-inline'")) {
		throw new Error("served script-src unexpectedly allows 'unsafe-inline'");
	}

	const allowedHashes = [...scriptDirective.matchAll(/'sha256-[^']+'/g)]
		.map((match) => match[0])
		.sort();
	const expected = [...new Set(expectedHashes)].sort();
	if (JSON.stringify(allowedHashes) !== JSON.stringify(expected)) {
		throw new Error(
			'served script-src hashes do not match the invoking UI dist',
		);
	}

	const servedHtmlHashes = [...new Set(inlineScriptHashesFromHtml(html))];
	if (
		servedHtmlHashes.length === 0 ||
		!servedHtmlHashes.every((hash) => allowedHashes.includes(hash))
	) {
		throw new Error('served HTML contains an inline script not allowed by CSP');
	}
}

async function waitForServedCsp(port, expectedHashes, server) {
	const deadline = Date.now() + 30_000;
	let lastError;
	while (Date.now() < deadline) {
		if (server.exitCode !== null) {
			throw new Error(`parish-server exited before CSP validation`);
		}
		try {
			const response = await fetch(`http://127.0.0.1:${port}/`);
			if (response.ok) {
				const csp = response.headers.get('content-security-policy') ?? '';
				assertServedCspCoherent(await response.text(), csp, expectedHashes);
				return;
			}
			lastError = new Error(`server returned HTTP ${response.status}`);
		} catch (error) {
			lastError = error;
		}
		await delay(100);
	}
	throw new Error(
		`timed out validating served CSP/HTML coherence: ${lastError?.message ?? 'server unavailable'}`,
	);
}

export function superviseServer(
	command,
	args,
	{ cwd = PARISH_DIR, env = process.env, preservedBinary } = {},
) {
	const server = spawn(command, args, { cwd, env, stdio: 'inherit' });
	let cleaned = false;
	const cleanup = () => {
		if (!preservedBinary || cleaned) return;
		try {
			rmSync(preservedBinary, { force: true });
			cleaned = true;
		} catch (error) {
			if (error?.code !== 'EBUSY' && error?.code !== 'EPERM') throw error;
		}
	};

	const signalHandlers = new Map();
	for (const signal of ['SIGINT', 'SIGTERM']) {
		const handler = () => {
			cleanup();
			server.kill(signal);
		};
		signalHandlers.set(signal, handler);
		process.once(signal, handler);
	}
	process.once('exit', cleanup);

	const unregister = () => {
		for (const [signal, handler] of signalHandlers) {
			process.removeListener(signal, handler);
		}
	};
	server.once('error', (error) => {
		cleanup();
		unregister();
		console.error(error);
		process.exitCode = 1;
	});
	server.once('exit', (code, signal) => {
		cleanup();
		unregister();
		process.exitCode = code ?? (signal ? 0 : 1);
	});

	return { cleanup, server };
}

function parsePort(args) {
	const index = args.indexOf('--port');
	const port = index >= 0 ? args[index + 1] : undefined;
	if (!port || !/^\d+$/.test(port)) {
		throw new Error(
			'usage: node scripts/playwright-worktree-server.js --port PORT',
		);
	}
	return port;
}

async function main() {
	const port = parsePort(process.argv.slice(2));
	let command;
	let args;
	let preservedBinary;
	let expectedHashes;

	if (process.env.GITHUB_ACTIONS === 'true') {
		// GitHub-hosted jobs have an isolated filesystem and already prebuild this
		// exact binary, so keep the established Cargo cache path there.
		command = 'cargo';
		args = ['run', '-p', 'parish-server', '--', '--port', port];
	} else {
		const prepared = await prepareIsolatedServerBinary();
		preservedBinary = prepared.path;
		expectedHashes = prepared.expectedHashes;
		command = preservedBinary;
		args = ['--port', port];
	}

	const supervision = superviseServer(command, args, { preservedBinary });
	if (expectedHashes) {
		try {
			await waitForServedCsp(port, expectedHashes, supervision.server);
		} catch (error) {
			supervision.cleanup();
			supervision.server.kill('SIGTERM');
			process.exitCode = 1;
			throw error;
		}
	}
}

if (
	process.argv[1] &&
	pathToFileURL(realpathSync(process.argv[1])).href === import.meta.url
) {
	await main();
}
