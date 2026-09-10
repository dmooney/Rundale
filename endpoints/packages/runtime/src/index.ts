import type { EndpointVersionSnapshot, JsonSchema, ProviderId } from "@parish/domain";
import { compileSchema, formatValidationErrors, type SchemaIssue } from "@parish/schemas";

export type NormalizedErrorCode =
  | "AUTHENTICATION_FAILED"
  | "AUTHORIZATION_FAILED"
  | "ENDPOINT_NOT_FOUND"
  | "VERSION_NOT_FOUND"
  | "ENDPOINT_DISABLED"
  | "INVALID_INPUT"
  | "UNSUPPORTED_MEDIA_TYPE"
  | "REQUEST_TOO_LARGE"
  | "RATE_LIMITED"
  | "QUOTA_EXCEEDED"
  | "PROVIDER_UNAVAILABLE"
  | "PROVIDER_RATE_LIMITED"
  | "MODEL_ERROR"
  | "OUTPUT_VALIDATION_FAILED"
  | "REQUEST_TIMEOUT"
  | "INTERNAL_ERROR";

export class RuntimeError extends Error {
  attempts?: RuntimeAttempt[];

  constructor(
    public readonly code: NormalizedErrorCode,
    message: string,
    public readonly retryable = false,
    public readonly details?: SchemaIssue[],
    public readonly providerMetadata?: ProviderErrorMetadata,
  ) {
    super(message);
    this.name = "RuntimeError";
  }
}

export interface InvocationAttachment {
  field: string;
  mediaType: "image/jpeg" | "image/png" | "image/webp";
  bytes: Uint8Array;
}

export interface ProviderInvocation {
  model: string;
  instructions: string;
  values: Record<string, unknown>;
  attachments: InvocationAttachment[];
  outputSchema: JsonSchema;
  parameters: { temperature?: number; maxOutputTokens: number };
}

export interface ProviderExecutionContext {
  requestId: string;
  deadline: Date;
  signal: AbortSignal;
}

export interface NormalizedUsage {
  inputTokens?: number;
  outputTokens?: number;
  totalTokens?: number;
}

/**
 * Safe metadata retained when a provider returns a response that cannot be used.
 * It intentionally excludes raw provider output and request content.
 */
export interface ProviderErrorMetadata {
  usage: NormalizedUsage;
  providerRequestId?: string;
  finishReason?: string;
}

export interface ProviderResult {
  output: unknown;
  usage: NormalizedUsage;
  providerRequestId?: string;
  finishReason?: string;
}

export interface ModelProvider {
  readonly id: ProviderId;
  execute(
    invocation: ProviderInvocation,
    context: ProviderExecutionContext,
  ): Promise<ProviderResult>;
}

export interface ProviderRegistry {
  get(provider: ProviderId): ModelProvider;
}

export interface InvocationInput {
  values: Record<string, unknown>;
  attachments: InvocationAttachment[];
}

export interface InvocationContext {
  requestId: string;
  deadline: Date;
  signal: AbortSignal;
}

export interface RuntimeAttempt {
  attempt: number;
  provider: ProviderId;
  model: string;
  durationMs: number;
  status: "succeeded" | "failed";
  usage: NormalizedUsage;
  estimatedCostUsd: string;
  errorCode?: NormalizedErrorCode;
}

export interface InvocationResult {
  output: unknown;
  usage: NormalizedUsage;
  estimatedCostUsd: string;
  providerRequestId?: string;
  attempts: RuntimeAttempt[];
}

export interface CostCalculator {
  estimate(provider: ProviderId, model: string, usage: NormalizedUsage): string;
  /**
   * Returns whether pricing is configured for the provider/model pair.
   * Production pricing calculators should implement this so the runtime can
   * reject an unpriced live model before making a provider request.
   */
  isConfigured?(provider: ProviderId, model: string): boolean;
}

export interface RuntimeOptions {
  /** Current provider/model allowlist. Published versions do not snapshot it. */
  allowedModels?: ReadonlySet<string>;
}

export interface SemanticRuntime {
  invoke(
    version: EndpointVersionSnapshot,
    input: InvocationInput,
    context: InvocationContext,
  ): Promise<InvocationResult>;
}

export class DeterministicRuntime implements SemanticRuntime {
  constructor(
    private readonly providers: ProviderRegistry,
    private readonly costs: CostCalculator,
    private readonly options: RuntimeOptions = {},
  ) {}

  async invoke(
    version: EndpointVersionSnapshot,
    input: InvocationInput,
    context: InvocationContext,
  ): Promise<InvocationResult> {
    const validateInput = compileSchema(version.inputSchema);
    const normalizedInput = { ...input.values };
    for (const attachment of input.attachments)
      normalizedInput[attachment.field] = "[binary image]";
    if (!validateInput(normalizedInput)) {
      throw new RuntimeError(
        "INVALID_INPUT",
        "Input does not match the Endpoint contract.",
        false,
        formatValidationErrors(validateInput.errors),
      );
    }

    const providerId = version.providerConfig.provider;
    const model = version.providerConfig.model;
    const modelKey = `${providerId}/${model}`;
    if (this.options.allowedModels !== undefined && !this.options.allowedModels.has(modelKey)) {
      throw new RuntimeError("PROVIDER_UNAVAILABLE", "The configured model is unavailable.");
    }
    if (
      providerId !== "fake" &&
      (this.costs.isConfigured === undefined || !this.costs.isConfigured(providerId, model))
    ) {
      throw new RuntimeError("PROVIDER_UNAVAILABLE", "The configured model is unavailable.");
    }
    const provider = this.providers.get(providerId);
    const invocation: ProviderInvocation = {
      model: version.providerConfig.model,
      instructions: version.instructions,
      values: input.values,
      attachments: input.attachments,
      outputSchema: version.outputSchema,
      parameters: {
        ...(version.inferenceConfig.temperature === undefined
          ? {}
          : { temperature: version.inferenceConfig.temperature }),
        maxOutputTokens: version.inferenceConfig.maxOutputTokens,
      },
    };
    const attempts: RuntimeAttempt[] = [];
    const maximumAttempts = version.inferenceConfig.retryCount + 1;
    let latestError: RuntimeError | undefined;

    for (let attempt = 1; attempt <= maximumAttempts; attempt += 1) {
      const started = performance.now();
      try {
        const result = await provider.execute(invocation, context);
        const validateOutput = compileSchema(version.outputSchema);
        if (!validateOutput(result.output)) {
          throw new RuntimeError(
            "OUTPUT_VALIDATION_FAILED",
            "The Endpoint could not produce output matching its contract.",
            true,
            formatValidationErrors(validateOutput.errors),
            providerMetadata(result),
          );
        }
        const estimatedCostUsd = this.costs.estimate(provider.id, invocation.model, result.usage);
        attempts.push({
          attempt,
          provider: provider.id,
          model: invocation.model,
          durationMs: Math.round(performance.now() - started),
          status: "succeeded",
          usage: result.usage,
          estimatedCostUsd,
        });
        const usage = aggregateUsage(attempts);
        const totalEstimatedCostUsd = sumEstimatedCosts(
          attempts.map((currentAttempt) => currentAttempt.estimatedCostUsd),
        );
        return {
          output: result.output,
          usage,
          estimatedCostUsd: totalEstimatedCostUsd,
          ...(result.providerRequestId === undefined
            ? {}
            : { providerRequestId: result.providerRequestId }),
          attempts,
        };
      } catch (error) {
        latestError = normalizeProviderError(error);
        const usage = latestError.providerMetadata?.usage ?? {};
        const estimatedCostUsd = usageHasKnownTokens(usage)
          ? this.costs.estimate(provider.id, invocation.model, usage)
          : "0.000000";
        attempts.push({
          attempt,
          provider: provider.id,
          model: invocation.model,
          durationMs: Math.round(performance.now() - started),
          status: "failed",
          usage,
          estimatedCostUsd,
          errorCode: latestError.code,
        });
        if (!latestError.retryable || attempt === maximumAttempts) {
          latestError.attempts = attempts;
          throw latestError;
        }
      }
    }
    throw latestError ?? new RuntimeError("INTERNAL_ERROR", "Invocation failed.");
  }
}

function providerMetadata(result: ProviderResult): ProviderErrorMetadata {
  return {
    usage: result.usage,
    ...(result.providerRequestId === undefined
      ? {}
      : { providerRequestId: result.providerRequestId }),
    ...(result.finishReason === undefined ? {} : { finishReason: result.finishReason }),
  };
}

export function aggregateUsage(attempts: readonly RuntimeAttempt[]): NormalizedUsage {
  let inputTokens = 0;
  let outputTokens = 0;
  let totalTokens = 0;
  let hasInputTokens = false;
  let hasOutputTokens = false;
  let hasTotalTokens = false;

  for (const attempt of attempts) {
    if (attempt.usage.inputTokens !== undefined) {
      inputTokens += attempt.usage.inputTokens;
      hasInputTokens = true;
    }
    if (attempt.usage.outputTokens !== undefined) {
      outputTokens += attempt.usage.outputTokens;
      hasOutputTokens = true;
    }
    if (attempt.usage.totalTokens !== undefined) {
      totalTokens += attempt.usage.totalTokens;
      hasTotalTokens = true;
    }
  }

  return {
    ...(hasInputTokens ? { inputTokens } : {}),
    ...(hasOutputTokens ? { outputTokens } : {}),
    ...(hasTotalTokens ? { totalTokens } : {}),
  };
}

function usageHasKnownTokens(usage: NormalizedUsage): boolean {
  return (
    usage.inputTokens !== undefined ||
    usage.outputTokens !== undefined ||
    usage.totalTokens !== undefined
  );
}

export function sumEstimatedCosts(costs: readonly string[]): string {
  let totalMicros = 0n;
  for (const cost of costs) {
    const match = /^(-?)(\d+)(?:\.(\d+))?$/.exec(cost.trim());
    if (match === null) continue;
    const wholePart = match[2];
    if (wholePart === undefined) continue;
    const fractional = (match[3] ?? "").padEnd(6, "0").slice(0, 6);
    const micros = BigInt(wholePart) * 1_000_000n + BigInt(fractional);
    totalMicros += match[1] === "-" ? -micros : micros;
  }
  const sign = totalMicros < 0n ? "-" : "";
  const magnitude = totalMicros < 0n ? -totalMicros : totalMicros;
  const whole = magnitude / 1_000_000n;
  const fractional = (magnitude % 1_000_000n).toString().padStart(6, "0");
  return `${sign}${whole.toString()}.${fractional}`;
}

export function normalizeProviderError(error: unknown): RuntimeError {
  if (error instanceof RuntimeError) return error;
  if (error instanceof DOMException && error.name === "AbortError") {
    return new RuntimeError("REQUEST_TIMEOUT", "The provider request timed out.", true);
  }
  return new RuntimeError(
    "MODEL_ERROR",
    "The model provider could not complete the request.",
    false,
  );
}
