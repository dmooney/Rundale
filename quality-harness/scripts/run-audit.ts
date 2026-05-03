import fs from "fs";
import { aiJson } from "../src/ai";
import { validateAuditResult } from "../src/audit";
import { writeJson } from "../src/utils";
import { toMarkdown } from "../src/report";

async function main() {
  const context = JSON.parse(fs.readFileSync(".quality-harness/context.json", "utf8"));
  const raw = await aiJson("prompts/test-auditor.md", context);
  const parsed = validateAuditResult(JSON.parse(raw));
  writeJson(".quality-harness/audit-result.json", parsed);
  fs.writeFileSync(".quality-harness/audit-report.md", toMarkdown(parsed, null), "utf8");
}
main().catch((e) => { console.error(e); process.exit(1); });
