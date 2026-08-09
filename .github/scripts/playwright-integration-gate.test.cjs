'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');

const workflowPath = path.join(__dirname, '..', 'workflows', 'ci.yml');

function section(source, startMarker, endMarker) {
	const start = source.indexOf(startMarker);
	assert.notEqual(start, -1, `missing workflow section: ${startMarker}`);
	const end = source.indexOf(endMarker, start + startMarker.length);
	assert.notEqual(end, -1, `missing workflow boundary: ${endMarker}`);
	return source.slice(start, end);
}

test('server-side Playwright seam changes require the real launcher integration', () => {
	const workflow = fs.readFileSync(workflowPath, 'utf8');
	const changesJob = section(workflow, '  changes:', '  agent-check:');
	const runtimeFilterMatch = changesJob.match(
		/^ {12}runtime:\n((?:^ {14}- '[^']+'\n?)+)/m,
	);
	assert.ok(runtimeFilterMatch, 'missing runtime path filter');
	const runtimePatterns = [...runtimeFilterMatch[1].matchAll(/'([^']+)'/g)].map(
		(match) => match[1],
	);
	assert.ok(
		runtimePatterns.includes('parish/crates/**'),
		'parish-server build and readiness changes must select the runtime suite',
	);

	const runtimeSuiteJob = section(
		workflow,
		'  runtime-suite:',
		'  playwright-launcher-windows:',
	);
	assert.match(
		runtimeSuiteJob,
		/needs\.changes\.outputs\.runtime == 'true'/,
		'runtime suite must be selected by the guarded path filter',
	);
	assert.match(runtimeSuiteJob, /uses: \.\/\.github\/workflows\/full-ci\.yml/);

	const fullWorkflow = fs.readFileSync(
		path.join(__dirname, '..', 'workflows', 'full-ci.yml'),
		'utf8',
	);
	const uiE2eJob = section(fullWorkflow, '  ui-e2e:', '  full-ci-gate:');
	assert.match(
		uiE2eJob,
		/npm run test:playwright-launcher-integration/,
		'selected server seam changes must run the real launcher integration',
	);
});
