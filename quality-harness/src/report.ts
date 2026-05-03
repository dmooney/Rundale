import { AuditResult } from "./types";

export function toMarkdown(audit: AuditResult, mutation: any): string {
  const findings = audit.findings.map((f) => `- **${f.severity}** ${f.title}: ${f.detail}`).join("\n") || "- None";
  const matrix = audit.coverage_matrix.map((c) => `- ${c.area}: ${c.status}`).join("\n") || "- None";
  return `## PR Quality Harness Report\n\n**Overall:** ${audit.result}\n\n**Summary:** ${audit.summary}\n\n### Key Findings\n${findings}\n\n### Coverage Matrix\n${matrix}\n\n### Mutation Smoke\n- Killed: ${mutation?.killed?.length || 0}\n- Survived: ${mutation?.survived?.length || 0}\n- Skipped: ${mutation?.skipped?.length || 0}\n`;
}
