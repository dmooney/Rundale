#!/usr/bin/env node
// Builds a blind head-to-head judging file: the same case answered under two
// runs, shown as Reply 1 and Reply 2 in a seeded random order.
//   node insession/pair.mjs <label> <runA> <runB> [<runA2> <runB2> ...]
// Pairs are taken run by run (A with B, A2 with B2: repeat samples), case by
// case. Writes output/insession/judge/<label>.md and the key to
// output/insession/keys/<label>.json. Both runs must share a case set; the
// rubric comes from runA's render.
import fs from 'node:fs';
import path from 'node:path';
import { data, readJson, writeJson, seeded, shuffle, spoken } from './lib.mjs';

const [label, ...runs] = process.argv.slice(2);
if (!label || runs.length < 2 || runs.length % 2)
	throw new Error('usage: pair.mjs <label> <runA> <runB> [...]');
const random = seeded(label);
const items = [];
for (let i = 0; i < runs.length; i += 2) {
	const [a, b] = [runs[i], runs[i + 1]];
	const ra = readJson(path.join(data, 'replies', `${a}.json`));
	const rb = readJson(path.join(data, 'replies', `${b}.json`));
	for (const id of Object.keys(ra).sort()) {
		if (!rb[id]) continue;
		const flip = random() < 0.5;
		items.push({
			a,
			b,
			case: id,
			flip,
			ra: spoken(ra[id].raw),
			rb: spoken(rb[id].raw),
		});
	}
}
const ordered = shuffle(items, random);
const key = {};
let md = `# Head-to-head judging: ${label}\n\n`;
ordered.forEach((it, n) => {
	const id = `p${String(n + 1).padStart(2, '0')}`;
	key[id] = { a: it.a, b: it.b, case: it.case, flip: it.flip };
	const rubric = fs
		.readFileSync(
			path.join(data, 'render', it.a.split('@')[0], 'rubric', `${it.case}.txt`),
			'utf8',
		)
		.trim();
	const [one, two] = it.flip ? [it.rb, it.ra] : [it.ra, it.rb];
	md += `## ${id}\n\n### Rubric\n\n${rubric}\n\n### Reply 1\n\n${one}\n\n### Reply 2\n\n${two}\n\n`;
});
fs.mkdirSync(path.join(data, 'judge'), { recursive: true });
fs.writeFileSync(path.join(data, 'judge', `${label}.md`), md);
writeJson(path.join(data, 'keys', `${label}.json`), key);
console.log(`${label}: ${ordered.length} pairs`);
