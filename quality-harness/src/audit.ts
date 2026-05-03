import { AuditResult } from "./types";

export function validateAuditResult(raw: unknown): AuditResult {
  const d = raw as AuditResult;
  if (!["pass", "warn", "fail"].includes(d.result)) throw new Error("invalid audit result");
  if (!Array.isArray(d.findings)) throw new Error("invalid findings");
  d.coverage_matrix ||= [];
  d.suspicious_test_gaming_signals ||= [];
  d.recommended_mutations ||= [];
  return d;
}
