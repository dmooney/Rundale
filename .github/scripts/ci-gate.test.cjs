'use strict';

const assert = require('node:assert/strict');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
const test = require('node:test');

const gate = path.join(__dirname, 'ci-gate.sh');

function runGate({
	gatedResults = 'success success skipped success',
	playwrightWindowsResult = 'success',
	runtimeRequired,
	runtimeResult,
}) {
	return spawnSync('bash', [gate], {
		encoding: 'utf8',
		env: {
			...process.env,
			GATED_RESULTS: gatedResults,
			PLAYWRIGHT_WINDOWS_RESULT: playwrightWindowsResult,
			RUNTIME_SUITE_REQUIRED: runtimeRequired,
			RUNTIME_SUITE_RESULT: runtimeResult,
		},
	});
}

test('Windows Playwright launcher lifecycle must succeed', async (t) => {
	for (const [playwrightWindowsResult, expectedStatus] of [
		['success', 0],
		['skipped', 1],
		['failure', 1],
		['cancelled', 1],
	]) {
		await t.test(playwrightWindowsResult, () => {
			const result = runGate({
				playwrightWindowsResult,
				runtimeRequired: 'false',
				runtimeResult: 'skipped',
			});
			assert.equal(
				result.status,
				expectedStatus,
				result.stdout + result.stderr,
			);
		});
	}
});

test('required runtime correctness suite passes only when it succeeds', async (t) => {
	const cases = [
		['success', 0],
		['skipped', 1],
		['failure', 1],
		['cancelled', 1],
	];

	for (const [runtimeResult, expectedStatus] of cases) {
		await t.test(runtimeResult, () => {
			const result = runGate({
				gatedResults: `success success ${runtimeResult}`,
				runtimeRequired: 'true',
				runtimeResult,
			});
			assert.equal(
				result.status,
				expectedStatus,
				result.stdout + result.stderr,
			);
		});
	}
});

test('non-runtime pull requests pass only when runtime suite is skipped', async (t) => {
	const cases = [
		['skipped', 0],
		['success', 1],
		['failure', 1],
		['cancelled', 1],
	];

	for (const [runtimeResult, expectedStatus] of cases) {
		await t.test(runtimeResult, () => {
			const result = runGate({
				gatedResults: `success skipped ${runtimeResult}`,
				runtimeRequired: 'false',
				runtimeResult,
			});
			assert.equal(
				result.status,
				expectedStatus,
				result.stdout + result.stderr,
			);
		});
	}
});

test('an ordinary dependency failure still fails the aggregate gate', () => {
	const result = runGate({
		gatedResults: 'success failure skipped',
		runtimeRequired: 'false',
		runtimeResult: 'skipped',
	});

	assert.equal(result.status, 1, result.stdout + result.stderr);
	assert.match(result.stdout, /required CI job ended with 'failure'/);
});

test('an ordinary dependency cancellation still fails the aggregate gate', () => {
	const result = runGate({
		gatedResults: 'success cancelled success',
		runtimeRequired: 'true',
		runtimeResult: 'success',
	});

	assert.equal(result.status, 1, result.stdout + result.stderr);
	assert.match(result.stdout, /required CI job ended with 'cancelled'/);
});

test('an invalid runtime-suite requirement value fails closed', () => {
	const result = runGate({
		gatedResults: 'success skipped',
		runtimeRequired: 'unknown',
		runtimeResult: 'skipped',
	});

	assert.equal(result.status, 1, result.stdout + result.stderr);
	assert.match(result.stdout, /must be 'true' or 'false'/);
});
