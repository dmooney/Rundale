import fs from "fs";
import { getOctokit, parseRepo } from "../src/github";
import { toMarkdown } from "../src/report";

async function main() {
  const pr = Number(process.env.PR_NUMBER || 0);
  if (!pr) return;
  const { owner, repo } = parseRepo();
  const octokit = getOctokit();
  const audit = JSON.parse(fs.readFileSync(".quality-harness/audit-result.json", "utf8"));
  const mutation = fs.existsSync(".quality-harness/mutation-result.json") ? JSON.parse(fs.readFileSync(".quality-harness/mutation-result.json", "utf8")) : null;
  const body = toMarkdown(audit, mutation);
  const comments = await octokit.issues.listComments({ owner, repo, issue_number: pr });
  const existing = comments.data.find((c) => c.body?.includes("## PR Quality Harness Report"));
  if (existing) await octokit.issues.updateComment({ owner, repo, comment_id: existing.id, body });
  else await octokit.issues.createComment({ owner, repo, issue_number: pr, body });
}
main().catch((e) => { console.error(e); process.exit(1); });
