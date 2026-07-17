#!/usr/bin/env node
import { spawn, spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import assert from 'node:assert/strict';

import {
	SUPPORTED_TARGETS,
	defaultNpmCommand,
	generateUiNotices,
} from '../generate-ui-notices.mjs';

const testDirectory = path.dirname(fileURLToPath(import.meta.url));
const scriptsDirectory = path.resolve(testDirectory, '..');
const parishDirectory = path.resolve(scriptsDirectory, '..');
const repositoryRoot = path.resolve(parishDirectory, '..');
const generatorPath = path.join(scriptsDirectory, 'generate-ui-notices.mjs');
const scannerPackage = 'license-checker-rseidelsohn';
const scannerVersion = '4.4.2';
const firstTarget = [SUPPORTED_TARGETS[0]];

const tests = [];
function test(name, body) {
	tests.push({ body, name });
}

function sha256(file) {
	return createHash('sha256').update(fs.readFileSync(file)).digest('hex');
}

function noticeRow(dependency, license = 'MIT') {
	const encoded = encodeURIComponent(dependency.replace('@', '-'));
	return `- [${dependency}](https://example.test/${encoded}) - ${license}`;
}

function makeTree(nativeDependencies = []) {
	const dependencies = {
		alpha: {
			dependencies: {
				'omitted-for-target': {},
				shared: { version: '1.1.0' },
			},
			version: '1.0.0',
		},
		beta: { version: '2.0.0' },
	};
	for (const dependency of nativeDependencies) {
		const separator = dependency.lastIndexOf('@');
		dependencies[dependency.slice(0, separator)] = {
			version: dependency.slice(separator + 1),
		};
	}
	return { dependencies };
}

const targetNatives = {
	'darwin-arm64': ['native-darwin-arm64@1.0.0'],
	'darwin-x64': ['native-darwin-x64@1.0.0'],
	'linux-arm64': [
		'native-linux-arm64-gnu@1.0.0',
		'native-linux-arm64-musl@1.0.0',
	],
	'linux-x64': ['native-linux-x64-gnu@1.0.0', 'native-linux-x64-musl@1.0.0'],
	'win32-x64': ['native-win32-x64@1.0.0'],
};

function completeTargetConfig() {
	const common = ['alpha@1.0.0', 'beta@2.0.0', 'shared@1.1.0'];
	return Object.fromEntries(
		SUPPORTED_TARGETS.map((target, index) => {
			const dependencies = [...common, ...targetNatives[target.id]];
			if (index % 2 === 1) dependencies.reverse();
			return [
				target.id,
				{
					notice: `${dependencies.map((dependency) => noticeRow(dependency)).join('\n')}\n`,
					tree: makeTree(targetNatives[target.id]),
				},
			];
		}),
	);
}

function expectedUnion() {
	const dependencies = new Set(['alpha@1.0.0', 'beta@2.0.0', 'shared@1.1.0']);
	for (const natives of Object.values(targetNatives)) {
		for (const dependency of natives) dependencies.add(dependency);
	}
	return `${[...dependencies]
		.sort((left, right) => (left < right ? -1 : left > right ? 1 : 0))
		.map((dependency) => noticeRow(dependency))
		.join('\n')}\n`;
}

const commandStubSource = `#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";

const [role, ...args] = process.argv.slice(2);
const config = JSON.parse(fs.readFileSync(process.env.TEST_NOTICE_CONFIG, "utf8"));
const target = process.env.npm_config_os + "-" + process.env.npm_config_cpu;
const record = JSON.stringify({ args, cwd: process.cwd(), pid: process.pid, role, target }) + "\\n";
fs.appendFileSync(config.calls, record);
if (config.delayMs) {
  Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, config.delayMs);
}
const fails = (point) =>
  config.scenario?.failAt === point &&
  (!config.scenario.target || config.scenario.target === target);

if (role === "npm") {
  if (args[0] === "ci") {
    fs.rmSync(path.join(process.cwd(), "node_modules"), { force: true, recursive: true });
    fs.mkdirSync(path.join(process.cwd(), "node_modules"), { recursive: true });
    if (fails("npm-ci")) process.exit(41);
    process.exit(0);
  }
  if (args[0] === "ls") {
    if (fails("npm-ls")) process.exit(42);
    process.stdout.write(JSON.stringify(config.targets[target].tree));
    process.exit(0);
  }
  throw new Error("unexpected npm command: " + args.join(" "));
}

if (role === "scanner") {
  const outputIndex = args.indexOf("--out");
  if (outputIndex < 0 || !args[outputIndex + 1]) {
    throw new Error("scanner stub did not receive --out");
  }
  fs.writeFileSync(args[outputIndex + 1], config.targets[target].notice);
  if (fails("scanner")) process.exit(43);
  process.exit(0);
}
throw new Error("unexpected stub role: " + role);
`;

function createFixture() {
	const root = fs.mkdtempSync(
		path.join(os.tmpdir(), 'UI Notices Windows Path '),
	);
	const uiDirectory = path.join(root, 'source UI');
	const destination = path.join(root, 'THIRD PARTY NOTICES.ui.md');
	const baseline = path.join(root, 'baseline.bin');
	const calls = path.join(root, 'calls.jsonl');
	const configPath = path.join(root, 'stub config.json');
	const stubPath = path.join(root, 'command stub.mjs');
	fs.mkdirSync(uiDirectory, { recursive: true });
	const packageJson = {
		dependencies: { alpha: '1.0.0', beta: '2.0.0' },
		devDependencies: { [scannerPackage]: scannerVersion },
		name: 'notice-fixture',
		private: true,
	};
	const packageLock = {
		lockfileVersion: 3,
		name: 'notice-fixture',
		packages: {
			'': {
				dependencies: packageJson.dependencies,
				devDependencies: packageJson.devDependencies,
				name: 'notice-fixture',
			},
			[`node_modules/${scannerPackage}`]: {
				dev: true,
				integrity: 'sha512-fixture-integrity',
				version: scannerVersion,
			},
		},
		requires: true,
		version: '0.0.0',
	};
	fs.writeFileSync(
		path.join(uiDirectory, 'package.json'),
		`${JSON.stringify(packageJson, null, 2)}\n`,
	);
	fs.writeFileSync(
		path.join(uiDirectory, 'package-lock.json'),
		`${JSON.stringify(packageLock, null, 2)}\n`,
	);
	fs.writeFileSync(
		path.join(uiDirectory, 'license-clarifications.json'),
		'{}\n',
	);
	fs.mkdirSync(path.join(uiDirectory, 'node_modules'), { recursive: true });
	fs.writeFileSync(
		path.join(uiDirectory, 'node_modules', 'sentinel'),
		'source install must survive\n',
	);
	fs.writeFileSync(
		destination,
		'existing attribution\nwith trailing newline\n\n',
	);
	fs.copyFileSync(destination, baseline);
	fs.writeFileSync(calls, '');
	fs.writeFileSync(stubPath, commandStubSource);

	const config = {
		calls,
		delayMs: 0,
		scenario: {},
		targets: completeTargetConfig(),
	};
	const writeConfig = () =>
		fs.writeFileSync(configPath, `${JSON.stringify(config, null, 2)}\n`);
	writeConfig();
	const env = {
		...process.env,
		PARISH_UI_NOTICES_DESTINATION: destination,
		PARISH_UI_NOTICES_NPM_COMMAND_JSON: JSON.stringify([
			process.execPath,
			stubPath,
			'npm',
		]),
		PARISH_UI_NOTICES_SCANNER_COMMAND_JSON: JSON.stringify([
			process.execPath,
			stubPath,
			'scanner',
		]),
		PARISH_UI_NOTICES_UI_DIR: uiDirectory,
		TEST_NOTICE_CONFIG: configPath,
	};
	return {
		baseline,
		calls,
		config,
		destination,
		env,
		root,
		uiDirectory,
		writeConfig,
	};
}

function assertPreserved(fixture) {
	const baselineBytes = fs.readFileSync(fixture.baseline);
	const destinationBytes = fs.readFileSync(fixture.destination);
	assert.deepEqual(destinationBytes, baselineBytes);
	assert.equal(sha256(fixture.destination), sha256(fixture.baseline));
}

function assertSourceInstallPreserved(fixture) {
	assert.equal(
		fs.readFileSync(
			path.join(fixture.uiDirectory, 'node_modules', 'sentinel'),
			'utf8',
		),
		'source install must survive\n',
	);
}

function assertNoTransactionResidue(fixture) {
	const residue = fs
		.readdirSync(fixture.root)
		.filter((entry) => entry.startsWith('.ui-notices.'));
	assert.deepEqual(residue, []);
}

function readCalls(fixture) {
	return fs
		.readFileSync(fixture.calls, 'utf8')
		.trim()
		.split('\n')
		.filter(Boolean)
		.map((line) => JSON.parse(line));
}

function runFixture(fixture, options = {}) {
	const spawnOptions = options.spawnOptions ?? [];
	const spawnSyncImpl = (command, args, childOptions) => {
		spawnOptions.push({ args, command, options: childOptions });
		return spawnSync(command, args, childOptions);
	};
	generateUiNotices({
		env: fixture.env,
		fsOps: options.fsOps,
		logger: options.logger ?? { log() {} },
		spawnSyncImpl,
		targets: options.targets ?? firstTarget,
	});
	return spawnOptions;
}

function expectFailure(fixture, options = {}) {
	assert.throws(() => runFixture(fixture, options));
	assertPreserved(fixture);
	assertSourceInstallPreserved(fixture);
	assertNoTransactionResidue(fixture);
}

async function withFixture(body) {
	const fixture = createFixture();
	try {
		return await body(fixture);
	} finally {
		fs.rmSync(fixture.root, { force: true, recursive: true });
	}
}

function runCli(env) {
	return new Promise((resolve) => {
		const child = spawn(process.execPath, [generatorPath], {
			env,
			shell: false,
			stdio: ['ignore', 'pipe', 'pipe'],
			windowsHide: true,
		});
		let stdout = '';
		let stderr = '';
		child.stdout.on('data', (chunk) => {
			stdout += chunk;
		});
		child.stderr.on('data', (chunk) => {
			stderr += chunk;
		});
		child.on('close', (code, signal) =>
			resolve({ code, signal, stderr, stdout }),
		);
	});
}

test('supported target matrix stays tied to release and deploy facts', () => {
	const audit = fs.readFileSync(
		path.join(repositoryRoot, '.github', 'workflows', 'audit.yml'),
		'utf8',
	);
	const dockerfile = fs.readFileSync(
		path.join(repositoryRoot, 'deploy', 'Dockerfile'),
		'utf8',
	);
	const rustTargets = [
		...audit.matchAll(/^\s+"((?:aarch64|x86_64)-[^"]+)"$/gm),
	].map((match) => match[1]);
	assert.deepEqual(rustTargets.sort(), [
		'aarch64-apple-darwin',
		'x86_64-apple-darwin',
		'x86_64-pc-windows-msvc',
		'x86_64-unknown-linux-gnu',
	]);
	const dockerArchitectures = [
		...dockerfile.matchAll(/^\s+(amd64|arm64)\) SHA256=/gm),
	].map((match) => match[1]);
	assert.deepEqual(dockerArchitectures.sort(), ['amd64', 'arm64']);
	assert.deepEqual(SUPPORTED_TARGETS.map((target) => target.id).sort(), [
		'darwin-arm64',
		'darwin-x64',
		'linux-arm64',
		'linux-x64',
		'win32-x64',
	]);
});

test('native Windows entry point uses npm.cmd and shell-free Node', () => {
	assert.deepEqual(defaultNpmCommand('win32'), ['npm.cmd']);
	assert.deepEqual(defaultNpmCommand('darwin'), ['npm']);
	const packageJson = JSON.parse(
		fs.readFileSync(
			path.join(parishDirectory, 'apps', 'ui', 'package.json'),
			'utf8',
		),
	);
	assert.equal(
		packageJson.scripts.notices,
		'node ../../scripts/generate-ui-notices.mjs',
	);
	assert.equal(packageJson.devDependencies[scannerPackage], scannerVersion);
});

test('scanner and every transitive are lockfile-backed', () => {
	const lock = JSON.parse(
		fs.readFileSync(
			path.join(parishDirectory, 'apps', 'ui', 'package-lock.json'),
			'utf8',
		),
	);
	assert.equal(
		lock.packages[''].devDependencies[scannerPackage],
		scannerVersion,
	);
	assert.equal(
		lock.packages[`node_modules/${scannerPackage}`].version,
		scannerVersion,
	);
	assert.match(
		lock.packages[`node_modules/${scannerPackage}`].integrity,
		/^sha512-/,
	);
	for (const [packagePath, metadata] of Object.entries(lock.packages)) {
		if (packagePath && metadata.resolved) {
			assert.equal(
				typeof metadata.integrity,
				'string',
				`${packagePath} lacks integrity`,
			);
		}
	}
});

test('missing manifest fails before changing the destination', () =>
	withFixture((fixture) => {
		fs.rmSync(path.join(fixture.uiDirectory, 'package-lock.json'));
		expectFailure(fixture);
	}));

for (const failure of ['npm-ci', 'npm-ls', 'scanner']) {
	test(`${failure} failure preserves every destination byte`, () =>
		withFixture((fixture) => {
			fixture.config.scenario = { failAt: failure };
			fixture.writeConfig();
			expectFailure(fixture);
		}));
}

for (const [name, mutate] of [
	['blank', (target) => (target.notice = ' \r\n')],
	['malformed', (target) => (target.notice = 'not a notice row\n')],
	[
		'duplicate',
		(target) =>
			(target.notice = `${noticeRow('alpha@1.0.0')}\n${noticeRow('alpha@1.0.0')}\n`),
	],
	[
		'missing',
		(target) =>
			(target.notice = `${noticeRow('alpha@1.0.0')}\n${noticeRow('beta@2.0.0')}\n`),
	],
	[
		'unexpected',
		(target) => (target.notice += `${noticeRow('not-installed@9.0.0')}\n`),
	],
]) {
	test(`${name} scanner output is rejected byte-for-byte`, () =>
		withFixture((fixture) => {
			mutate(fixture.config.targets[SUPPORTED_TARGETS[0].id]);
			fixture.writeConfig();
			expectFailure(fixture);
		}));
}

test('conflicting URL or license text for one version is rejected', () =>
	withFixture((fixture) => {
		const second = fixture.config.targets[SUPPORTED_TARGETS[1].id];
		second.notice = second.notice.replace(
			noticeRow('alpha@1.0.0'),
			noticeRow('alpha@1.0.0', 'ISC'),
		);
		fixture.writeConfig();
		expectFailure(fixture, { targets: SUPPORTED_TARGETS.slice(0, 2) });
	}));

for (const failure of ['chmod', 'cleanup', 'rename']) {
	test(`${failure} failure occurs before or without changing the destination`, () =>
		withFixture((fixture) => {
			let injected = false;
			const fsOps = Object.create(fs);
			if (failure === 'chmod') {
				fsOps.chmodSync = () => {
					throw new Error('injected chmod failure');
				};
			} else if (failure === 'cleanup') {
				fsOps.rmSync = (target, options) => {
					if (
						!injected &&
						path.basename(target).startsWith('.ui-notices.work-')
					) {
						injected = true;
						throw new Error('injected cleanup failure');
					}
					return fs.rmSync(target, options);
				};
			} else {
				fsOps.renameSync = () => {
					throw new Error('injected rename failure');
				};
			}
			expectFailure(fixture, { fsOps });
		}));
}

test('matrix success uses private installs, sorted union, and final atomic rename', () =>
	withFixture((fixture) => {
		const packageHash = sha256(path.join(fixture.uiDirectory, 'package.json'));
		const lockHash = sha256(
			path.join(fixture.uiDirectory, 'package-lock.json'),
		);
		const events = [];
		const fsOps = Object.create(fs);
		fsOps.chmodSync = (target, mode) => {
			events.push({ operation: 'chmod', target });
			return fs.chmodSync(target, mode);
		};
		fsOps.rmSync = (target, options) => {
			events.push({ operation: 'cleanup', target });
			return fs.rmSync(target, options);
		};
		fsOps.renameSync = (source, target) => {
			events.push({ operation: 'rename', source, target });
			assert.equal(path.dirname(source), path.dirname(target));
			return fs.renameSync(source, target);
		};
		const spawnOptions = [];
		runFixture(fixture, {
			fsOps,
			spawnOptions,
			targets: SUPPORTED_TARGETS,
		});
		assert.equal(fs.readFileSync(fixture.destination, 'utf8'), expectedUnion());
		assertSourceInstallPreserved(fixture);
		assert.equal(
			sha256(path.join(fixture.uiDirectory, 'package.json')),
			packageHash,
		);
		assert.equal(
			sha256(path.join(fixture.uiDirectory, 'package-lock.json')),
			lockHash,
		);
		assert.ok(spawnOptions.every((call) => call.options.shell === false));
		const ciCalls = readCalls(fixture).filter(
			(call) => call.role === 'npm' && call.args[0] === 'ci',
		);
		assert.equal(ciCalls.length, SUPPORTED_TARGETS.length);
		assert.equal(
			new Set(ciCalls.map((call) => call.cwd)).size,
			SUPPORTED_TARGETS.length,
		);
		assert.ok(
			ciCalls.every((call) => !call.cwd.startsWith(fixture.uiDirectory)),
		);
		assert.ok(ciCalls.every((call) => call.args.includes('--ignore-scripts')));
		assert.equal(events.at(-1).operation, 'rename');
		assert.ok(
			events.findIndex((event) => event.operation === 'cleanup') <
				events.length - 1,
		);
		assertNoTransactionResidue(fixture);
	}));

test('two sequential matrix generations are byte-identical', () =>
	withFixture((fixture) => {
		runFixture(fixture, { targets: SUPPORTED_TARGETS });
		const firstBytes = fs.readFileSync(fixture.destination);
		const firstHash = sha256(fixture.destination);
		runFixture(fixture, { targets: SUPPORTED_TARGETS });
		assert.deepEqual(fs.readFileSync(fixture.destination), firstBytes);
		assert.equal(sha256(fixture.destination), firstHash);
		assertSourceInstallPreserved(fixture);
	}));

test('two real concurrent CLI processes cannot share or corrupt installs', async () => {
	await withFixture(async (fixture) => {
		fixture.config.delayMs = 15;
		fixture.writeConfig();
		const [first, second] = await Promise.all([
			runCli(fixture.env),
			runCli(fixture.env),
		]);
		assert.equal(first.code, 0, first.stderr);
		assert.equal(second.code, 0, second.stderr);
		assert.equal(first.signal, null);
		assert.equal(second.signal, null);
		assert.equal(fs.readFileSync(fixture.destination, 'utf8'), expectedUnion());
		assertSourceInstallPreserved(fixture);
		const ciCalls = readCalls(fixture).filter(
			(call) => call.role === 'npm' && call.args[0] === 'ci',
		);
		assert.equal(ciCalls.length, SUPPORTED_TARGETS.length * 2);
		assert.equal(new Set(ciCalls.map((call) => call.cwd)).size, ciCalls.length);
		assert.ok(
			ciCalls.every((call) => !call.cwd.startsWith(fixture.uiDirectory)),
		);
		assertNoTransactionResidue(fixture);
	});
});

let failures = 0;
for (const { body, name } of tests) {
	try {
		await body();
		console.log(`ok   - ${name}`);
	} catch (error) {
		failures += 1;
		console.error(`FAIL - ${name}`);
		console.error(error.stack ?? error);
	}
}
if (failures > 0) {
	console.error(`generate-ui-notices.test.mjs: ${failures} failure(s)`);
	process.exit(1);
}
console.log(`generate-ui-notices.test.mjs: all ${tests.length} tests passed.`);
