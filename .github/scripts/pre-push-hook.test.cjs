'use strict';

// Drives .githooks/pre-push in a throwaway repository whose tools are stubs
// that log their arguments, so each test sees exactly which checks ran.

const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
const test = require('node:test');

const hook = path.join(__dirname, '..', '..', '.githooks', 'pre-push');

function git(cwd, ...args) {
	const result = spawnSync(
		'git',
		['-c', 'user.name=Test', '-c', 'user.email=test@example.invalid', ...args],
		{ cwd, encoding: 'utf8' },
	);
	assert.equal(result.status, 0, result.stderr);
}

function stub(file, body) {
	fs.mkdirSync(path.dirname(file), { recursive: true });
	fs.writeFileSync(file, `#!/usr/bin/env bash\n${body}\n`, { mode: 0o755 });
}

// fail: names of stubs that exit 1 ('ensure-py', 'mypy', 'ruff').
function repository({ changed, fail = [] }) {
	const root = fs.mkdtempSync(path.join(os.tmpdir(), 'pre-push-'));
	const log = path.join(root, 'calls.log');
	const record = (name) =>
		`echo "${name} $*" >> "${log}"${fail.includes(name) ? '\nexit 1' : ''}`;
	stub(
		path.join(root, 'limerick/scripts/ensure-deps.sh'),
		`if [ "$1" = py ]; then ${record('ensure-py').replaceAll('\n', '; ')}; fi`,
	);
	stub(
		path.join(root, 'limerick/scripts/check-docs-format.sh'),
		record('docs'),
	);
	stub(path.join(root, '.venv-dev/bin/ruff'), record('ruff'));
	stub(path.join(root, '.venv-dev/bin/mypy'), record('mypy'));
	fs.mkdirSync(path.join(root, '.githooks'));
	fs.copyFileSync(hook, path.join(root, '.githooks/pre-push'));
	fs.writeFileSync(path.join(root, '.gitignore'), '.venv-dev/\ncalls.log\n');
	git(root, 'init', '-q', '-b', 'main');
	git(root, 'add', '-A');
	git(root, 'commit', '-q', '-m', 'base');
	git(root, 'update-ref', 'refs/remotes/origin/main', 'HEAD');
	fs.writeFileSync(path.join(root, changed), 'x = 1\n');
	git(root, 'add', '-A');
	git(root, 'commit', '-q', '-m', 'change');
	return { root, log };
}

function push(root) {
	return spawnSync('bash', ['.githooks/pre-push'], {
		cwd: root,
		encoding: 'utf8',
		input: '',
	});
}

function calls(log) {
	return fs.existsSync(log)
		? fs
				.readFileSync(log, 'utf8')
				.trim()
				.split('\n')
				.map((line) => line.trim())
		: [];
}

test('a Python change runs ruff and mypy repository-wide', () => {
	const { root, log } = repository({ changed: 'tool.py' });
	const result = push(root);
	assert.equal(result.status, 0, result.stdout + result.stderr);
	assert.deepEqual(calls(log), [
		'docs',
		'ensure-py py',
		'ruff check .',
		'ruff format --check .',
		'mypy .',
	]);
});

test('a change without Python runs only the docs check', () => {
	const { root, log } = repository({ changed: 'notes.md' });
	const result = push(root);
	assert.equal(result.status, 0, result.stdout + result.stderr);
	assert.deepEqual(calls(log), ['docs']);
});

for (const failing of ['mypy', 'ruff', 'ensure-py']) {
	test(`a failing ${failing} rejects the push`, () => {
		const { root } = repository({ changed: 'tool.py', fail: [failing] });
		assert.notEqual(push(root).status, 0);
	});
}
