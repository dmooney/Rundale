import { execFileSync, spawn } from 'node:child_process';
import { createHash, randomBytes } from 'node:crypto';
import {
	chmodSync,
	closeSync,
	fstatSync,
	futimesSync,
	mkdirSync,
	openSync,
	readdirSync,
	readFileSync,
	realpathSync,
	renameSync,
	rmdirSync,
	rmSync,
	statSync,
	utimesSync,
	writeFileSync,
} from 'node:fs';
import { createServer as createNetServer } from 'node:net';
import { hostname } from 'node:os';
import {
	basename,
	dirname,
	isAbsolute,
	join,
	relative,
	resolve,
	sep,
} from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const UI_DIR = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const PARISH_DIR = resolve(UI_DIR, '../..');
const EXECUTABLE_SUFFIX = process.platform === 'win32' ? '.exe' : '';

export const PLAYWRIGHT_BUILD_LOCK_TIMEOUT_MS = 300_000;
export const PLAYWRIGHT_SERVER_TIMEOUT_MS = 420_000;
export const PLAYWRIGHT_SERVER_SHUTDOWN_TIMEOUT_MS = 10_000;
export const PLAYWRIGHT_LOCK_STALE_GRACE_MS = 15_000;
export const PLAYWRIGHT_LOCK_HEARTBEAT_MS = 2_000;
export const PLAYWRIGHT_ACTIVE_USE_STALE_GRACE_MS = 15_000;
export const PLAYWRIGHT_ACTIVE_USE_HEARTBEAT_MS = 2_000;
export const PLAYWRIGHT_ARTIFACT_MAX_AGE_MS = 24 * 60 * 60 * 1_000;
export const PLAYWRIGHT_MAX_CACHED_ARTIFACTS = 3;
export const PLAYWRIGHT_BUILD_ID_HEADER = 'x-parish-playwright-build-id';

const RUN_ID_PATTERN = /^[A-Za-z0-9_-]{16,128}$/;
const ACTIVE_USE_LEASE_PREFIX = '.playwright-server-active-';
const ACTIVE_USE_TOMBSTONE_PREFIX = '.playwright-server-retired-';
const ACTIVE_USE_LEASE_SUFFIX = '.json';
const NPM_EXEC_PATH_OVERRIDE = 'PARISH_PLAYWRIGHT_NPM_EXEC_PATH';

export async function allocateLoopbackPort() {
	const reservation = createNetServer();
	reservation.unref();
	await new Promise((resolveListen, rejectListen) => {
		reservation.once('error', rejectListen);
		reservation.listen(0, '127.0.0.1', resolveListen);
	});
	const address = reservation.address();
	if (!address || typeof address === 'string') {
		reservation.close();
		throw new Error('failed to allocate a loopback port for Playwright');
	}
	await new Promise((resolveClose, rejectClose) => {
		reservation.close((error) => (error ? rejectClose(error) : resolveClose()));
	});
	return address.port;
}

export async function resolvePlaywrightPort(
	environment = process.env,
	allocate = allocateLoopbackPort,
) {
	if (environment.PARISH_TEST_PORT) return environment.PARISH_TEST_PORT;
	const port = await allocate();
	environment.PARISH_TEST_PORT = String(port);
	return environment.PARISH_TEST_PORT;
}

export function playwrightWebServerConfig(
	port,
	{ platform = process.platform, runId = randomBytes(16).toString('hex') } = {},
) {
	if (!RUN_ID_PATTERN.test(runId)) {
		throw new Error(
			'Playwright run identity must be 16-128 URL-safe characters',
		);
	}
	const config = {
		command: `node scripts/playwright-worktree-server.js --port ${port}`,
		url: `http://127.0.0.1:${port}/api/playwright-ready/${runId}`,
		timeout: PLAYWRIGHT_SERVER_TIMEOUT_MS,
		// A process from another worktree must never satisfy this test run.
		reuseExistingServer: false,
		env: {
			PARISH_MAX_SESSIONS: '500',
			PARISH_PLAYWRIGHT_RUN_ID: runId,
		},
	};
	// Playwright explicitly ignores gracefulShutdown on Windows and taskkills
	// the process tree. Artifact correctness therefore never depends on this;
	// it remains a polite shutdown optimization only on POSIX hosts.
	if (platform !== 'win32') {
		config.gracefulShutdown = {
			signal: 'SIGTERM',
			timeout: PLAYWRIGHT_SERVER_SHUTDOWN_TIMEOUT_MS,
		};
	}
	return config;
}

export function npmBuildCommand({
	environment = process.env,
	execPath = process.execPath,
	stat = statSync,
} = {}) {
	const override = environment[NPM_EXEC_PATH_OVERRIDE];
	const npmExecPath = override || environment.npm_execpath;
	const source = override ? NPM_EXEC_PATH_OVERRIDE : 'npm_execpath';
	if (!npmExecPath) {
		throw new Error(
			`npm_execpath is unavailable; run Playwright through npm/npx or set ${NPM_EXEC_PATH_OVERRIDE}`,
		);
	}
	if (!isAbsolute(npmExecPath)) {
		throw new Error(
			`${source} must be an absolute path to npm's JavaScript entry point`,
		);
	}
	if (!/\.(?:cjs|mjs|js)$/i.test(npmExecPath)) {
		throw new Error(`${source} must name a JavaScript entry point`);
	}
	let stats;
	try {
		stats = stat(npmExecPath);
	} catch (error) {
		throw new Error(`${source} does not exist: ${npmExecPath}`, {
			cause: error,
		});
	}
	if (!stats.isFile()) {
		throw new Error(`${source} is not a file: ${npmExecPath}`);
	}
	return [execPath, npmExecPath];
}

export function buildUiDist({
	cwd = UI_DIR,
	environment = process.env,
	execPath = process.execPath,
	resolveCommand = npmBuildCommand,
	runCommand = execFileSync,
} = {}) {
	const [command, ...prefixArgs] = resolveCommand({
		environment,
		execPath,
	});
	return runCommand(command, [...prefixArgs, 'run', 'build'], {
		cwd,
		env: environment,
		shell: false,
		stdio: 'inherit',
		windowsHide: true,
	});
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

export function captureUiDist(distDir) {
	try {
		if (!statSync(distDir).isDirectory()) return undefined;
	} catch (error) {
		if (error?.code === 'ENOENT') return undefined;
		throw error;
	}

	const files = [];
	const pending = [{ absolute: distDir, relative: '' }];
	while (pending.length > 0) {
		const current = pending.pop();
		let entries;
		try {
			entries = readdirSync(current.absolute, { withFileTypes: true }).sort(
				(left, right) => left.name.localeCompare(right.name),
			);
		} catch (error) {
			if (error?.code === 'ENOENT') return undefined;
			throw error;
		}
		for (const entry of entries) {
			const relative = current.relative
				? `${current.relative}/${entry.name}`
				: entry.name;
			const absolute = join(current.absolute, entry.name);
			if (entry.isDirectory()) {
				pending.push({ absolute, relative });
				continue;
			}
			if (!entry.isFile()) {
				throw new Error(`UI dist contains unsupported entry: ${relative}`);
			}
			try {
				files.push({
					contents: readFileSync(absolute),
					mode: statSync(absolute).mode,
					relative,
				});
			} catch (error) {
				if (error?.code === 'ENOENT') return undefined;
				throw error;
			}
		}
	}

	files.sort((left, right) => left.relative.localeCompare(right.relative));
	const digest = createHash('sha256');
	const hashes = new Set();
	for (const file of files) {
		digest.update(file.relative);
		digest.update('\0');
		digest.update(String(file.contents.length));
		digest.update('\0');
		digest.update(file.contents);
		digest.update('\0');
		if (file.relative.toLowerCase().endsWith('.html')) {
			for (const hash of inlineScriptHashesFromHtml(
				file.contents.toString('utf8'),
			)) {
				hashes.add(hash);
			}
		}
	}

	return {
		expectedHashes: [...hashes].sort(),
		files,
		fingerprint: digest.digest('hex'),
	};
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

export function playwrightBuildIdentity(worktreeRoot, uiFingerprint) {
	return `pw-${worktreeKey(worktreeRoot)}-${uiFingerprint}`;
}

export function binaryContainsExpectedBuildIdentity(binary, buildIdentity) {
	return binary.includes(Buffer.from(buildIdentity));
}

export function binaryContentDigest(binary) {
	return createHash('sha256').update(binary).digest('hex');
}

export function cargoBuildArgs() {
	return [
		'build',
		'--message-format=json-render-diagnostics',
		'-p',
		'parish-server',
		'--bin',
		'parish-server',
	];
}

export function runCargoBuild(
	args,
	{ cwd = PARISH_DIR, env = process.env, maxBuffer = 64 * 1024 * 1024 } = {},
) {
	return new Promise((resolveBuild, rejectBuild) => {
		const child = spawn('cargo', args, {
			cwd,
			env,
			stdio: ['ignore', 'pipe', 'inherit'],
		});
		const chunks = [];
		let length = 0;
		let settled = false;
		child.stdout.on('data', (chunk) => {
			length += chunk.length;
			if (length > maxBuffer) {
				child.kill('SIGKILL');
				if (!settled) {
					settled = true;
					rejectBuild(new Error('cargo JSON output exceeded 64 MiB'));
				}
				return;
			}
			chunks.push(chunk);
		});
		child.once('error', (error) => {
			if (settled) return;
			settled = true;
			rejectBuild(error);
		});
		child.once('close', (status) => {
			if (settled) return;
			settled = true;
			resolveBuild({
				status,
				stdout: Buffer.concat(chunks).toString('utf8'),
			});
		});
	});
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

	// A heartbeat lease is authoritative. PID existence is not: PIDs are
	// reused, and a lock may be shared by containers or hosts with unrelated
	// process namespaces. Fresh malformed files receive the same bounded grace.
	if (now - lockStat.mtimeMs < staleGraceMs) return false;
	return quarantineLock(lockPath);
}

function tryCreateLock(lockPath, token) {
	let descriptor;
	try {
		descriptor = openSync(lockPath, 'wx', 0o600);
	} catch (error) {
		if (error?.code === 'EEXIST') return undefined;
		throw error;
	}
	try {
		writeFileSync(
			descriptor,
			JSON.stringify({ hostname: hostname(), pid: process.pid, token }),
		);
		return descriptor;
	} catch (error) {
		closeSync(descriptor);
		rmSync(lockPath, { force: true });
		throw error;
	}
}

/** Hold an outer lock through Cargo build, validation, and binary copy. */
export async function acquireBuildLock(
	lockPath,
	{
		timeoutMs = PLAYWRIGHT_BUILD_LOCK_TIMEOUT_MS,
		pollMs = 100,
		staleGraceMs = PLAYWRIGHT_LOCK_STALE_GRACE_MS,
		heartbeatMs = PLAYWRIGHT_LOCK_HEARTBEAT_MS,
	} = {},
) {
	if (heartbeatMs <= 0 || heartbeatMs * 3 >= staleGraceMs) {
		throw new Error(
			'build-lock heartbeat must be less than one-third of its stale grace',
		);
	}
	mkdirSync(dirname(lockPath), { recursive: true });
	const deadline = Date.now() + timeoutMs;
	const token = randomBytes(16).toString('hex');

	while (Date.now() < deadline) {
		const descriptor = tryCreateLock(lockPath, token);
		if (descriptor !== undefined) {
			let leaseError;
			const refresh = () => {
				try {
					const now = new Date();
					futimesSync(descriptor, now, now);
				} catch (error) {
					leaseError = error;
				}
			};
			const heartbeat = setInterval(refresh, heartbeatMs);
			heartbeat.unref();
			return {
				assertOwned() {
					if (leaseError) throw leaseError;
					const lockStat = statSync(lockPath);
					const owner = readLockOwner(lockPath, lockStat);
					if (owner?.token !== token) {
						throw new Error('Playwright server build lock lease was lost');
					}
				},
				release() {
					clearInterval(heartbeat);
					closeSync(descriptor);
					try {
						const lockStat = statSync(lockPath);
						const owner = readLockOwner(lockPath, lockStat);
						if (owner?.token === token) rmSync(lockPath, { force: true });
					} catch (error) {
						if (error?.code !== 'ENOENT') throw error;
					}
				},
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

function removePrunable(path, removePath = rmSync) {
	try {
		removePath(path, { force: true, recursive: true });
		return true;
	} catch (error) {
		if (['EBUSY', 'ENOENT', 'EPERM'].includes(error?.code)) return false;
		throw error;
	}
}

function isServerArtifactPath(serverRoot, path) {
	const root = resolve(serverRoot);
	const artifact = resolve(path);
	const fromRoot = relative(root, artifact);
	return (
		fromRoot !== '' &&
		fromRoot !== '..' &&
		!fromRoot.startsWith(`..${sep}`) &&
		!isAbsolute(fromRoot)
	);
}

function activeUseLeaseToken(name) {
	if (
		!name.startsWith(ACTIVE_USE_LEASE_PREFIX) ||
		!name.endsWith(ACTIVE_USE_LEASE_SUFFIX)
	)
		return undefined;
	const token = name.slice(
		ACTIVE_USE_LEASE_PREFIX.length,
		-ACTIVE_USE_LEASE_SUFFIX.length,
	);
	return /^[a-f0-9]{32}$/.test(token) ? token : undefined;
}

function retiredActiveUseLeaseIdentity(name) {
	if (
		!name.startsWith(ACTIVE_USE_TOMBSTONE_PREFIX) ||
		!name.endsWith(ACTIVE_USE_LEASE_SUFFIX)
	)
		return undefined;
	const identity = name.slice(
		ACTIVE_USE_TOMBSTONE_PREFIX.length,
		-ACTIVE_USE_LEASE_SUFFIX.length,
	);
	const match = identity.match(/^([a-f0-9]{32})-([0-9]{1,16})-[a-f0-9]{8}$/);
	if (!match) return undefined;
	const retiredAtMs = Number(match[2]);
	return Number.isSafeInteger(retiredAtMs)
		? { retiredAtMs, token: match[1] }
		: undefined;
}

function readActiveUseLease(leasePath, serverRoot, expectedToken) {
	const payload = JSON.parse(readFileSync(leasePath, 'utf8'));
	if (
		payload?.version !== 1 ||
		payload.token !== expectedToken ||
		typeof payload.hostname !== 'string' ||
		!Number.isInteger(payload.pid) ||
		!Array.isArray(payload.artifacts) ||
		payload.artifacts.length !== 2 ||
		!payload.artifacts.every((path) => typeof path === 'string')
	) {
		return undefined;
	}

	const artifacts = [
		...new Set(payload.artifacts.map((path) => resolve(path))),
	];
	if (
		artifacts.length !== 2 ||
		!artifacts.every((path) => isServerArtifactPath(serverRoot, path)) ||
		!artifacts.some((path) => basename(path).startsWith('parish-server-')) ||
		!artifacts.some((path) => basename(path).startsWith('ui-dist-'))
	) {
		return undefined;
	}
	return { ...payload, artifacts };
}

/**
 * Publish the binary and UI snapshot used by one live managed server.
 *
 * Production callers create this while holding the global build lock. The
 * unique pathname plus token check makes release owner-specific, while the
 * heartbeat lets later pruners recover residue from an abruptly killed owner.
 */
export function publishActiveUseLease(
	serverRoot,
	artifacts,
	{
		heartbeatMs = PLAYWRIGHT_ACTIVE_USE_HEARTBEAT_MS,
		staleGraceMs = PLAYWRIGHT_ACTIVE_USE_STALE_GRACE_MS,
	} = {},
) {
	if (heartbeatMs <= 0 || heartbeatMs * 3 >= staleGraceMs) {
		throw new Error(
			'active-use heartbeat must be less than one-third of its stale grace',
		);
	}
	const root = resolve(serverRoot);
	const resolvedArtifacts = [
		...new Set(artifacts.map((path) => resolve(path))),
	];
	if (
		resolvedArtifacts.length !== 2 ||
		!resolvedArtifacts.every((path) => isServerArtifactPath(root, path)) ||
		!resolvedArtifacts.some((path) =>
			basename(path).startsWith('parish-server-'),
		) ||
		!resolvedArtifacts.some((path) => basename(path).startsWith('ui-dist-'))
	) {
		throw new Error(
			'active-use lease must contain one cached server binary and one UI snapshot',
		);
	}

	mkdirSync(root, { recursive: true });
	const token = randomBytes(16).toString('hex');
	const leasePath = join(
		root,
		`${ACTIVE_USE_LEASE_PREFIX}${token}${ACTIVE_USE_LEASE_SUFFIX}`,
	);
	const descriptor = openSync(leasePath, 'wx+', 0o600);
	try {
		writeFileSync(
			descriptor,
			JSON.stringify({
				artifacts: resolvedArtifacts,
				hostname: hostname(),
				pid: process.pid,
				token,
				version: 1,
			}),
		);
	} catch (error) {
		closeSync(descriptor);
		rmSync(leasePath, { force: true });
		throw error;
	}

	let leaseError;
	let notifyLeaseLost;
	const lost = new Promise((resolveLost) => {
		notifyLeaseLost = resolveLost;
	});
	let released = false;
	const recordLeaseLoss = (error) => {
		if (leaseError) return;
		leaseError = error;
		notifyLeaseLost(error);
	};
	const assertLeaseFileOwned = () => {
		if (released)
			throw new Error('Playwright server active-use lease was released');
		const descriptorStat = fstatSync(descriptor);
		const pathStat = statSync(leasePath);
		const owner = readActiveUseLease(leasePath, root, token);
		if (
			!owner ||
			descriptorStat.dev !== pathStat.dev ||
			descriptorStat.ino !== pathStat.ino
		) {
			throw new Error('Playwright server active-use lease was lost');
		}
	};
	const refresh = () => {
		try {
			assertLeaseFileOwned();
			const now = new Date();
			futimesSync(descriptor, now, now);
		} catch (error) {
			recordLeaseLoss(error);
		}
	};
	const heartbeat = setInterval(refresh, heartbeatMs);
	heartbeat.unref();

	return {
		artifacts: resolvedArtifacts,
		lost,
		path: leasePath,
		assertOwned() {
			if (leaseError) throw leaseError;
			try {
				assertLeaseFileOwned();
			} catch (error) {
				recordLeaseLoss(error);
				throw error;
			}
		},
		release() {
			if (released) return;
			released = true;
			clearInterval(heartbeat);
			closeSync(descriptor);
			try {
				const owner = readActiveUseLease(leasePath, root, token);
				if (owner?.token === token) rmSync(leasePath, { force: true });
			} catch (error) {
				// A replaced or truncated lease is no longer ours to unlink. Leave it
				// for bounded tombstone recovery instead of deleting by pathname.
				if (error?.code !== 'ENOENT' && !(error instanceof SyntaxError)) {
					throw error;
				}
			}
		},
	};
}

function collectActiveUseLeasePaths(
	serverRoot,
	{
		leaseStaleGraceMs,
		now,
		readDirectory,
		removePath,
		renamePath,
		statPath,
		touchPath,
	},
) {
	const keepPaths = new Set();
	let pruningBlocked = false;
	let entries;
	try {
		entries = readDirectory(serverRoot, { withFileTypes: true });
	} catch (error) {
		if (error?.code === 'ENOENT') return { keepPaths, pruningBlocked };
		throw error;
	}

	for (const entry of entries) {
		const isActive =
			entry.name.startsWith(ACTIVE_USE_LEASE_PREFIX) &&
			entry.name.endsWith(ACTIVE_USE_LEASE_SUFFIX);
		const isRetired =
			entry.name.startsWith(ACTIVE_USE_TOMBSTONE_PREFIX) &&
			entry.name.endsWith(ACTIVE_USE_LEASE_SUFFIX);
		if (!isActive && !isRetired) continue;
		const leasePath = join(serverRoot, entry.name);
		let leaseStat;
		try {
			leaseStat = statPath(leasePath);
		} catch (error) {
			if (error?.code === 'ENOENT') continue;
			throw error;
		}

		const retiredIdentity = isRetired
			? retiredActiveUseLeaseIdentity(entry.name)
			: undefined;
		const token = isActive
			? activeUseLeaseToken(entry.name)
			: retiredIdentity?.token;
		let lease;
		try {
			lease = token
				? readActiveUseLease(leasePath, serverRoot, token)
				: undefined;
		} catch (error) {
			if (error?.code === 'ENOENT') continue;
			if (!(error instanceof SyntaxError)) throw error;
		}

		// The retirement timestamp in the owned tombstone name is authoritative
		// if a platform renames the open lease but cannot refresh its mtime.
		const freshSinceMs = retiredIdentity
			? Math.max(leaseStat.mtimeMs, retiredIdentity.retiredAtMs)
			: leaseStat.mtimeMs;
		const stale = now - freshSinceMs >= leaseStaleGraceMs;
		if (isRetired) {
			if (stale) {
				// Removing a tombstone never authorizes artifact deletion in the same
				// pass. A later pruner may reclaim after the owner had a full grace
				// period to observe lease loss and fence its child.
				removePrunable(leasePath, removePath);
				pruningBlocked = true;
				continue;
			}
			if (lease) {
				for (const path of lease.artifacts) keepPaths.add(path);
			} else {
				pruningBlocked = true;
			}
			continue;
		}

		if (stale) {
			const retiredToken = token ?? randomBytes(16).toString('hex');
			const retiredAtMs = Math.max(0, Math.trunc(now));
			const tombstonePath = join(
				serverRoot,
				`${ACTIVE_USE_TOMBSTONE_PREFIX}${retiredToken}-${retiredAtMs}-${randomBytes(4).toString('hex')}${ACTIVE_USE_LEASE_SUFFIX}`,
			);
			try {
				renamePath(leasePath, tombstonePath);
				const retiredAt = new Date(retiredAtMs);
				touchPath(tombstonePath, retiredAt, retiredAt);
			} catch (error) {
				if (!['EBUSY', 'ENOENT', 'EPERM'].includes(error?.code)) throw error;
				// An uncertain retirement cannot authorize artifact deletion.
				pruningBlocked = true;
			}
			if (lease) {
				for (const path of lease.artifacts) keepPaths.add(path);
			} else {
				pruningBlocked = true;
			}
			continue;
		}

		if (lease) {
			for (const path of lease.artifacts) keepPaths.add(path);
		} else {
			// A fresh partial or invalid lease may still belong to a publisher.
			// Fail closed until it can transition through bounded retirement.
			pruningBlocked = true;
		}
	}
	return { keepPaths, pruningBlocked };
}

export function pruneLegacyLockCandidates(
	targetDir,
	{
		now = Date.now(),
		readDirectory = readdirSync,
		statPath = statSync,
		removePath = rmSync,
		staleGraceMs = PLAYWRIGHT_LOCK_STALE_GRACE_MS,
	} = {},
) {
	let entries;
	try {
		entries = readDirectory(targetDir, { withFileTypes: true });
	} catch (error) {
		if (error?.code === 'ENOENT') return;
		throw error;
	}
	for (const entry of entries) {
		if (
			!entry.name.startsWith(
				'.playwright-parish-server-build.lock.candidate-',
			) &&
			!entry.name.startsWith('.playwright-parish-server-build.lock.abandoned-')
		)
			continue;
		const path = join(targetDir, entry.name);
		try {
			if (now - statPath(path).mtimeMs >= staleGraceMs) {
				removePrunable(path, removePath);
			}
		} catch (error) {
			if (error?.code !== 'ENOENT') throw error;
		}
	}
}

function pruneCacheGroup(
	artifacts,
	{ keepPaths, maxArtifacts, now, removePath, staleGraceMs },
) {
	const retained = artifacts
		.filter((artifact) => {
			if (keepPaths.has(resolve(artifact.path))) return true;
			if (now - artifact.mtimeMs >= PLAYWRIGHT_ARTIFACT_MAX_AGE_MS) {
				return !removePrunable(artifact.path, removePath);
			}
			return true;
		})
		.sort((left, right) => right.mtimeMs - left.mtimeMs);

	let kept = retained.filter((artifact) =>
		keepPaths.has(resolve(artifact.path)),
	).length;
	for (const artifact of retained) {
		if (keepPaths.has(resolve(artifact.path))) continue;
		kept += 1;
		if (kept > maxArtifacts && now - artifact.mtimeMs >= staleGraceMs) {
			removePrunable(artifact.path, removePath);
		}
	}
}

export function pruneServerArtifacts(
	serverRoot,
	{
		keepPaths: keep = [],
		leaseStaleGraceMs = PLAYWRIGHT_ACTIVE_USE_STALE_GRACE_MS,
		maxArtifacts = PLAYWRIGHT_MAX_CACHED_ARTIFACTS,
		now = Date.now(),
		readDirectory = readdirSync,
		removeEmptyDirectory = rmdirSync,
		statPath = statSync,
		removePath = rmSync,
		renamePath = renameSync,
		staleGraceMs = PLAYWRIGHT_LOCK_STALE_GRACE_MS,
		touchPath = utimesSync,
	} = {},
) {
	const keepPaths = new Set(keep.map((path) => resolve(path)));
	const activeUse = collectActiveUseLeasePaths(serverRoot, {
		leaseStaleGraceMs,
		now,
		readDirectory,
		removePath,
		renamePath,
		statPath,
		touchPath,
	});
	if (activeUse.pruningBlocked) return;
	for (const path of activeUse.keepPaths) keepPaths.add(path);
	const root = resolve(serverRoot);
	const pending = [{ cleanup: false, path: serverRoot }];
	while (pending.length > 0) {
		const item = pending.pop();
		const current = item.path;
		if (item.cleanup) {
			if (resolve(current) !== root) {
				try {
					removeEmptyDirectory(current);
				} catch (error) {
					if (
						!['EBUSY', 'ENOENT', 'ENOTEMPTY', 'EPERM'].includes(error?.code)
					) {
						throw error;
					}
				}
			}
			continue;
		}
		let entries;
		try {
			entries = readDirectory(current, { withFileTypes: true });
		} catch (error) {
			if (error?.code === 'ENOENT') continue;
			throw error;
		}
		pending.push({ cleanup: true, path: current });

		const binaries = [];
		const snapshots = [];
		for (const entry of entries) {
			const path = join(current, entry.name);
			let pathStat;
			try {
				pathStat = statPath(path);
			} catch (error) {
				if (error?.code === 'ENOENT') continue;
				throw error;
			}
			if (entry.name.includes('.candidate-')) {
				if (now - pathStat.mtimeMs >= staleGraceMs) {
					removePrunable(path, removePath);
				}
				continue;
			}
			if (entry.name.startsWith('.playwright-ready-')) {
				if (now - pathStat.mtimeMs >= PLAYWRIGHT_ARTIFACT_MAX_AGE_MS) {
					removePrunable(path, removePath);
				}
				continue;
			}
			if (entry.isFile() && entry.name.startsWith('parish-server-')) {
				binaries.push({ path, mtimeMs: pathStat.mtimeMs });
				continue;
			}
			if (entry.isDirectory() && entry.name.startsWith('ui-dist-')) {
				snapshots.push({ path, mtimeMs: pathStat.mtimeMs });
				continue;
			}
			if (entry.isDirectory()) pending.push({ cleanup: false, path });
		}

		for (const artifacts of [binaries, snapshots]) {
			pruneCacheGroup(artifacts, {
				keepPaths,
				maxArtifacts,
				now,
				removePath,
				staleGraceMs,
			});
		}
	}
}

export function publishUiSnapshot(outputDir, capture) {
	const destination = join(outputDir, `ui-dist-${capture.fingerprint}`);
	const existing = captureUiDist(destination);
	if (existing) {
		if (existing.fingerprint !== capture.fingerprint) {
			throw new Error(
				'cached Playwright UI snapshot failed content validation',
			);
		}
		return destination;
	}

	const candidate = join(
		outputDir,
		`.ui-dist-${capture.fingerprint}.candidate-${process.pid}-${randomBytes(6).toString('hex')}`,
	);
	mkdirSync(candidate, { recursive: false });
	try {
		for (const file of capture.files) {
			const path = join(candidate, ...file.relative.split('/'));
			mkdirSync(dirname(path), { recursive: true });
			writeFileSync(path, file.contents, { mode: file.mode });
		}
		const copied = captureUiDist(candidate);
		if (copied?.fingerprint !== capture.fingerprint) {
			throw new Error(
				'copied Playwright UI snapshot failed content validation',
			);
		}
		try {
			renameSync(candidate, destination);
		} catch (error) {
			if (!['EEXIST', 'ENOTEMPTY'].includes(error?.code)) throw error;
			const raced = captureUiDist(destination);
			if (raced?.fingerprint !== capture.fingerprint) throw error;
		}
		return destination;
	} finally {
		rmSync(candidate, { force: true, recursive: true });
	}
}

export function publishCachedBinary(outputDir, binary, mode) {
	const digest = binaryContentDigest(binary);
	const destination = join(
		outputDir,
		`parish-server-${digest}${EXECUTABLE_SUFFIX}`,
	);
	try {
		const existing = readFileSync(destination);
		if (binaryContentDigest(existing) !== digest) {
			throw new Error('content-addressed Playwright server cache is corrupt');
		}
		return destination;
	} catch (error) {
		if (error?.code !== 'ENOENT') throw error;
	}

	const candidate = `${destination}.candidate-${process.pid}-${randomBytes(6).toString('hex')}`;
	try {
		writeFileSync(candidate, binary, { flag: 'wx', mode });
		chmodSync(candidate, mode);
		try {
			renameSync(candidate, destination);
		} catch (error) {
			if (!['EEXIST', 'ENOTEMPTY'].includes(error?.code)) throw error;
			const raced = readFileSync(destination);
			if (binaryContentDigest(raced) !== digest) throw error;
		}
		return destination;
	} finally {
		rmSync(candidate, { force: true });
	}
}

export async function prepareIsolatedServerBinary({
	afterCargoBuild,
	cargoRunner = runCargoBuild,
	maxAttempts = 3,
} = {}) {
	const worktreeRoot = realpathSync(
		commandOutput('git', ['rev-parse', '--show-toplevel'], PARISH_DIR),
	);
	const targetDir = effectiveCargoTarget();
	const serverRoot = join(targetDir, 'playwright-servers');
	const outputDir = join(serverRoot, worktreeKey(worktreeRoot));
	mkdirSync(outputDir, { recursive: true });
	pruneLegacyLockCandidates(targetDir);

	const lease = await acquireBuildLock(
		join(targetDir, '.playwright-parish-server-build.lock'),
	);
	try {
		pruneServerArtifacts(serverRoot);
		for (let attempt = 1; attempt <= maxAttempts; attempt += 1) {
			lease.assertOwned();
			const capture = captureUiDist(join(UI_DIR, 'dist'));
			if (!capture || capture.expectedHashes.length === 0) {
				throw new Error(
					'UI dist has no inline script hashes; run the UI build before Playwright',
				);
			}
			const staticDir = publishUiSnapshot(outputDir, capture);
			const sourceAfterCopy = captureUiDist(join(UI_DIR, 'dist'));
			if (sourceAfterCopy?.fingerprint !== capture.fingerprint) {
				if (attempt === maxAttempts) {
					throw new Error(
						'UI dist changed repeatedly while Playwright was snapshotting it',
					);
				}
				continue;
			}

			const buildId = playwrightBuildIdentity(
				worktreeRoot,
				capture.fingerprint,
			);
			const build = await cargoRunner(cargoBuildArgs(), {
				cwd: PARISH_DIR,
				env: {
					...process.env,
					PARISH_PLAYWRIGHT_BUILD_ID: buildId,
					PARISH_UI_DIST_DIGEST: capture.fingerprint,
					PARISH_UI_DIST_DIR: staticDir,
				},
			});
			const source = cargoExecutableFromMessages(build.stdout ?? '');
			if (build.status !== 0) {
				throw new Error(`cargo build failed with status ${build.status}`);
			}
			if (!source) {
				throw new Error(
					'cargo build did not report a parish-server executable',
				);
			}
			if (afterCargoBuild) {
				await afterCargoBuild({ attempt, buildId, source, staticDir });
			}
			lease.assertOwned();

			// Read the shared Cargo output once. An ordinary Cargo invocation does
			// not honor our outer lock, so identity validation—not its pathname—is
			// what proves this immutable snapshot came from our build.
			const binary = readFileSync(source);
			if (
				!binaryContainsExpectedBuildIdentity(binary, buildId) ||
				!binaryContainsExpectedCsp(binary, capture.expectedHashes)
			) {
				if (attempt === maxAttempts) {
					throw new Error(
						"Cargo's shared parish-server output never matched this worktree's build identity and CSP",
					);
				}
				continue;
			}

			const path = publishCachedBinary(
				outputDir,
				binary,
				statSync(source).mode,
			);
			lease.assertOwned();
			const activeUseLease = publishActiveUseLease(serverRoot, [
				path,
				staticDir,
			]);
			try {
				// The active-use lease is visible before the global build lock is
				// released, so every later preparer sees a complete handoff.
				pruneServerArtifacts(serverRoot);
				lease.assertOwned();
				activeUseLease.assertOwned();
				return {
					activeUseLease,
					attempts: attempt,
					buildId,
					expectedHashes: capture.expectedHashes,
					outputDir,
					path,
					staticDir,
				};
			} catch (error) {
				activeUseLease.release();
				throw error;
			}
		}
	} finally {
		lease.release();
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

function readyMarkerPayload(runId, buildId) {
	return `${runId}\n${buildId}\n`;
}

export function publishReadyMarker(readyFile, runId, buildId) {
	const payload = readyMarkerPayload(runId, buildId);
	try {
		if (readFileSync(readyFile, 'utf8') === payload) return;
		throw new Error(
			'Playwright readiness marker contains an unexpected identity',
		);
	} catch (error) {
		if (error?.code !== 'ENOENT') throw error;
	}
	const candidate = `${readyFile}.candidate-${process.pid}-${randomBytes(6).toString('hex')}`;
	try {
		writeFileSync(candidate, payload, { flag: 'wx', mode: 0o600 });
		renameSync(candidate, readyFile);
	} finally {
		rmSync(candidate, { force: true });
	}
}

async function fetchTextBeforeDeadline(url, deadline, fetchImpl) {
	const remaining = deadline - Date.now();
	if (remaining <= 0)
		throw new Error('Playwright server validation deadline elapsed');
	const controller = new AbortController();
	const timer = setTimeout(() => controller.abort(), remaining);
	try {
		const response = await fetchImpl(url, { signal: controller.signal });
		const body = await response.text();
		return { body, response };
	} finally {
		clearTimeout(timer);
	}
}

export async function waitForServedCsp({
	buildId,
	expectedHashes,
	fetchImpl = fetch,
	port,
	readyFile,
	runId,
	server,
	timeoutMs = 30_000,
}) {
	const deadline = Date.now() + timeoutMs;
	let lastError;
	while (Date.now() < deadline) {
		if (server.exitCode !== null) {
			throw new Error('parish-server exited before identity/CSP validation');
		}
		try {
			const identityUrl = `http://127.0.0.1:${port}/api/playwright-ready/${encodeURIComponent(runId)}`;
			const identity = await fetchTextBeforeDeadline(
				identityUrl,
				deadline,
				fetchImpl,
			);
			const servedBuildId = identity.response.headers.get(
				PLAYWRIGHT_BUILD_ID_HEADER,
			);
			if (
				(identity.response.status === 200 ||
					identity.response.status === 503) &&
				servedBuildId !== buildId
			) {
				throw new Error(
					'live server build identity does not match this worktree',
				);
			}
			if (identity.response.status === 200) return;
			if (identity.response.status === 503) {
				const root = await fetchTextBeforeDeadline(
					`http://127.0.0.1:${port}/`,
					deadline,
					fetchImpl,
				);
				if (!root.response.ok) {
					throw new Error(`server returned HTTP ${root.response.status}`);
				}
				const csp = root.response.headers.get('content-security-policy') ?? '';
				assertServedCspCoherent(root.body, csp, expectedHashes);
				publishReadyMarker(readyFile, runId, buildId);
				continue;
			}
			if (identity.response.status === 404) {
				lastError = new Error(
					'another Playwright run owns the listener on this port',
				);
			} else {
				lastError = new Error(
					`readiness endpoint returned HTTP ${identity.response.status}`,
				);
			}
		} catch (error) {
			lastError = error;
		}
		const remaining = deadline - Date.now();
		if (remaining > 0) await delay(Math.min(100, remaining));
	}
	throw new Error(
		`timed out validating served identity/CSP/HTML coherence: ${lastError?.message ?? 'server unavailable'}`,
	);
}

export function superviseServer(
	command,
	args,
	{
		cwd = PARISH_DIR,
		env = process.env,
		shutdownTimeoutMs = PLAYWRIGHT_SERVER_SHUTDOWN_TIMEOUT_MS,
		stdio = 'inherit',
	} = {},
) {
	const server = spawn(command, args, { cwd, env, stdio });
	let settleExit;
	const exited = new Promise((resolveExit) => {
		settleExit = resolveExit;
	});
	let settled = false;
	const settle = (result) => {
		if (settled) return;
		settled = true;
		settleExit(result);
	};
	server.once('error', (error) => {
		settle({ error });
	});
	server.once('exit', (code, signal) => {
		settle({ code, signal });
	});

	return {
		exited,
		server,
		async stop(signal = 'SIGTERM') {
			if (server.exitCode === null && server.signalCode === null) {
				try {
					server.kill(signal);
				} catch (error) {
					if (error?.code !== 'ESRCH') throw error;
				}
			}
			if (signal === 'SIGKILL') return exited;

			let escalationTimer;
			const forcedExit = new Promise((resolveForcedExit, rejectForcedExit) => {
				escalationTimer = setTimeout(async () => {
					try {
						if (server.exitCode === null && server.signalCode === null) {
							server.kill('SIGKILL');
						}
						resolveForcedExit(await exited);
					} catch (error) {
						if (error?.code === 'ESRCH') {
							resolveForcedExit(await exited);
						} else {
							rejectForcedExit(error);
						}
					}
				}, shutdownTimeoutMs);
				escalationTimer.unref();
			});
			try {
				return await Promise.race([exited, forcedExit]);
			} finally {
				clearTimeout(escalationTimer);
			}
		},
	};
}

/**
 * Keep the launcher alive through signal-driven child shutdown and fence the
 * child immediately if its artifact lease is lost.
 */
export async function runManagedServerLifecycle({
	activeUseLease,
	processRef = process,
	supervision,
	waitUntilReady = async () => {},
}) {
	let resolveTermination;
	const termination = new Promise((resolveSignal) => {
		resolveTermination = resolveSignal;
	});
	const handlers = new Map(
		['SIGTERM', 'SIGINT'].map((signal) => [
			signal,
			() => resolveTermination({ signal, type: 'termination' }),
		]),
	);
	for (const [signal, handler] of handlers) processRef.on(signal, handler);

	const childExit = supervision.exited.then((result) => ({
		result,
		type: 'child-exit',
	}));
	const leaseLoss = activeUseLease.lost.then((error) => ({
		error,
		type: 'lease-loss',
	}));
	const readiness = Promise.resolve()
		.then(waitUntilReady)
		.then(
			() => ({ type: 'ready' }),
			(error) => ({ error, type: 'startup-error' }),
		);

	try {
		let outcome = await Promise.race([
			readiness,
			childExit,
			leaseLoss,
			termination,
		]);
		if (outcome.type === 'ready') {
			outcome = await Promise.race([childExit, leaseLoss, termination]);
		}

		if (outcome.type === 'termination') {
			await supervision.stop(outcome.signal);
			processRef.exitCode = outcome.signal === 'SIGINT' ? 130 : 143;
			return;
		}
		if (outcome.type === 'lease-loss') {
			processRef.exitCode = 1;
			await supervision.stop('SIGTERM');
			throw new Error(
				`Playwright server artifact lease was lost: ${outcome.error.message}`,
				{ cause: outcome.error },
			);
		}
		if (outcome.type === 'startup-error') {
			processRef.exitCode = 1;
			await supervision.stop('SIGTERM');
			throw outcome.error;
		}
		if (outcome.result.error) {
			processRef.exitCode = 1;
			throw outcome.result.error;
		}
		processRef.exitCode =
			outcome.result.code ?? (outcome.result.signal ? 0 : 1);
	} finally {
		for (const [signal, handler] of handlers) {
			processRef.removeListener(signal, handler);
		}
		activeUseLease.release();
	}
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

export async function prepareManagedServer({
	buildUi = buildUiDist,
	prepare = prepareIsolatedServerBinary,
} = {}) {
	await buildUi();
	return prepare();
}

async function main() {
	const port = parsePort(process.argv.slice(2));
	const runId = process.env.PARISH_PLAYWRIGHT_RUN_ID;
	if (!runId || !RUN_ID_PATTERN.test(runId)) {
		throw new Error('PARISH_PLAYWRIGHT_RUN_ID is missing or invalid');
	}
	const prepared = await prepareManagedServer();
	const readyFile = join(prepared.outputDir, `.playwright-ready-${runId}`);
	rmSync(readyFile, { force: true });
	const env = {
		...process.env,
		PARISH_PLAYWRIGHT_BUILD_ID: prepared.buildId,
		PARISH_PLAYWRIGHT_READY_FILE: readyFile,
		PARISH_PLAYWRIGHT_RUN_ID: runId,
	};
	const supervision = superviseServer(
		prepared.path,
		['--port', port, '--static-dir', prepared.staticDir],
		{ env },
	);
	await runManagedServerLifecycle({
		activeUseLease: prepared.activeUseLease,
		supervision,
		waitUntilReady: () =>
			waitForServedCsp({
				buildId: prepared.buildId,
				expectedHashes: prepared.expectedHashes,
				port,
				readyFile,
				runId,
				server: supervision.server,
			}),
	});
}

if (
	process.argv[1] &&
	pathToFileURL(realpathSync(process.argv[1])).href === import.meta.url
) {
	await main();
}
