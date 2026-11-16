#!/usr/bin/env bash
# Renders one prompt file against one case file, offline, with promptfoo's echo
# provider (no model or judge calls, no keys), as check.sh does.
#   bash insession/render.sh <prompt.yaml> <cases.yaml> <outdir>
# Writes, for each case:
#   <outdir>/request/<case>.txt  the system instruction, a blank line, then the
#                                user part ("Endpoint input values:" and the JSON)
#   <outdir>/rubric/<case>.txt   the judges' rubric (x-rubric), filled in
#   <outdir>/cases.json          each case's id, description, tags and player line
set -euo pipefail
prompt=$(realpath "$1")
cases=$(realpath "$2")
mkdir -p "$3"
out=$(realpath "$3")
cd "$(dirname "$0")/.."
pf="$(cd .. && pwd)/node_modules/.bin/promptfoo"
export PROMPTFOO_DISABLE_TELEMETRY=1 PROMPTFOO_DISABLE_UPDATE=1
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
cp -r content "$tmp/"
mkdir "$tmp/prompts" "$tmp/cases"
cp "$prompt" "$tmp/prompts/under-test.yaml"
cp "$cases" "$tmp/cases/cases.yaml"

node -e '
  const yaml = require(require.resolve("js-yaml", { paths: ["../node_modules"] }));
  const fs = require("fs");
  const cfg = yaml.load(fs.readFileSync("promptfooconfig.yaml", "utf8"));
  cfg.prompts = [{ id: "file://prompts/under-test.yaml", label: "under-test" }];
  cfg.tests = "file://cases/cases.yaml";
  const judge = cfg.defaultTest.assert.find((a) => a.metric === "opus");
  const request = structuredClone(cfg);
  request.defaultTest.assert = cfg.defaultTest.assert.filter((a) => a.metric === "schema");
  fs.writeFileSync(process.argv[1] + "/request.yaml", yaml.dump(request));
  // An echo grader returns the rendered rubric instead of a verdict.
  cfg.defaultTest.assert = [{ ...judge, provider: "echo" }];
  cfg.defaultTest.options.transform = "\"Good day.\"";
  fs.writeFileSync(process.argv[1] + "/rubric.yaml", yaml.dump(cfg));
' "$tmp"
(cd "$tmp" && $pf eval -c request.yaml -r echo --no-cache --no-progress-bar -o request.json >/dev/null 2>&1) || true
(cd "$tmp" && $pf eval -c rubric.yaml -r echo --no-cache --no-progress-bar -o rubric.json >/dev/null 2>&1) || true

node -e '
  const fs = require("fs");
  const [tmp, out] = process.argv.slice(1);
  const rows = (f) => JSON.parse(fs.readFileSync(`${tmp}/${f}`, "utf8")).results.results;
  const id = (r) => r.testCase.description.split(" ")[0];
  fs.mkdirSync(`${out}/request`, { recursive: true });
  fs.mkdirSync(`${out}/rubric`, { recursive: true });
  const cases = [];
  for (const r of rows("request.json")) {
    const [system, user] = JSON.parse(r.prompt.raw);
    if (system.role !== "system" || user.role !== "user" || !user.content.startsWith("Endpoint input values:\n")) {
      throw new Error(`${id(r)}: not a system instruction and one user part`);
    }
    JSON.parse(user.content.split("\n").slice(1).join("\n"));
    fs.writeFileSync(`${out}/request/${id(r)}.txt`, `${system.content.trim()}\n\n${user.content.trim()}\n`);
    cases.push({
      id: id(r),
      description: r.testCase.description,
      tags: (r.testCase.description.match(/\(([^)]*)\)/) || [, ""])[1].split(/,\s*/).filter(Boolean),
      speaker: r.vars.speaker,
      player_line: r.vars.player_line,
    });
  }
  for (const r of rows("rubric.json")) {
    const rubric = r.gradingResult.componentResults[0].metadata.renderedAssertionValue;
    if (/[{][{]/.test(rubric)) throw new Error(`${id(r)}: rubric not filled in`);
    fs.writeFileSync(`${out}/rubric/${id(r)}.txt`, rubric.trim() + "\n");
  }
  fs.writeFileSync(`${out}/cases.json`, JSON.stringify(cases, null, 1) + "\n");
  console.log(`rendered ${cases.length} cases to ${out}`);
' "$tmp" "$out"
