'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');

const workflowPath = path.join(
	__dirname,
	'..',
	'workflows',
	'dependabot-auto-merge.yml',
);

const forbiddenMergeMutations = [
	[/\bgh\s+pr\s+merge\b/i, 'GitHub CLI pull-request merge'],
	[/(?:^|\s)--auto\b/i, 'GitHub CLI automatic-merge flag'],
	[/\benablePullRequestAutoMerge\b/i, 'GraphQL automatic-merge mutation'],
	[/\bmergePullRequest\b/i, 'GraphQL pull-request merge mutation'],
	[/\/pulls\/[^\s'"]+\/merge\b/i, 'REST pull-request merge endpoint'],
	[/\bpulls\s*\.\s*merge\b/i, 'Octokit pull-request merge call'],
	[/uses:\s*[^\n]*(?:auto-merge|automerge)/i, 'automatic-merge action'],
];

const forbiddenPermissionEscalations = [
	[/\bpermissions\s*:\s*['"]?write-all\b/i, 'workflow-wide write permission'],
];

function findMergeMutations(source) {
	return forbiddenMergeMutations
		.filter(([pattern]) => pattern.test(source))
		.map(([, description]) => description);
}

function findPermissionEscalations(source) {
	return forbiddenPermissionEscalations
		.filter(([pattern]) => pattern.test(source))
		.map(([, description]) => description);
}

test('Dependabot workflow leaves every merge decision to the coordinator', () => {
	const workflow = fs.readFileSync(workflowPath, 'utf8');

	assert.deepEqual(findMergeMutations(workflow), []);
	assert.deepEqual(findPermissionEscalations(workflow), []);
	assert.match(workflow, /merge remains coordinator-owned/i);
	assert.match(workflow, /contents:\s*read/i);
	assert.doesNotMatch(workflow, /contents:\s*write/i);
});

test('sensor recognizes representative automatic-merge mechanisms', async (t) => {
	const cases = [
		['gh pr merge --auto --squash $PR_URL', 'GitHub CLI pull-request merge'],
		['some-merge-command --auto', 'GitHub CLI automatic-merge flag'],
		[
			'mutation { enablePullRequestAutoMerge(input: $input) { clientMutationId } }',
			'GraphQL automatic-merge mutation',
		],
		[
			'gh api --method PUT repos/example/project/pulls/42/merge',
			'REST pull-request merge endpoint',
		],
		[
			'uses: pascalgn/automerge-action@0123456789abcdef',
			'automatic-merge action',
		],
		[
			'await github.rest.pulls.merge({ owner, repo, pull_number })',
			'Octokit pull-request merge call',
		],
	];

	for (const [source, expected] of cases) {
		await t.test(expected, () => {
			assert.ok(findMergeMutations(source).includes(expected));
		});
	}
});

test('sensor recognizes workflow-wide write permissions', () => {
	const source = 'permissions: write-all';

	assert.deepEqual(findPermissionEscalations(source), [
		'workflow-wide write permission',
	]);
});
