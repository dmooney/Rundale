#!/usr/bin/env node
// Scores a head-to-head judging.
//   node insession/pairscore.mjs <label>
// Reads output/insession/judgments/<label>.json: [{id, better: 1|2|0, s1, s2, why}]
// (0 is a tie; s1 and s2 are 0.0-1.0 rubric scores). Reports, for runB against
// runA: wins, losses, ties, the sign-test p-value (two-sided, ties dropped),
// the mean score of each and the cases B lost, with the judge's reason.
import path from 'node:path';
import { data, readJson, mean } from './lib.mjs';

const label = process.argv[2];
const key = readJson(path.join(data, 'keys', `${label}.json`));
const verdicts = readJson(path.join(data, 'judgments', `${label}.json`));
let w = 0,
	l = 0,
	t = 0;
const sa = [],
	sb = [],
	lost = [];
for (const v of verdicts) {
	const k = key[v.id];
	if (!k) throw new Error(`unknown id ${v.id}`);
	const [a, b] = k.flip ? [v.s2, v.s1] : [v.s1, v.s2];
	if (![a, b].every((x) => typeof x === 'number' && x >= 0 && x <= 1))
		throw new Error(`bad scores on ${v.id}`);
	sa.push(a * 100);
	sb.push(b * 100);
	const bWon = v.better === 0 ? 0 : (v.better === 2) !== k.flip ? 1 : -1;
	if (bWon > 0) w++;
	else if (bWon < 0) {
		l++;
		lost.push(`${k.case}: ${v.why}`);
	} else t++;
}
const missing = Object.keys(key).filter(
	(id) => !verdicts.some((v) => v.id === id),
);
if (missing.length) throw new Error(`no verdict for ${missing.join(',')}`);
const n = w + l;
let p = 0;
const choose = (n, k) => {
	let c = 1;
	for (let i = 0; i < k; i++) c = (c * (n - i)) / (i + 1);
	return c;
};
for (let k = 0; k <= Math.min(w, l); k++) p += choose(n, k);
p = Math.min(1, (2 * p) / 2 ** n);
const [ra, rb] = [Object.values(key)[0].a, Object.values(key)[0].b];
console.log(
	`${label}: ${rb} vs ${ra}: wins ${w}, losses ${l}, ties ${t}; sign-test p=${p.toFixed(3)}; mean ${mean(sb).toFixed(1)} vs ${mean(sa).toFixed(1)}`,
);
if (lost.length) console.log('B lost:\n  ' + lost.join('\n  '));
