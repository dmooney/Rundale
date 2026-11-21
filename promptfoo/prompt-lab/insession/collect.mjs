#!/usr/bin/env node
// Collects what the subagents handed back, from Claude Code's own transcripts,
// so no reply or verdict is retyped by the researcher.
//   node insession/collect.mjs <transcripts-dir>
// <transcripts-dir> is the session's subagents directory
// (~/.claude/projects/<project>/<session>/subagents). A transcript whose task
// names output/insession/render/<run>/request/<case>.txt is a stand-in reply;
// one that names output/insession/judge/<batch>.md is a judge's verdicts.
// Writes output/insession/replies/<run>.json and output/insession/judgments/<batch>.json.
// A case answered by more than one subagent keeps the first answer and reports the rest.
import fs from 'node:fs';
import path from 'node:path';
import { data, writeJson } from './lib.mjs';

const dir = process.argv[2];
if (!dir) throw new Error('usage: collect.mjs <transcripts-dir>');
const replies = {};
const judgments = {};
const notes = [];

const files = fs
	.readdirSync(dir)
	.filter((f) => /^agent-.*\.jsonl$/.test(f) && !f.includes('.prefix.'))
	.map((f) => {
		// Order by the transcript's first timestamp, so "the first answer" does
		// not depend on whether the filesystem records birth time.
		const first = fs
			.readFileSync(path.join(dir, f), 'utf8')
			.split('\n')
			.find((l) => l.includes('"timestamp"'));
		let t = Infinity;
		try {
			t = Date.parse(JSON.parse(first).timestamp);
		} catch {}
		return { f, t: Number.isNaN(t) ? Infinity : t };
	})
	.sort((a, b) => a.t - b.t);

for (const { f } of files) {
	const lines = fs
		.readFileSync(path.join(dir, f), 'utf8')
		.split('\n')
		.filter(Boolean)
		.map((l) => JSON.parse(l));
	const first = lines.find((l) => l.type === 'user');
	const task =
		typeof first?.message?.content === 'string'
			? first.message.content
			: JSON.stringify(first?.message?.content ?? '');
	let handback = null;
	let model = null;
	for (const l of lines) {
		if (l.type !== 'assistant' || !Array.isArray(l.message?.content)) continue;
		model = l.message.model ?? model;
		for (const part of l.message.content) {
			if (part.type === 'tool_use' && part.name === 'SubagentHandback')
				handback = part.input?.message ?? null;
		}
	}
	const request = task.match(
		/output\/insession\/render\/([^/\s]+)\/request\/([^/\s]+)\.txt/,
	);
	const batch = task.match(/output\/insession\/judge\/([^/\s]+)\.md/);
	if (request) {
		const [, run, id] = request;
		if (handback === null) {
			notes.push(`${run}/${id}: ${f} handed nothing back`);
			continue;
		}
		replies[run] ??= {};
		if (replies[run][id])
			notes.push(`${run}/${id}: extra answer in ${f} ignored`);
		else replies[run][id] = { raw: handback.trim(), model, transcript: f };
	} else if (batch) {
		if (handback === null) {
			notes.push(`judge ${batch[1]}: ${f} handed nothing back`);
			continue;
		}
		let verdicts;
		try {
			verdicts = JSON.parse(
				handback.trim().replace(/^```(?:json)?\s*|\s*```$/g, ''),
			);
		} catch {
			notes.push(`judge ${batch[1]}: ${f} did not hand back JSON`);
			continue;
		}
		if (judgments[batch[1]])
			notes.push(`judge ${batch[1]}: extra verdicts in ${f} ignored`);
		else judgments[batch[1]] = { model, transcript: f, verdicts };
	}
}

// Merge into what is already collected, so collecting from a second session's
// transcripts never drops replies gathered from the first; a case already
// collected keeps its earlier answer.
for (const [run, byCase] of Object.entries(replies)) {
	const file = path.join(data, 'replies', `${run}.json`);
	const before = fs.existsSync(file)
		? JSON.parse(fs.readFileSync(file, 'utf8'))
		: {};
	writeJson(file, { ...byCase, ...before });
}
for (const [batch, value] of Object.entries(judgments))
	writeJson(path.join(data, 'judgments', `${batch}.json`), value);
console.log(
	Object.entries(replies)
		.map(([run, byCase]) => `${run}: ${Object.keys(byCase).length} replies`)
		.join('\n') + `\n${Object.keys(judgments).length} judge batches`,
);
for (const note of notes) console.log('note: ' + note);
