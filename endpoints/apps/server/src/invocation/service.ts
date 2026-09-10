import { parseApiKey, permitsEndpoint, verifyApiKey } from "@parish/auth";
import type { SemanticRuntime, InvocationInput } from "@parish/runtime";
import { RuntimeError } from "@parish/runtime";
import {
  InvocationQuotaExceededError,
  type InvocationRepository,
  type ResolvedEndpoint,
} from "./contracts.js";
import { failureAccounting } from "./accounting.js";
import { InMemoryRateGate } from "./rate-gate.js";

export interface InvocationServiceOptions {
  globalInferenceEnabled: boolean;
  requestsPerMinute: number;
  timeoutMs: number;
  requestsPerDay: number;
}

export interface InvocationRequest {
  authorization: string | undefined;
  organizationSlug: string;
  endpointSlug: string;
  version?: number;
  requestId: string;
  input: InvocationInput;
  inputBytes: number;
}

export class InvocationService {
  private readonly rateGate: InMemoryRateGate;

  constructor(
    private readonly repository: InvocationRepository,
    private readonly runtime: SemanticRuntime,
    private readonly options: InvocationServiceOptions,
  ) {
    this.rateGate = new InMemoryRateGate(options.requestsPerMinute);
  }

  async resolveAuthorized(request: Omit<InvocationRequest, "input" | "inputBytes">): Promise<{
    key: Awaited<ReturnType<InvocationRepository["findApiKeyByPrefix"]>> & {};
    endpoint: ResolvedEndpoint;
  }> {
    const bearer = request.authorization?.match(/^Bearer (.+)$/)?.[1];
    if (bearer === undefined) {
      throw new RuntimeError("AUTHENTICATION_FAILED", "A valid invocation API key is required.");
    }
    const parsed = parseApiKey(bearer);
    if (parsed === null) {
      throw new RuntimeError("AUTHENTICATION_FAILED", "A valid invocation API key is required.");
    }
    const key = await this.repository.findApiKeyByPrefix(parsed.prefix);
    if (key === null || key.status !== "active" || !verifyApiKey(bearer, key.keyDigest)) {
      throw new RuntimeError("AUTHENTICATION_FAILED", "A valid invocation API key is required.");
    }
    if (key.organizationStatus !== "active") {
      throw new RuntimeError("ENDPOINT_DISABLED", "Inference is disabled for this organization.");
    }
    const endpoint =
      request.version === undefined
        ? await this.repository.resolveProduction(request.organizationSlug, request.endpointSlug)
        : await this.repository.resolveVersion(
            request.organizationSlug,
            request.endpointSlug,
            request.version,
          );
    if (endpoint === null || endpoint.version.organizationId !== key.organizationId) {
      throw new RuntimeError(
        request.version === undefined ? "ENDPOINT_NOT_FOUND" : "VERSION_NOT_FOUND",
        "The requested Endpoint or version was not found.",
      );
    }
    if (!permitsEndpoint(key.scopes, endpoint.endpointSlug)) {
      throw new RuntimeError("AUTHORIZATION_FAILED", "The API key does not permit this Endpoint.");
    }
    if (
      !this.options.globalInferenceEnabled ||
      !key.organizationInferenceEnabled ||
      endpoint.endpointStatus !== "active" ||
      !endpoint.endpointInferenceEnabled ||
      !(await this.repository.isInferenceEnabled(
        endpoint.version.providerConfig.provider,
        endpoint.version.providerConfig.model,
      ))
    ) {
      throw new RuntimeError("ENDPOINT_DISABLED", "Inference is currently disabled.");
    }
    if (
      !this.rateGate.allow([
        `key:${key.id}`,
        `organization:${key.organizationId}`,
        `endpoint:${endpoint.endpointId}`,
      ])
    ) {
      throw new RuntimeError("RATE_LIMITED", "The invocation rate limit was exceeded.");
    }
    return { key, endpoint };
  }

  async invoke(
    request: InvocationRequest,
    authorized?: Awaited<ReturnType<InvocationService["resolveAuthorized"]>>,
  ): Promise<unknown> {
    const resolved =
      authorized ??
      (await this.resolveAuthorized({
        authorization: request.authorization,
        organizationSlug: request.organizationSlug,
        endpointSlug: request.endpointSlug,
        ...(request.version === undefined ? {} : { version: request.version }),
        requestId: request.requestId,
      }));
    const started = performance.now();
    let invocationId: string;
    try {
      invocationId = await this.repository.createInvocation(
        {
          requestId: request.requestId,
          callerOrganizationId: resolved.key.organizationId,
          endpointId: resolved.endpoint.endpointId,
          endpointVersionId: resolved.endpoint.version.id,
          endpointDraftId: null,
          apiKeyId: resolved.key.id,
          isTest: false,
          inputBytes: request.inputBytes,
          provider: resolved.endpoint.version.providerConfig.provider,
          model: resolved.endpoint.version.providerConfig.model,
        },
        Math.min(resolved.key.dailyInvocationQuota, this.options.requestsPerDay),
      );
    } catch (error) {
      if (error instanceof InvocationQuotaExceededError) {
        throw new RuntimeError("QUOTA_EXCEEDED", error.message);
      }
      throw error;
    }
    const abort = new AbortController();
    const timeout = setTimeout(() => abort.abort(), this.options.timeoutMs);
    try {
      const result = await this.runtime.invoke(resolved.endpoint.version, request.input, {
        requestId: request.requestId,
        deadline: new Date(Date.now() + this.options.timeoutMs),
        signal: abort.signal,
      });
      await this.repository.recordAttempts(invocationId, result.attempts);
      const serialized = JSON.stringify(result.output);
      const finalized = await this.repository.finalizeSuccess(invocationId, {
        durationMs: Math.round(performance.now() - started),
        outputBytes: Buffer.byteLength(serialized),
        ...(result.providerRequestId === undefined
          ? {}
          : { providerRequestId: result.providerRequestId }),
        ...(result.usage.inputTokens === undefined
          ? {}
          : { inputTokens: result.usage.inputTokens }),
        ...(result.usage.outputTokens === undefined
          ? {}
          : { outputTokens: result.usage.outputTokens }),
        ...(result.usage.totalTokens === undefined
          ? {}
          : { totalTokens: result.usage.totalTokens }),
        estimatedProviderCost: result.estimatedCostUsd,
      });
      if (!finalized) {
        throw new RuntimeError("INTERNAL_ERROR", "The invocation outcome could not be recorded.");
      }
      void this.repository.touchApiKey(resolved.key.id, new Date()).catch(() => undefined);
      return result.output;
    } catch (error) {
      const normalized =
        error instanceof RuntimeError
          ? error
          : new RuntimeError("INTERNAL_ERROR", "Invocation failed.");
      if (normalized.attempts !== undefined) {
        await this.repository.recordAttempts(invocationId, normalized.attempts);
      }
      const finalized = await this.repository.finalizeFailure(invocationId, {
        durationMs: Math.round(performance.now() - started),
        errorCode: normalized.code,
        validationStatus:
          normalized.code === "OUTPUT_VALIDATION_FAILED" || normalized.code === "INVALID_INPUT"
            ? "invalid"
            : "pending",
        ...failureAccounting(normalized),
      });
      if (!finalized) {
        throw new RuntimeError("INTERNAL_ERROR", "The invocation outcome could not be recorded.");
      }
      throw normalized;
    } finally {
      clearTimeout(timeout);
    }
  }
}
