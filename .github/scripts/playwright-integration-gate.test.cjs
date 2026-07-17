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
	const uiFilterMatch = changesJob.match(
		/^ {12}ui:\n((?:^ {14}- '[^']+'\n?)+)/m,
	);
	assert.ok(uiFilterMatch, 'missing ui path filter');
	const uiPatterns = [...uiFilterMatch[1].matchAll(/'([^']+)'/g)].map(
		(match) => match[1],
	);
	assert.ok(
		uiPatterns.includes('parish/crates/parish-server/**'),
		'parish-server build and readiness changes must select UI Playwright e2e',
	);

	const uiE2eJob = section(
		workflow,
		'  ui-e2e:',
		'  playwright-launcher-windows:',
	);
	assert.match(
		uiE2eJob,
		/needs\.changes\.outputs\.ui == 'true'/,
		'ui-e2e must be selected by the guarded path filter',
	);
	assert.match(
		uiE2eJob,
		/npm run test:playwright-launcher-integration/,
		'selected server seam changes must run the real launcher integration',
	);
});
