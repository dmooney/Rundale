#!/usr/bin/env node
// Scores judged batches.
//   node insession/score.mjs <label> [<label>...]         a summary per run
//   node insession/score.mjs --stability <label> <label>  how far repeat judging moved
// Reads output/insession/keys/<label>.json and the judges' verdicts in
// output/insession/judgments/. A reply's score is the mean of its verdicts
// (0 to 100) across the labels given; a malformed reply scores 0 whatever a
// judge said, as in summary.jq. Writes output/insession/scores/<label>.json.
import fs from "node:fs";
import path from "node:path";
import { data, readJson, writeJson, mean, spoken } from "./lib.mjs";

const args = process.argv.slice(2);
const stability = args[0] === "--stability";
const labels = stability ? args.slice(1) : args;
if (!labels.length) throw new Error("usage: score.mjs [--stability] <label>...");

// label -> [{run, case, score, reason}]
function verdicts(label) {
  const key = readJson(path.join(data, "keys", `${label}.json`));
  const rows = [];
  for (const [batch, items] of Object.entries(key.batches)) {
    const file = path.join(data, "judgments", `${batch}.json`);
    if (!fs.existsSync(file)) throw new Error(`${label}: batch ${batch} has no verdicts yet`);
    const byId = Object.fromEntries(readJson(file).verdicts.map((v) => [v.id, v]));
    for (const [id, where] of Object.entries(items)) {
      const v = byId[id];
      if (!v || typeof v.score !== "number" || v.score < 0 || v.score > 1) {
        throw new Error(`${label}: batch ${batch} item ${id} has no usable verdict`);
      }
      rows.push({ ...where, score: Math.round(v.score * 1000) / 10, reason: v.reason ?? "" });
    }
  }
  return rows;
}

if (stability) {
  const [a, b] = labels.map(verdicts);
  const pairs = [];
  for (const x of a) {
    const y = b.find((r) => r.run === x.run && r.case === x.case);
    if (y) pairs.push({ run: x.run, case: x.case, first: x.score, second: y.score, moved: Math.round((y.score - x.score) * 10) / 10 });
  }
  const abs = pairs.map((p) => Math.abs(p.moved));
  console.log(
    JSON.stringify(
      {
        replies: pairs.length,
        mean_first: mean(pairs.map((p) => p.first)),
        mean_second: mean(pairs.map((p) => p.second)),
        mean_absolute_move: mean(abs),
        largest_move: Math.max(...abs),
        moved_over_10: pairs.filter((p) => Math.abs(p.moved) > 10).length,
        moved_over_20: pairs.filter((p) => Math.abs(p.moved) > 20).length,
        pairs,
      },
      null,
      1,
    ),
  );
  process.exit(0);
}

const all = labels.flatMap((label) => verdicts(label).map((r) => ({ ...r, label })));
const runs = [...new Set(all.map((r) => r.run))];
const summary = {};
for (const run of runs) {
  const cases = readJson(path.join(data, "render", run.split("@")[0], "cases.json"));
  const replies = readJson(path.join(data, "replies", `${run}.json`));
  const rows = [];
  for (const c of cases) {
    const mine = all.filter((r) => r.run === run && r.case === c.id);
    if (!mine.length) continue;
    const said = spoken(replies[c.id].raw);
    const malformed = said.startsWith("MALFORMED: ");
    rows.push({
      case: c.id,
      tags: c.tags,
      points: malformed ? 0 : mean(mine.map((r) => r.score)),
      malformed,
      verdicts: mine.map((r) => r.score),
      reply: said,
      reasons: mine.map((r) => r.reason),
    });
  }
  const tags = [...new Set(rows.flatMap((r) => r.tags))].sort((x, y) => Number(x.slice(1)) - Number(y.slice(1)));
  summary[run] = {
    scored: rows.length,
    malformed: rows.filter((r) => r.malformed).length,
    mean: mean(rows.map((r) => r.points)),
    by_requirement: Object.fromEntries(tags.map((t) => [t, mean(rows.filter((r) => r.tags.includes(t)).map((r) => r.points))])),
    verdicts_split_over_20: rows.filter((r) => Math.max(...r.verdicts) - Math.min(...r.verdicts) > 20).map((r) => r.case),
    rows: rows.sort((x, y) => x.points - y.points),
  };
}
writeJson(path.join(data, "scores", `${labels.join("+")}.json`), summary);
for (const [run, s] of Object.entries(summary)) {
  console.log(`\n== ${run}: mean ${s.mean} over ${s.scored} replies, ${s.malformed} malformed`);
  console.log(Object.entries(s.by_requirement).map(([t, v]) => `${t} ${v}`).join(", "));
  for (const r of s.rows) console.log(`${String(r.points).padStart(5)}  ${r.case}\n       “${r.reply}”\n       ${r.reasons.join(" | ")}`);
}
