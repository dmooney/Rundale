'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');

const workflowPath = path.join(__dirname, '..', 'workflows', 'full-ci.yml');

test('every full-suite Rust cache targets the nested Cargo workspace', () => {
	const workflow = fs.readFileSync(workflowPath, 'utf8');
	const cacheSteps = workflow.match(
		/^      - name: Cache Rust artifacts\n(?:^        .*\n)+/gm,
	);

	assert.ok(cacheSteps, 'full-ci.yml has no Rust cache steps');
	assert.equal(cacheSteps.length, 6, 'unexpected Rust cache step count');
	for (const cacheStep of cacheSteps) {
		assert.match(cacheStep, /workspaces: limerick -> target/);
	}
});
