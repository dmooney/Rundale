import { aggregateUsage, sumEstimatedCosts, type RuntimeError } from "@parish/runtime";

export interface FailureAccounting {
  providerRequestId?: string;
  inputTokens?: number;
  outputTokens?: number;
  totalTokens?: number;
  estimatedProviderCost?: string;
}

export function failureAccounting(error: RuntimeError): FailureAccounting {
  const attempts = error.attempts ?? [];
  const usage = aggregateUsage(attempts);
  return {
    ...(error.providerMetadata?.providerRequestId === undefined
      ? {}
      : { providerRequestId: error.providerMetadata.providerRequestId }),
    ...(usage.inputTokens === undefined ? {} : { inputTokens: usage.inputTokens }),
    ...(usage.outputTokens === undefined ? {} : { outputTokens: usage.outputTokens }),
    ...(usage.totalTokens === undefined ? {} : { totalTokens: usage.totalTokens }),
    ...(attempts.length === 0
      ? {}
      : {
          estimatedProviderCost: sumEstimatedCosts(
            attempts.map((attempt) => attempt.estimatedCostUsd),
          ),
        }),
  };
}
