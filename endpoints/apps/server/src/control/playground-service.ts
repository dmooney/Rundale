import { definitionContentHash, type EndpointVersionSnapshot } from "@parish/domain";
import type { InvocationInput, SemanticRuntime } from "@parish/runtime";
import { RuntimeError } from "@parish/runtime";
import {
  InvocationQuotaExceededError,
  type InvocationRepository,
} from "../invocation/contracts.js";
import { failureAccounting } from "../invocation/accounting.js";
import type { ControlRepository, CreatorPrincipal } from "./contracts.js";

export interface PlaygroundRequest {
  requestId: string;
  endpointId: string;
  input: InvocationInput;
  inputBytes: number;
}

export class PlaygroundService {
  constructor(
    private readonly controlRepository: ControlRepository,
    private readonly invocationRepository: InvocationRepository,
    private readonly runtime: SemanticRuntime,
    private readonly options: {
      globalInferenceEnabled: boolean;
      timeoutMs: number;
      requestsPerDay: number;
    },
  ) {}

  async invokeDraft(principal: CreatorPrincipal, request: PlaygroundRequest): Promise<unknown> {
    const [endpoint, draft] = await Promise.all([
      this.controlRepository.getEndpoint(principal.organizationId, request.endpointId),
      this.controlRepository.getDraft(principal.organizationId, request.endpointId),
    ]);
    if (endpoint === null || draft === null) {
      throw new RuntimeError("ENDPOINT_NOT_FOUND", "The Endpoint draft was not found.");
    }
    if (
      !this.options.globalInferenceEnabled ||
      endpoint.status !== "active" ||
      endpoint.inferenceEnabled === false ||
      endpoint.organizationStatus !== "active" ||
      !endpoint.organizationInferenceEnabled ||
      !(await this.invocationRepository.isInferenceEnabled(
        draft.providerConfig.provider,
        draft.providerConfig.model,
      ))
    ) {
      throw new RuntimeError("ENDPOINT_DISABLED", "Inference is currently disabled.");
    }
    const version: EndpointVersionSnapshot = {
      id: draft.id,
      endpointId: endpoint.id,
      organizationId: principal.organizationId,
      version: 0,
      contentHash: definitionContentHash(draft),
      inputSchema: draft.inputSchema,
      outputSchema: draft.outputSchema,
      instructions: draft.instructions,
      providerConfig: draft.providerConfig,
      inferenceConfig: draft.inferenceConfig,
      publishedBy: principal.userId,
      publishedAt: draft.updatedAt,
    };
    let invocationId: string;
    try {
      invocationId = await this.invocationRepository.createInvocation(
        {
          requestId: request.requestId,
          callerOrganizationId: principal.organizationId,
          endpointId: endpoint.id,
          endpointVersionId: null,
          endpointDraftId: draft.id,
          apiKeyId: null,
          isTest: true,
          inputBytes: request.inputBytes,
          provider: draft.providerConfig.provider,
          model: draft.providerConfig.model,
        },
        this.options.requestsPerDay,
      );
    } catch (error) {
      if (error instanceof InvocationQuotaExceededError) {
        throw new RuntimeError("QUOTA_EXCEEDED", error.message);
      }
      throw error;
    }
    const started = performance.now();
    const abort = new AbortController();
    const timeout = setTimeout(() => abort.abort(), this.options.timeoutMs);
    try {
      const result = await this.runtime.invoke(version, request.input, {
        requestId: request.requestId,
        deadline: new Date(Date.now() + this.options.timeoutMs),
        signal: abort.signal,
      });
      await this.invocationRepository.recordAttempts(invocationId, result.attempts);
      const finalized = await this.invocationRepository.finalizeSuccess(invocationId, {
        durationMs: Math.round(performance.now() - started),
        outputBytes: Buffer.byteLength(JSON.stringify(result.output)),
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
      return result.output;
    } catch (error) {
      const normalized =
        error instanceof RuntimeError
          ? error
          : new RuntimeError("INTERNAL_ERROR", "Draft test failed.");
      if (normalized.attempts !== undefined) {
        await this.invocationRepository.recordAttempts(invocationId, normalized.attempts);
      }
      const finalized = await this.invocationRepository.finalizeFailure(invocationId, {
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
