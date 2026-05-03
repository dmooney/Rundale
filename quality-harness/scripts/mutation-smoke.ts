import fs from "fs";
import path from "path";
import { loadConfig } from "../src/config";
import { writeJson } from "../src/utils";
import { withMutation, MutationRun } from "../src/mutation";
import booleanFlip from "../src/operators/booleanFlip";
import comparisonFlip from "../src/operators/comparisonFlip";
import returnDefault from "../src/operators/returnDefault";
import removeThrow from "../src/operators/removeThrow";
import bypassBranch from "../src/operators/bypassBranch";

const operators: Record<string, (s: string) => string | null> = { booleanFlip, comparisonFlip, returnDefault, removeThrow, bypassBranch };
const result: MutationRun = { killed: [], survived: [], skipped: [] };
const cfg = loadConfig();
if (!cfg.mutation.enabled) { writeJson(".quality-harness/mutation-result.json", result); process.exit(0); }
const audit = JSON.parse(fs.readFileSync(".quality-harness/audit-result.json", "utf8"));
const picks = (audit.recommended_mutations || []).slice(0, cfg.mutation.max_mutations);
for (const m of picks) {
  const op = operators[m.operator];
  if (!op || !fs.existsSync(m.file)) { result.skipped.push(m); continue; }
  const outcome = withMutation(path.resolve(m.file), op, cfg.commands.related_tests || cfg.commands.test);
  (result as any)[outcome.status].push(m);
}
writeJson(".quality-harness/mutation-result.json", result);
