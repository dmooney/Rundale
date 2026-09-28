import { parseApiKey, permitsEndpoint, verifyApiKey } from "@parish/auth";
import type { SemanticRuntime, InvocationInput } from "@parish/runtime";
import { RuntimeError } from "@parish/runtime";
import {
  InvocationQuotaExceededError,
  type InvocationRepository,
  type ResolvedEndpoint,
  type InvocationPrincipal,
} from "./contracts.js";
import type { FirebaseMobileAuthenticator, MobileCredentialRequest } from "../auth/mobile-auth.js";
import { failureAccounting } from "./accounting.js";
import {
  LocalInvocationCancellationCoordinator,
  streamCancellationKey,
  type InvocationCancellationCoordinator,
} from "./cancellation.js";
import { InMemoryRateGate } from "./rate-gate.js";

export interface InvocationServiceOptions {
  globalInferenceEnabled: boolean;
  requestsPerMinute: number;
  timeoutMs: number;
  requestsPerDay: number;
  mobileAuthenticator?: FirebaseMobileAuthenticator;
  cancellationCoordinator?: InvocationCancellationCoordinator;
}

export interface InvocationRequest {
  authorization: string | undefined;
  organizationSlug: string;
  endpointSlug: string;
  version?: number;
  requestId: string;
  input: InvocationInput;
  inputBytes: number;
  attemptId?: string;
  appCheck?: string | undefined;
}

export class InvocationService {
  private readonly rateGate: InMemoryRateGate;
  private readonly cancellationCoordinator: InvocationCancellationCoordinator;

  constructor(
    private readonly repository: InvocationRepository,
    private readonly runtime: SemanticRuntime,
    private readonly options: InvocationServiceOptions,
  ) {
    this.rateGate = new InMemoryRateGate(options.requestsPerMinute);
    this.cancellationCoordinator =
      options.cancellationCoordinator ?? new LocalInvocationCancellationCoordinator();
  }

  supportsStreaming(endpoint: ResolvedEndpoint): boolean {
    if (endpoint.version.inferenceConfig.streaming === undefined) return false;
    return this.runtime.supportsStreaming(endpoint.version);
  }

  async openStream(
    request: InvocationRequest,
    authorized: Awaited<ReturnType<InvocationService["resolveAuthorized"]>>,
    callerSignal?: AbortSignal,
  ): Promise<{
    invocationId: string;
    events: AsyncGenerator<
      { type: "delta"; content: string } | { type: "completed"; output: unknown }
    >;
    abort: () => void;
  }> {
    const started = performance.now();
    let invocationId: string;
    try {
      invocationId = await this.repository.createInvocation(
        {
          requestId:
            request.attemptId === undefined
              ? request.requestId
              : `${request.requestId}:${request.attemptId}`,
          callerOrganizationId: authorized.principal.organizationId,
          endpointId: authorized.endpoint.endpointId,
          endpointVersionId: authorized.endpoint.version.id,
          endpointDraftId: null,
          apiKeyId: authorized.principal.apiKeyId,
          isTest: false,
          inputBytes: request.inputBytes,
          provider: authorized.endpoint.version.providerConfig.provider,
          model: authorized.endpoint.version.providerConfig.model,
        },
        Math.min(authorized.principal.dailyInvocationQuota, this.options.requestsPerDay),
      );
    } catch (error) {
      if (error instanceof InvocationQuotaExceededError)
        throw new RuntimeError("QUOTA_EXCEEDED", error.message);
      throw error;
    }
    const providerAbort = new AbortController();
    let callerCancelled = false;
    const abort = () => {
      callerCancelled = true;
      providerAbort.abort();
    };
    const unregisterCancellation = this.cancellationCoordinator.register(
      streamCancellationKey(request.requestId, request.attemptId ?? ""),
      abort,
    );
    const timer = setTimeout(() => providerAbort.abort(), this.options.timeoutMs);
    const signal = providerAbort.signal;
    let settled = false;
    if (callerSignal !== undefined) {
      if (callerSignal.aborted) abort();
      else callerSignal.addEventListener("abort", abort, { once: true });
    }
    const events = (async function* (self: InvocationService) {
      try {
        for await (const event of self.runtime.stream(authorized.endpoint.version, request.input, {
          requestId: request.requestId,
          deadline: new Date(Date.now() + self.options.timeoutMs),
          signal,
        })) {
          if (event.type === "delta") yield { type: "delta" as const, content: event.content };
          else {
            await self.repository.recordAttempts(invocationId, event.result.attempts);
            const finalized = await self.repository.finalizeSuccess(invocationId, {
              durationMs: Math.round(performance.now() - started),
              outputBytes: Buffer.byteLength(JSON.stringify(event.result.output)),
              ...(event.result.providerRequestId === undefined
                ? {}
                : { providerRequestId: event.result.providerRequestId }),
              ...(event.result.usage.inputTokens === undefined
                ? {}
                : { inputTokens: event.result.usage.inputTokens }),
              ...(event.result.usage.outputTokens === undefined
                ? {}
                : { outputTokens: event.result.usage.outputTokens }),
              ...(event.result.usage.totalTokens === undefined
                ? {}
                : { totalTokens: event.result.usage.totalTokens }),
              estimatedProviderCost: event.result.estimatedCostUsd,
            });
            if (!finalized && (await self.repository.isCancellationRequested(invocationId))) {
              throw new RuntimeError("REQUEST_CANCELLED", "The caller cancelled the request.");
            }
            if (!finalized)
              throw new RuntimeError(
                "INTERNAL_ERROR",
                "The invocation outcome could not be recorded.",
              );
            settled = true;
            if (authorized.principal.apiKeyId !== null)
              void self.repository
                .touchApiKey(authorized.principal.apiKeyId, new Date())
                .catch(() => undefined);
            yield { type: "completed" as const, output: event.result.output };
          }
        }
      } catch (error) {
        let normalized =
          error instanceof RuntimeError
            ? error
            : new RuntimeError("INTERNAL_ERROR", "Invocation failed.");
        if (callerCancelled) {
          const cancellation = new RuntimeError(
            "REQUEST_CANCELLED",
            "The caller cancelled the request.",
          );
          if (normalized.attempts !== undefined) {
            cancellation.attempts = normalized.attempts.map((attempt) => ({
              ...attempt,
              errorCode: "REQUEST_CANCELLED",
            }));
          }
          normalized = cancellation;
        }
        if (normalized.attempts !== undefined)
          await self.repository.recordAttempts(invocationId, normalized.attempts);
        const finalized = await self.repository.finalizeFailure(invocationId, {
          durationMs: Math.round(performance.now() - started),
          errorCode: normalized.code,
          validationStatus:
            normalized.code === "OUTPUT_VALIDATION_FAILED" || normalized.code === "INVALID_INPUT"
              ? "invalid"
              : "pending",
          ...failureAccounting(normalized),
        });
        if (!finalized)
          throw new RuntimeError("INTERNAL_ERROR", "The invocation outcome could not be recorded.");
        settled = true;
        throw normalized;
      } finally {
        if (!settled) {
          callerCancelled = true;
          providerAbort.abort();
          await self.repository.finalizeFailure(invocationId, {
            durationMs: Math.round(performance.now() - started),
            errorCode: "REQUEST_CANCELLED",
            validationStatus: "pending",
            estimatedProviderCost: "0.000000",
          });
          settled = true;
        }
        clearTimeout(timer);
        unregisterCancellation();
        if (callerSignal !== undefined) callerSignal.removeEventListener("abort", abort);
      }
    })(this);
    return { invocationId, events, abort };
  }

  async cancelStream(request: Omit<InvocationRequest, "input" | "inputBytes">): Promise<void> {
    if (request.attemptId === undefined)
      throw new RuntimeError("INVALID_INPUT", "X-Attempt-Id is required for cancellation.");
    await this.resolveAuthorized(request, "cancel");
    await this.repository.requestCancellation(`${request.requestId}:${request.attemptId}`);
    await this.cancellationCoordinator.cancel(
      streamCancellationKey(request.requestId, request.attemptId),
    );
  }

  async resolveAuthorized(
    request: Omit<InvocationRequest, "input" | "inputBytes">,
    purpose: "invoke" | "cancel" = "invoke",
  ): Promise<{
    principal: InvocationPrincipal;
    endpoint: ResolvedEndpoint;
  }> {
    const mobile =
      this.options.mobileAuthenticator === undefined
        ? null
        : await this.options.mobileAuthenticator.authenticate({
            headers: {
              ...(request.authorization === undefined
                ? {}
                : { authorization: request.authorization }),
              ...(request.appCheck === undefined
                ? {}
                : { "x-firebase-appcheck": request.appCheck }),
            },
          } satisfies MobileCredentialRequest);
    if (mobile !== null) {
      const endpoint =
        request.version === undefined
          ? await this.repository.resolveProduction(request.organizationSlug, request.endpointSlug)
          : await this.repository.resolveVersion(
              request.organizationSlug,
              request.endpointSlug,
              request.version,
            );
      if (
        endpoint === null ||
        endpoint.version.organizationId !== mobile.organizationId ||
        mobile.organizationSlug !== request.organizationSlug ||
        endpoint.endpointSlug !== request.endpointSlug ||
        request.version === undefined ||
        !Object.hasOwn(mobile.allowedEndpointVersions, request.endpointSlug) ||
        !mobile.allowedEndpointVersions[request.endpointSlug]?.includes(request.version) ||
        (purpose === "invoke" &&
          (endpoint.organizationStatus !== "active" ||
            endpoint.organizationInferenceEnabled !== true))
      )
        throw new RuntimeError(
          "ENDPOINT_NOT_FOUND",
          "The requested Endpoint or version was not found.",
        );
      if (
        purpose === "invoke" &&
        (!this.options.globalInferenceEnabled ||
          endpoint.endpointStatus !== "active" ||
          !endpoint.endpointInferenceEnabled ||
          !(await this.repository.isInferenceEnabled(
            endpoint.version.providerConfig.provider,
            endpoint.version.providerConfig.model,
          )))
      )
        throw new RuntimeError("ENDPOINT_DISABLED", "Inference is currently disabled.");
      if (
        purpose === "invoke" &&
        !this.rateGate.allow([
          mobile.rateIdentity,
          `organization:${mobile.organizationId}`,
          `endpoint:${endpoint.endpointId}`,
        ])
      )
        throw new RuntimeError("RATE_LIMITED", "The invocation rate limit was exceeded.");
      return {
        principal: {
          kind: "mobile",
          organizationId: mobile.organizationId,
          rateIdentity: mobile.rateIdentity,
          dailyInvocationQuota: mobile.dailyInvocationQuota,
          apiKeyId: null,
        },
        endpoint,
      };
    }
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
    if (purpose === "invoke" && key.organizationStatus !== "active") {
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
      purpose === "invoke" &&
      (!this.options.globalInferenceEnabled ||
        !key.organizationInferenceEnabled ||
        endpoint.endpointStatus !== "active" ||
        !endpoint.endpointInferenceEnabled ||
        !(await this.repository.isInferenceEnabled(
          endpoint.version.providerConfig.provider,
          endpoint.version.providerConfig.model,
        )))
    ) {
      throw new RuntimeError("ENDPOINT_DISABLED", "Inference is currently disabled.");
    }
    if (
      purpose === "invoke" &&
      !this.rateGate.allow([
        `key:${key.id}`,
        `organization:${key.organizationId}`,
        `endpoint:${endpoint.endpointId}`,
      ])
    ) {
      throw new RuntimeError("RATE_LIMITED", "The invocation rate limit was exceeded.");
    }
    return {
      principal: {
        kind: "api-key",
        organizationId: key.organizationId,
        rateIdentity: `key:${key.id}`,
        dailyInvocationQuota: key.dailyInvocationQuota,
        apiKeyId: key.id,
      },
      endpoint,
    };
  }

  async invoke(
    request: InvocationRequest,
    authorized?: Awaited<ReturnType<InvocationService["resolveAuthorized"]>>,
  ): Promise<unknown> {
    const resolved =
      authorized ??
      (await this.resolveAuthorized({
        authorization: request.authorization,
        appCheck: request.appCheck,
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
          callerOrganizationId: resolved.principal.organizationId,
          endpointId: resolved.endpoint.endpointId,
          endpointVersionId: resolved.endpoint.version.id,
          endpointDraftId: null,
          apiKeyId: resolved.principal.apiKeyId,
          isTest: false,
          inputBytes: request.inputBytes,
          provider: resolved.endpoint.version.providerConfig.provider,
          model: resolved.endpoint.version.providerConfig.model,
        },
        Math.min(resolved.principal.dailyInvocationQuota, this.options.requestsPerDay),
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
      if (resolved.principal.apiKeyId !== null)
        void this.repository
          .touchApiKey(resolved.principal.apiKeyId, new Date())
          .catch(() => undefined);
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
