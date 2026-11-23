#!/usr/bin/env node
// Merges the part files batched stand-ins write into one reply file per run.
//   node insession/merge.mjs <run>
// Reads output/insession/parts/<run>.*.json (each {case: rawReply}) and writes
// output/insession/replies/<run>.json ({case: {raw}}), the shape collect.mjs
// writes. Reports any case of the run's render that has no reply.
import fs from 'node:fs';
import path from 'node:path';
import { data, readJson, writeJson } from './lib.mjs';

const run = process.argv[2];
if (!run) throw new Error('usage: merge.mjs <run>');
const base = run.split('@')[0];
const dir = path.join(data, 'parts');
const out = {};
for (const f of fs.readdirSync(dir).filter((f) => f.startsWith(run + '.')))
	for (const [id, raw] of Object.entries(readJson(path.join(dir, f))))
		out[id] = { raw: typeof raw === 'string' ? raw : JSON.stringify(raw) };
const want = readJson(path.join(data, 'render', base, 'cases.json')).map(
	(c) => c.id,
);
const missing = want.filter((id) => !out[id]);
writeJson(path.join(data, 'replies', `${run}.json`), out);
console.log(
	`${run}: ${Object.keys(out).length} replies; missing: ${missing.join(',') || 'none'}`,
);
