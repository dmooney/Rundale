'use strict';

const assert = require('node:assert/strict');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
const test = require('node:test');

const gate = path.join(__dirname, 'ci-gate.sh');

function runGate({
	gatedResults = 'success success skipped success',
	playwrightWindowsResult = 'success',
	uiRequired,
	uiResult,
}) {
	return spawnSync('bash', [gate], {
		encoding: 'utf8',
		env: {
			...process.env,
			GATED_RESULTS: gatedResults,
			PLAYWRIGHT_WINDOWS_RESULT: playwrightWindowsResult,
			UI_E2E_REQUIRED: uiRequired,
			UI_E2E_RESULT: uiResult,
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
				uiRequired: 'false',
				uiResult: 'skipped',
			});
			assert.equal(
				result.status,
				expectedStatus,
				result.stdout + result.stderr,
			);
		});
	}
});

test('required UI Playwright passes only when it succeeds', async (t) => {
	const cases = [
		['success', 0],
		['skipped', 1],
		['failure', 1],
		['cancelled', 1],
	];

	for (const [uiResult, expectedStatus] of cases) {
		await t.test(uiResult, () => {
			const result = runGate({
				gatedResults: `success success ${uiResult}`,
				uiRequired: 'true',
				uiResult,
			});
			assert.equal(
				result.status,
				expectedStatus,
				result.stdout + result.stderr,
			);
		});
	}
});

test('non-UI pull requests pass only when UI Playwright is skipped', async (t) => {
	const cases = [
		['skipped', 0],
		['success', 1],
		['failure', 1],
		['cancelled', 1],
	];

	for (const [uiResult, expectedStatus] of cases) {
		await t.test(uiResult, () => {
			const result = runGate({
				gatedResults: `success skipped ${uiResult}`,
				uiRequired: 'false',
				uiResult,
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
		uiRequired: 'false',
		uiResult: 'skipped',
	});

	assert.equal(result.status, 1, result.stdout + result.stderr);
	assert.match(result.stdout, /required CI job ended with 'failure'/);
});

test('an ordinary dependency cancellation still fails the aggregate gate', () => {
	const result = runGate({
		gatedResults: 'success cancelled success',
		uiRequired: 'true',
		uiResult: 'success',
	});

	assert.equal(result.status, 1, result.stdout + result.stderr);
	assert.match(result.stdout, /required CI job ended with 'cancelled'/);
});

test('an invalid UI requirement value fails closed', () => {
	const result = runGate({
		gatedResults: 'success skipped',
		uiRequired: 'unknown',
		uiResult: 'skipped',
	});

	assert.equal(result.status, 1, result.stdout + result.stderr);
	assert.match(result.stdout, /must be 'true' or 'false'/);
});
