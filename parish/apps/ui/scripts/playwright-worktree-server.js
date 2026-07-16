import { execFileSync, spawn, spawnSync } from 'node:child_process';
import { createHash, randomBytes } from 'node:crypto';
import {
	chmodSync,
	copyFileSync,
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
	const pending = [distDir];

	while (pending.length > 0) {
		const directory = pending.pop();
		for (const entry of readdirSync(directory, { withFileTypes: true })) {
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
		'-p',
		'parish-server',
		'--bin',
		'parish-server',
		'--',
		'-C',
		`metadata=playwright_${buildNonce}`,
	];
}

function processIsAlive(pid) {
	try {
		process.kill(pid, 0);
		return true;
	} catch (error) {
		return error?.code !== 'ESRCH';
	}
}

function removeAbandonedLock(lockDir) {
	let owner;
	try {
		owner = JSON.parse(readFileSync(join(lockDir, 'owner.json'), 'utf8'));
	} catch {
		return false;
	}

	if (
		owner.hostname !== hostname() ||
		!Number.isInteger(owner.pid) ||
		processIsAlive(owner.pid)
	) {
		return false;
	}

	const abandoned = `${lockDir}.abandoned-${process.pid}-${randomBytes(4).toString('hex')}`;
	try {
		renameSync(lockDir, abandoned);
		rmSync(abandoned, { force: true, recursive: true });
		return true;
	} catch (error) {
		if (error?.code === 'ENOENT') return true;
		return false;
	}
}

/**
 * Acquire a real cross-process lock using atomic directory creation.
 *
 * Cargo releases its own target lock before this launcher can preserve the
 * final binary. Holding this outer lock through build, validation, and copy
 * closes that window between concurrent Playwright processes.
 */
export async function acquireDirectoryLock(
	lockDir,
	{ timeoutMs = 240_000, pollMs = 100 } = {},
) {
	mkdirSync(dirname(lockDir), { recursive: true });
	const deadline = Date.now() + timeoutMs;
	const token = randomBytes(16).toString('hex');

	while (Date.now() < deadline) {
		try {
			mkdirSync(lockDir);
			writeFileSync(
				join(lockDir, 'owner.json'),
				JSON.stringify({ hostname: hostname(), pid: process.pid, token }),
			);
			return () => {
				try {
					const owner = JSON.parse(
						readFileSync(join(lockDir, 'owner.json'), 'utf8'),
					);
					if (owner.token === token) {
						rmSync(lockDir, { force: true, recursive: true });
					}
				} catch (error) {
					if (error?.code !== 'ENOENT') throw error;
				}
			};
		} catch (error) {
			if (error?.code !== 'EEXIST') throw error;
			if (!removeAbandonedLock(lockDir)) await delay(pollMs);
		}
	}

	throw new Error(
		`timed out waiting for Playwright server build lock: ${lockDir}`,
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

function pruneOldCopies(directory) {
	const oldestAllowed = Date.now() - 24 * 60 * 60 * 1000;
	try {
		for (const entry of readdirSync(directory, { withFileTypes: true })) {
			if (!entry.isFile() || !entry.name.startsWith('parish-server-')) continue;
			const path = join(directory, entry.name);
			if (statSync(path).mtimeMs < oldestAllowed) rmSync(path, { force: true });
		}
	} catch (error) {
		if (error?.code !== 'ENOENT') throw error;
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

	const outputDir = join(
		targetDir,
		'playwright-servers',
		worktreeKey(worktreeRoot),
	);
	mkdirSync(outputDir, { recursive: true });
	pruneOldCopies(outputDir);
	const releaseLock = await acquireDirectoryLock(
		join(targetDir, '.playwright-parish-server-build.lock'),
	);

	try {
		for (let attempt = 1; attempt <= 3; attempt += 1) {
			const nonce = `${process.pid}_${Date.now()}_${randomBytes(6).toString('hex')}`;
			const build = spawnSync('cargo', cargoRustcArgs(nonce), {
				cwd: PARISH_DIR,
				env: {
					...process.env,
					PARISH_UI_DIST_DIGEST: uiDistFingerprint(expectedHashes),
				},
				stdio: 'inherit',
			});
			if (build.error) throw build.error;
			if (build.status !== 0) {
				throw new Error(`cargo rustc failed with status ${build.status}`);
			}

			const source = join(
				targetDir,
				'debug',
				`parish-server${EXECUTABLE_SUFFIX}`,
			);
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
				return destination;
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

	if (process.env.GITHUB_ACTIONS === 'true') {
		// GitHub-hosted jobs have an isolated filesystem and already prebuild this
		// exact binary, so keep the established Cargo cache path there.
		command = 'cargo';
		args = ['run', '-p', 'parish-server', '--', '--port', port];
	} else {
		preservedBinary = await prepareIsolatedServerBinary();
		command = preservedBinary;
		args = ['--port', port];
	}

	const server = spawn(command, args, {
		cwd: PARISH_DIR,
		env: process.env,
		stdio: 'inherit',
	});
	for (const signal of ['SIGINT', 'SIGTERM']) {
		process.once(signal, () => server.kill(signal));
	}
	server.once('error', (error) => {
		if (preservedBinary) rmSync(preservedBinary, { force: true });
		console.error(error);
		process.exit(1);
	});
	server.once('exit', (code) => {
		if (preservedBinary) rmSync(preservedBinary, { force: true });
		process.exit(code ?? 1);
	});
}

if (
	process.argv[1] &&
	pathToFileURL(realpathSync(process.argv[1])).href === import.meta.url
) {
	await main();
}
