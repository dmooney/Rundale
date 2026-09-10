import type { EndpointVersionSnapshot, Id } from "@parish/domain";
import type { RuntimeAttempt } from "@parish/runtime";

export interface InvocationApiKey {
  id: Id;
  organizationId: Id;
  keyDigest: string;
  scopes: string[];
  status: "active" | "revoked";
  organizationStatus: "active" | "suspended";
  dailyInvocationQuota: number;
  organizationInferenceEnabled: boolean;
}

export interface ResolvedEndpoint {
  endpointId: Id;
  endpointSlug: string;
  endpointStatus: "active" | "disabled";
  endpointInferenceEnabled: boolean;
  version: EndpointVersionSnapshot;
}

export interface InvocationStart {
  requestId: string;
  callerOrganizationId: Id;
  endpointId: Id;
  endpointVersionId: Id | null;
  endpointDraftId: Id | null;
  apiKeyId: Id | null;
  isTest: boolean;
  inputBytes: number;
  provider: string;
  model: string;
}

export class InvocationQuotaExceededError extends Error {
  constructor() {
    super("The daily invocation quota was exceeded.");
    this.name = "InvocationQuotaExceededError";
  }
}

export interface InvocationRepository {
  findApiKeyByPrefix(prefix: string): Promise<InvocationApiKey | null>;
  resolveProduction(
    organizationSlug: string,
    endpointSlug: string,
  ): Promise<ResolvedEndpoint | null>;
  resolveVersion(
    organizationSlug: string,
    endpointSlug: string,
    version: number,
  ): Promise<ResolvedEndpoint | null>;
  isInferenceEnabled(provider: string, model: string): Promise<boolean>;
  createInvocation(start: InvocationStart, dailyInvocationQuota: number): Promise<Id>;
  recordAttempts(invocationId: Id, attempts: readonly RuntimeAttempt[]): Promise<void>;
  finalizeSuccess(
    invocationId: Id,
    result: {
      durationMs: number;
      outputBytes: number;
      providerRequestId?: string;
      inputTokens?: number;
      outputTokens?: number;
      totalTokens?: number;
      estimatedProviderCost: string;
    },
  ): Promise<boolean>;
  finalizeFailure(
    invocationId: Id,
    result: {
      durationMs: number;
      errorCode: string;
      validationStatus: "invalid" | "pending";
      providerRequestId?: string;
      inputTokens?: number;
      outputTokens?: number;
      totalTokens?: number;
      estimatedProviderCost?: string;
    },
  ): Promise<boolean>;
  touchApiKey(keyId: Id, usedAt: Date): Promise<void>;
}
