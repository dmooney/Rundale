#!/usr/bin/env node
/**
 * Generate a deterministic UI third-party notice without touching the
 * source checkout's node_modules or exposing the destination to partial data.
 */
import { spawnSync } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const SCANNER_PACKAGE = 'license-checker-rseidelsohn';
const SCANNER_VERSION = '4.4.2';
const REQUIRED_MANIFESTS = [
	'package.json',
	'package-lock.json',
	'license-clarifications.json',
];

// Keep this matrix aligned with the support sensors in the regression test.
// Desktop targets come from .github/workflows/audit.yml; Linux arm64 is an
// active container target in deploy/Dockerfile. Lockfile-only future targets
// (Android, iOS, FreeBSD, OpenHarmony, Windows arm64) are intentionally absent.
export const SUPPORTED_TARGETS = Object.freeze([
	Object.freeze({ id: 'darwin-arm64', os: 'darwin', cpu: 'arm64' }),
	Object.freeze({ id: 'darwin-x64', os: 'darwin', cpu: 'x64' }),
	Object.freeze({ id: 'linux-arm64', os: 'linux', cpu: 'arm64' }),
	Object.freeze({ id: 'linux-x64', os: 'linux', cpu: 'x64' }),
	Object.freeze({ id: 'win32-x64', os: 'win32', cpu: 'x64' }),
]);

export function defaultNpmCommand(platform = process.platform) {
	return [platform === 'win32' ? 'npm.cmd' : 'npm'];
}

function parseCommandOverride(value, label) {
	if (!value) return undefined;
	let parsed;
	try {
		parsed = JSON.parse(value);
	} catch (error) {
		throw new Error(`${label} must be a JSON string array`, { cause: error });
	}
	if (
		!Array.isArray(parsed) ||
		parsed.length === 0 ||
		parsed.some((part) => typeof part !== 'string' || part.length === 0)
	) {
		throw new Error(`${label} must be a nonempty JSON string array`);
	}
	return parsed;
}

function runCommand(commandSpec, args, options) {
	const [command, ...prefixArgs] = commandSpec;
	const result = options.spawnSyncImpl(command, [...prefixArgs, ...args], {
		cwd: options.cwd,
		encoding: 'utf8',
		env: options.env,
		shell: false,
		stdio: options.captureStdout ? ['ignore', 'pipe', 'inherit'] : 'inherit',
		windowsHide: true,
	});

	if (result.error) {
		throw new Error(`failed to start ${command}: ${result.error.message}`, {
			cause: result.error,
		});
	}
	if (result.status !== 0) {
		const detail = result.signal
			? `signal ${result.signal}`
			: `exit status ${result.status}`;
		throw new Error(`${command} failed with ${detail}`);
	}
	return result.stdout ?? '';
}

function parseNotice(content, targetId) {
	if (content.trim().length === 0) {
		throw new Error(`${targetId}: generated UI notice is blank`);
	}

	const rows = new Map();
	for (const rawLine of content.split(/\r?\n/)) {
		const line = rawLine.trim();
		if (line.length === 0) continue;
		const match = line.match(/^- \[([^\]]+)\]\(([^)]+)\) - (\S.*)$/);
		if (!match) {
			throw new Error(`${targetId}: malformed UI notice row: ${rawLine}`);
		}
		const key = match[1];
		const normalized = `- [${key}](${match[2].trim()}) - ${match[3].trim()}`;
		if (rows.has(key)) {
			throw new Error(`${targetId}: duplicate UI notice row: ${key}`);
		}
		rows.set(key, normalized);
	}
	return rows;
}

function collectInstalledDependencies(tree, targetId) {
	const installed = new Set();
	function collect(dependencies) {
		for (const [name, dependency] of Object.entries(dependencies ?? {})) {
			if (!dependency || typeof dependency !== 'object') {
				throw new Error(
					`${targetId}: installed dependency lacks data: ${name}`,
				);
			}
			// A completed target-private npm ls represents packages omitted for that
			// target as empty objects. They are omitted here, but the supported-target
			// union below requires their installed variants from the other targets.
			if (Object.keys(dependency).length === 0) continue;
			if (!dependency.version) {
				throw new Error(
					`${targetId}: installed dependency lacks a version: ${name}`,
				);
			}
			installed.add(`${name}@${dependency.version}`);
			collect(dependency.dependencies);
		}
	}
	collect(tree.dependencies);
	if (installed.size === 0) {
		throw new Error(
			`${targetId}: installed production dependency tree is empty`,
		);
	}
	return installed;
}

function validateAndMergeTargets(targetArtifacts, fsOps) {
	const unionInstalled = new Set();
	const unionRows = new Map();

	for (const artifact of targetArtifacts) {
		let tree;
		try {
			tree = JSON.parse(fsOps.readFileSync(artifact.treePath, 'utf8'));
		} catch (error) {
			throw new Error(`${artifact.id}: invalid npm dependency tree JSON`, {
				cause: error,
			});
		}
		const installed = collectInstalledDependencies(tree, artifact.id);
		const rows = parseNotice(
			fsOps.readFileSync(artifact.noticePath, 'utf8'),
			artifact.id,
		);

		const missing = [...installed].filter(
			(dependency) => !rows.has(dependency),
		);
		const unexpected = [...rows.keys()].filter(
			(dependency) => !installed.has(dependency),
		);
		if (missing.length > 0) {
			throw new Error(
				`${artifact.id}: notice is missing installed dependencies: ${missing.join(', ')}`,
			);
		}
		if (unexpected.length > 0) {
			throw new Error(
				`${artifact.id}: notice contains uninstalled dependencies: ${unexpected.join(', ')}`,
			);
		}

		for (const dependency of installed) unionInstalled.add(dependency);
		for (const [dependency, row] of rows) {
			const existing = unionRows.get(dependency);
			if (existing !== undefined && existing !== row) {
				throw new Error(
					`${dependency}: conflicting normalized license rows across targets: ` +
						`${existing} != ${row}`,
				);
			}
			unionRows.set(dependency, row);
		}
	}

	const missingFromUnion = [...unionInstalled].filter(
		(dependency) => !unionRows.has(dependency),
	);
	const unexpectedInUnion = [...unionRows.keys()].filter(
		(dependency) => !unionInstalled.has(dependency),
	);
	if (missingFromUnion.length > 0 || unexpectedInUnion.length > 0) {
		throw new Error(
			`supported-target union mismatch: missing=[${missingFromUnion.join(', ')}], ` +
				`unexpected=[${unexpectedInUnion.join(', ')}]`,
		);
	}

	const sortedDependencies = [...unionInstalled].sort((left, right) =>
		left < right ? -1 : left > right ? 1 : 0,
	);
	return `${sortedDependencies.map((dependency) => unionRows.get(dependency)).join('\n')}\n`;
}

function verifyLockedScanner(manifests) {
	const packageJson = JSON.parse(
		manifests.get('package.json').toString('utf8'),
	);
	const packageLock = JSON.parse(
		manifests.get('package-lock.json').toString('utf8'),
	);
	const declared = packageJson.devDependencies?.[SCANNER_PACKAGE];
	const lockedDeclaration =
		packageLock.packages?.['']?.devDependencies?.[SCANNER_PACKAGE];
	const lockedPackage =
		packageLock.packages?.[`node_modules/${SCANNER_PACKAGE}`];
	if (declared !== SCANNER_VERSION || lockedDeclaration !== SCANNER_VERSION) {
		throw new Error(
			`${SCANNER_PACKAGE} must be declared exactly as ${SCANNER_VERSION} in package.json and package-lock.json`,
		);
	}
	if (
		lockedPackage?.version !== SCANNER_VERSION ||
		typeof lockedPackage.integrity !== 'string'
	) {
		throw new Error(
			`${SCANNER_PACKAGE}@${SCANNER_VERSION} must have a lockfile integrity record`,
		);
	}
}

function createCommitCandidate(fsOps, destinationDirectory) {
	for (let attempt = 0; attempt < 10; attempt += 1) {
		const candidate = path.join(
			destinationDirectory,
			`.ui-notices.commit-${process.pid}-${randomUUID()}`,
		);
		try {
			const descriptor = fsOps.openSync(candidate, 'wx', 0o600);
			fsOps.closeSync(descriptor);
			return candidate;
		} catch (error) {
			if (error.code !== 'EEXIST') throw error;
		}
	}
	throw new Error('could not allocate a unique UI notice commit candidate');
}

function cleanupBestEffort(fsOps, target) {
	if (!target) return;
	try {
		fsOps.rmSync(target, { force: true, recursive: true });
	} catch {
		// Preserve the original failure. Cleanup is deliberately best-effort only
		// before commit; the success path disarms it before the atomic rename.
	}
}

export function generateUiNotices(options = {}) {
	const env = options.env ?? process.env;
	const fsOps = options.fsOps ?? fs;
	const spawnSyncImpl = options.spawnSyncImpl ?? spawnSync;
	const logger = options.logger ?? console;
	const scriptDirectory = path.dirname(fileURLToPath(import.meta.url));
	const parishDirectory = path.resolve(scriptDirectory, '..');
	const uiDirectory = path.resolve(
		env.PARISH_UI_NOTICES_UI_DIR ?? path.join(parishDirectory, 'apps', 'ui'),
	);
	const destination = path.resolve(
		env.PARISH_UI_NOTICES_DESTINATION ??
			path.join(parishDirectory, 'THIRD_PARTY_NOTICES.ui.md'),
	);
	const destinationDirectory = path.dirname(destination);
	const targets = options.targets ?? SUPPORTED_TARGETS;
	const npmCommand =
		options.npmCommand ??
		parseCommandOverride(
			env.PARISH_UI_NOTICES_NPM_COMMAND_JSON,
			'PARISH_UI_NOTICES_NPM_COMMAND_JSON',
		) ??
		defaultNpmCommand(options.platform ?? process.platform);
	const scannerOverride =
		options.scannerCommand ??
		parseCommandOverride(
			env.PARISH_UI_NOTICES_SCANNER_COMMAND_JSON,
			'PARISH_UI_NOTICES_SCANNER_COMMAND_JSON',
		);

	if (!fsOps.statSync(destinationDirectory).isDirectory()) {
		throw new Error(
			`destination directory does not exist: ${destinationDirectory}`,
		);
	}
	const manifests = new Map();
	for (const manifest of REQUIRED_MANIFESTS) {
		const source = path.join(uiDirectory, manifest);
		try {
			manifests.set(manifest, fsOps.readFileSync(source));
		} catch (error) {
			throw new Error(`missing prerequisite ${source}`, { cause: error });
		}
	}
	verifyLockedScanner(manifests);

	const targetIds = new Set();
	for (const target of targets) {
		if (!target.id || !target.os || !target.cpu || targetIds.has(target.id)) {
			throw new Error(
				`invalid or duplicate supported target: ${JSON.stringify(target)}`,
			);
		}
		targetIds.add(target.id);
	}

	let workDirectory;
	let commitCandidate;
	try {
		workDirectory = fsOps.mkdtempSync(
			path.join(destinationDirectory, '.ui-notices.work-'),
		);
		commitCandidate = createCommitCandidate(fsOps, destinationDirectory);
		const cacheDirectory = path.join(workDirectory, 'npm-cache');
		const targetArtifacts = [];

		for (const target of targets) {
			const targetDirectory = path.join(workDirectory, target.id);
			fsOps.mkdirSync(targetDirectory, { recursive: true });
			for (const [manifest, content] of manifests) {
				fsOps.writeFileSync(path.join(targetDirectory, manifest), content);
			}
			const targetEnvironment = {
				...env,
				npm_config_cache: cacheDirectory,
				npm_config_cpu: target.cpu,
				npm_config_os: target.os,
			};

			logger.log(`Materializing locked UI dependencies for ${target.id}...`);
			// Notice generation only reads dependency/package metadata. Lifecycle
			// scripts produce build artifacts and may mutate the checkout; they are
			// unnecessary here and intentionally disabled in every private install.
			runCommand(
				npmCommand,
				[
					'ci',
					'--ignore-scripts',
					'--include=dev',
					'--include=optional',
					'--include=peer',
					'--no-audit',
					'--no-fund',
				],
				{
					captureStdout: false,
					cwd: targetDirectory,
					env: targetEnvironment,
					spawnSyncImpl,
				},
			);

			const dependencyTree = path.join(targetDirectory, 'production-tree.json');
			const treeJson = runCommand(
				npmCommand,
				[
					'ls',
					'--omit=dev',
					'--include=optional',
					'--include=peer',
					'--all',
					'--json',
				],
				{
					captureStdout: true,
					cwd: targetDirectory,
					env: targetEnvironment,
					spawnSyncImpl,
				},
			);
			fsOps.writeFileSync(dependencyTree, treeJson, 'utf8');

			const targetNotice = path.join(targetDirectory, 'notice.md');
			const scannerCommand = scannerOverride ?? [
				process.execPath,
				path.join(
					targetDirectory,
					'node_modules',
					SCANNER_PACKAGE,
					'bin',
					'license-checker-rseidelsohn.js',
				),
			];
			runCommand(
				scannerCommand,
				[
					'--production',
					'--excludePrivatePackages',
					'--clarificationsFile',
					'./license-clarifications.json',
					'--markdown',
					'--out',
					targetNotice,
				],
				{
					captureStdout: false,
					cwd: targetDirectory,
					env: targetEnvironment,
					spawnSyncImpl,
				},
			);
			targetArtifacts.push({
				id: target.id,
				noticePath: targetNotice,
				treePath: dependencyTree,
			});
		}

		const unionNotice = validateAndMergeTargets(targetArtifacts, fsOps);
		fsOps.writeFileSync(commitCandidate, unionNotice, 'utf8');

		// Refuse to commit a snapshot if its source manifests changed while the
		// private target installs were running.
		for (const [manifest, original] of manifests) {
			const current = fsOps.readFileSync(path.join(uiDirectory, manifest));
			if (!current.equals(original)) {
				throw new Error(
					`source prerequisite changed during generation: ${manifest}`,
				);
			}
		}

		fsOps.chmodSync(commitCandidate, 0o644);
		// All potentially failing cleanup finishes before commit. The failure path
		// below is best-effort and cannot mask the original error.
		fsOps.rmSync(workDirectory, { force: true, recursive: true });
		workDirectory = undefined;
		logger.log(
			`Validated ${targets.length}-target UI notice union; replacing ${destination} atomically.`,
		);
		// Same-directory rename is the literal final fallible operation.
		fsOps.renameSync(commitCandidate, destination);
	} catch (error) {
		cleanupBestEffort(fsOps, workDirectory);
		cleanupBestEffort(fsOps, commitCandidate);
		throw error;
	}
}

const invokedAsScript =
	process.argv[1] &&
	pathToFileURL(path.resolve(process.argv[1])).href === import.meta.url;
if (invokedAsScript) {
	try {
		generateUiNotices();
	} catch (error) {
		console.error(`generate-ui-notices: ${error.message}`);
		process.exitCode = 1;
	}
}
