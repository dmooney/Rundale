'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');

const workflowPath = path.join(__dirname, '..', 'workflows', 'ci.yml');

function agentProofCondition(source) {
	const jobStart = source.indexOf('\n  agent-check:');
	const nextJob = source.indexOf('\n  docs-consistency:', jobStart);

	assert.notEqual(jobStart, -1, 'ci.yml must define the agent-check job');
	assert.notEqual(nextJob, -1, 'agent-check job must precede docs-consistency');

	const job = source.slice(jobStart, nextJob);
	const condition = job.match(/^    if:\s*(.+)$/m);

	assert.ok(condition, 'agent-check job must have an if condition');
	return condition[1].trim();
}

function resolveOperand(operand, context) {
	if (operand.startsWith("'") && operand.endsWith("'")) {
		return operand.slice(1, -1);
	}

	const values = {
		'github.event_name': context.eventName,
		'github.actor': context.actor,
		'github.event.pull_request.user.login': context.pullRequestAuthor,
	};

	assert.ok(
		Object.hasOwn(values, operand),
		`unsupported workflow operand: ${operand}`,
	);
	return values[operand];
}

function evaluateCondition(condition, context) {
	return condition.split(/\s*&&\s*/).every((predicate) => {
		const comparison = predicate.match(/^(\S+)\s*(==|!=)\s*(\S+)$/);

		assert.ok(comparison, `unsupported workflow predicate: ${predicate}`);
		const [, left, operator, right] = comparison;
		const leftValue = resolveOperand(left, context);
		const rightValue = resolveOperand(right, context);

		return operator === '=='
			? leftValue === rightValue
			: leftValue !== rightValue;
	});
}

test('Agent proof gate trusts immutable pull-request authorship', () => {
	const workflow = fs.readFileSync(workflowPath, 'utf8');
	const condition = agentProofCondition(workflow);

	assert.match(condition, /github\.event\.pull_request\.user\.login/);
	assert.doesNotMatch(condition, /github\.actor/);
});

test('Agent proof gate author/actor matrix preserves only the Dependabot-author exemption', async (t) => {
	const workflow = fs.readFileSync(workflowPath, 'utf8');
	const condition = agentProofCondition(workflow);
	const cases = [
		{
			name: 'Dependabot author and coordinator synchronize actor',
			pullRequestAuthor: 'dependabot[bot]',
			actor: 'portfolio-coordinator',
			expected: false,
		},
		{
			name: 'human author and Dependabot synchronize actor',
			pullRequestAuthor: 'human-contributor',
			actor: 'dependabot[bot]',
			expected: true,
		},
		{
			name: 'Dependabot author and Dependabot actor',
			pullRequestAuthor: 'dependabot[bot]',
			actor: 'dependabot[bot]',
			expected: false,
		},
		{
			name: 'human author and coordinator actor',
			pullRequestAuthor: 'human-contributor',
			actor: 'portfolio-coordinator',
			expected: true,
		},
	];

	for (const testCase of cases) {
		await t.test(testCase.name, () => {
			assert.equal(
				evaluateCondition(condition, {
					eventName: 'pull_request',
					actor: testCase.actor,
					pullRequestAuthor: testCase.pullRequestAuthor,
				}),
				testCase.expected,
			);
		});
	}
});

test('Agent proof gate stays disabled outside pull-request events', () => {
	const workflow = fs.readFileSync(workflowPath, 'utf8');
	const condition = agentProofCondition(workflow);

	assert.equal(
		evaluateCondition(condition, {
			eventName: 'push',
			actor: 'human-contributor',
			pullRequestAuthor: 'human-contributor',
		}),
		false,
	);
});
