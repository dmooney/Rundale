export type ResultLevel = "pass" | "warn" | "fail";

export interface HarnessConfig {
  project: { name: string; language: string; description?: string };
  commands: { install: string; lint?: string; typecheck?: string; test: string; related_tests?: string };
  tests: { directories: string[]; patterns: string[] };
  docs: { requirements: string[] };
  risk_areas: string[];
  audit: { minimum_result: ResultLevel; fail_on: string[] };
  mutation: {
    enabled: boolean;
    mode: "smoke";
    max_mutations: number;
    include: string[];
    exclude: string[];
    operators: string[];
  };
}

export interface MutationCandidate { file: string; operator: string; description: string; }

export interface AuditResult {
  result: ResultLevel;
  summary: string;
  confidence: "low" | "medium" | "high";
  coverage_matrix: Array<{ area: string; status: string; evidence?: string }>;
  findings: Array<{ id: string; severity: ResultLevel; title: string; detail: string }>;
  suspicious_test_gaming_signals: string[];
  recommended_mutations: MutationCandidate[];
}
