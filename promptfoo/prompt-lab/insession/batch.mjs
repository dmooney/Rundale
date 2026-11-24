#!/usr/bin/env node
// Builds blind judging batches from collected replies.
//   node insession/batch.mjs <label> <run>[,<run>...] [--per 9] [--cases t01,t05]
// Each batch is one Markdown file, output/insession/judge/<batch>.md, holding
// for every item the case's rendered rubric and the reply's spoken text. Items
// from all the named runs are shuffled together; the batch name and the item
// ids are opaque, so a judge cannot tell which prompt or round a reply came
// from. The key (which item is which run and case) goes to
// output/insession/keys/<label>.json, which no judge is shown.
// A run "x@2" is a repeat sampling of run x's rendered requests, read from
// replies/x@2.json with x's rubrics.
import fs from 'node:fs';
import path from 'node:path';
import { data, readJson, writeJson, seeded, shuffle, spoken } from './lib.mjs';

const [label, runList, ...rest] = process.argv.slice(2);
if (!label || !runList)
	throw new Error(
		'usage: batch.mjs <label> <run>[,<run>...] [--per N] [--cases a,b]',
	);
const opt = (name, fallback) =>
	rest.includes(name) ? rest[rest.indexOf(name) + 1] : fallback;
const per = Number(opt('--per', 9));
const only = opt('--cases', '').split(',').filter(Boolean);

const random = seeded(label);
const token = (n) =>
	Array.from(
		{ length: n },
		() => 'abcdefghjkmnpqrstuvwxyz23456789'[Math.floor(random() * 31)],
	).join('');

let items = [];
for (const run of runList.split(',')) {
	const rendered = path.join(data, 'render', run.split('@')[0]);
	const replies = readJson(path.join(data, 'replies', `${run}.json`));
	for (const c of readJson(path.join(rendered, 'cases.json'))) {
		if (only.length && !only.some((p) => c.id.startsWith(p))) continue;
		if (!replies[c.id]) throw new Error(`${run}: no reply for ${c.id}`);
		items.push({
			run,
			case: c.id,
			rubric: fs
				.readFileSync(path.join(rendered, 'rubric', `${c.id}.txt`), 'utf8')
				.trim(),
			reply: spoken(replies[c.id].raw),
		});
	}
}
items = shuffle(items, random);

const batches = {};
const count = Math.ceil(items.length / per);
for (let b = 0; b < count; b++) {
	// Deal the shuffled items round-robin so the batches are the same size within one.
	const mine = items.filter((_, i) => i % count === b);
	const name = `b-${token(6)}`;
	const key = {};
	const parts = [
		`# Judging batch ${name}`,
		'',
		`This file holds ${mine.length} items. Each item has a rubric and one reply. Judge each reply on its own, against its own rubric only.`,
		'',
	];
	for (const item of mine) {
		let id = token(4);
		while (key[id]) id = token(4);
		key[id] = { run: item.run, case: item.case };
		parts.push(
			`## Item ${id}`,
			'',
			'### Rubric',
			'',
			item.rubric,
			'',
			`### Reply to judge (item ${id})`,
			'',
			item.reply,
			'',
		);
	}
	fs.mkdirSync(path.join(data, 'judge'), { recursive: true });
	fs.writeFileSync(path.join(data, 'judge', `${name}.md`), parts.join('\n'));
	batches[name] = key;
}
writeJson(path.join(data, 'keys', `${label}.json`), {
	label,
	runs: runList.split(','),
	batches,
});
for (const [name, key] of Object.entries(batches)) {
	console.log(
		`${path.join(data, 'judge', `${name}.md`)}  (${Object.keys(key).length} items)`,
	);
}
