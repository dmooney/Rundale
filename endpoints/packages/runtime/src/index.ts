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
  | "REQUEST_CANCELLED"
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

export interface ProviderStreamDelta {
  type: "delta";
  text: string;
}

export interface ProviderStreamCompleted {
  type: "completed";
  result: ProviderResult;
}

export type ProviderStreamEvent = ProviderStreamDelta | ProviderStreamCompleted;

export interface ModelProvider {
  readonly id: ProviderId;
  readonly supportsStreaming?: boolean;
  execute(
    invocation: ProviderInvocation,
    context: ProviderExecutionContext,
  ): Promise<ProviderResult>;
  stream?(
    invocation: ProviderInvocation,
    context: ProviderExecutionContext,
  ): AsyncIterable<ProviderStreamEvent>;
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

export interface InvocationStreamEvent {
  type: "delta";
  content: string;
}

export interface InvocationStreamResult {
  type: "completed";
  result: InvocationResult;
}

export type InvocationStreamEventOrResult = InvocationStreamEvent | InvocationStreamResult;

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
  /** Side-effect-free capability/configuration check for streaming. */
  supportsStreaming(version: EndpointVersionSnapshot): boolean;
  invoke(
    version: EndpointVersionSnapshot,
    input: InvocationInput,
    context: InvocationContext,
  ): Promise<InvocationResult>;
  stream(
    version: EndpointVersionSnapshot,
    input: InvocationInput,
    context: InvocationContext,
  ): AsyncIterable<InvocationStreamEventOrResult>;
}

export class DeterministicRuntime implements SemanticRuntime {
  constructor(
    private readonly providers: ProviderRegistry,
    private readonly costs: CostCalculator,
    private readonly options: RuntimeOptions = {},
  ) {}

  supportsStreaming(version: EndpointVersionSnapshot): boolean {
    const projection = version.inferenceConfig.streaming;
    if (projection?.version !== 1 || projection.textField.length === 0) return false;
    try {
      const provider = this.providers.get(version.providerConfig.provider);
      return provider.supportsStreaming === true && provider.stream !== undefined;
    } catch {
      return false;
    }
  }

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

  async *stream(
    version: EndpointVersionSnapshot,
    input: InvocationInput,
    context: InvocationContext,
  ): AsyncIterable<InvocationStreamEventOrResult> {
    const projection = version.inferenceConfig.streaming;
    if (projection === undefined || projection.version !== 1 || projection.textField.length === 0) {
      throw new RuntimeError("MODEL_ERROR", "Streaming is not configured for this endpoint.");
    }
    const inputValidation = compileSchema(version.inputSchema);
    const normalizedInput = { ...input.values };
    for (const attachment of input.attachments)
      normalizedInput[attachment.field] = "[binary image]";
    if (!inputValidation(normalizedInput)) {
      throw new RuntimeError(
        "INVALID_INPUT",
        "Input does not match the Endpoint contract.",
        false,
        formatValidationErrors(inputValidation.errors),
      );
    }
    const providerId = version.providerConfig.provider;
    const model = version.providerConfig.model;
    if (
      this.options.allowedModels !== undefined &&
      !this.options.allowedModels.has(`${providerId}/${model}`)
    ) {
      throw new RuntimeError("PROVIDER_UNAVAILABLE", "The configured model is unavailable.");
    }
    if (
      providerId !== "fake" &&
      (this.costs.isConfigured === undefined || !this.costs.isConfigured(providerId, model))
    ) {
      throw new RuntimeError("PROVIDER_UNAVAILABLE", "The configured model is unavailable.");
    }
    const provider = this.providers.get(providerId);
    if (provider.supportsStreaming !== true || provider.stream === undefined)
      throw new RuntimeError("MODEL_ERROR", "The configured provider does not support streaming.");
    const invocation: ProviderInvocation = {
      model,
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
    const parser = new JsonStringProjection(projection.textField);
    const started = performance.now();
    const attempts: RuntimeAttempt[] = [];
    let completed: ProviderResult | undefined;
    let pendingDelta = "";
    try {
      for await (const event of provider.stream(invocation, context)) {
        if (event.type === "delta") {
          pendingDelta += parser.push(event.text).join("");
          // Coalesce very small provider fragments so bounded downstream
          // consumers cannot be exhausted by one frame per character.
          if (utf8Bytes(pendingDelta) >= 32) {
            yield { type: "delta", content: pendingDelta };
            pendingDelta = "";
          }
        } else completed = event.result;
      }
    } catch (error) {
      const normalized = normalizeProviderError(error);
      attempts.push({
        attempt: 1,
        provider: provider.id,
        model,
        durationMs: Math.round(performance.now() - started),
        status: "failed",
        usage: normalized.providerMetadata?.usage ?? {},
        estimatedCostUsd: "0.000000",
        errorCode: normalized.code,
      });
      normalized.attempts = attempts;
      throw normalized;
    }
    let estimatedCostUsd: string;
    try {
      if (completed === undefined)
        throw new RuntimeError("MODEL_ERROR", "The model did not complete a structured response.");
      parser.finish();
      const validateOutput = compileSchema(version.outputSchema);
      if (!validateOutput(completed.output))
        throw new RuntimeError(
          "OUTPUT_VALIDATION_FAILED",
          "The Endpoint could not produce output matching its contract.",
          false,
          formatValidationErrors(validateOutput.errors),
          providerMetadata(completed),
        );
      estimatedCostUsd = this.costs.estimate(provider.id, model, completed.usage);
    } catch (error) {
      const normalized = normalizeProviderError(error);
      attempts.push({
        attempt: 1,
        provider: provider.id,
        model,
        durationMs: Math.round(performance.now() - started),
        status: "failed",
        usage: normalized.providerMetadata?.usage ?? completed?.usage ?? {},
        estimatedCostUsd: "0.000000",
        errorCode: normalized.code,
      });
      normalized.attempts = attempts;
      throw normalized;
    }
    if (pendingDelta.length > 0) yield { type: "delta", content: pendingDelta };
    attempts.push({
      attempt: 1,
      provider: provider.id,
      model,
      durationMs: Math.round(performance.now() - started),
      status: "succeeded",
      usage: completed.usage,
      estimatedCostUsd,
    });
    const result: InvocationResult = {
      output: completed.output,
      usage: completed.usage,
      estimatedCostUsd,
      attempts,
      ...(completed.providerRequestId === undefined
        ? {}
        : { providerRequestId: completed.providerRequestId }),
    };
    yield { type: "completed", result };
  }
}

/** Incrementally decodes exactly one top-level JSON string property. */
export class JsonStringProjection {
  private raw = "";
  private cursor = 0;
  private state:
    | "root"
    | "member"
    | "key"
    | "colon"
    | "valueStart"
    | "target"
    | "targetEscape"
    | "targetUnicode"
    | "escape"
    | "unicode"
    | "skipString"
    | "skipEscape"
    | "skipUnicode"
    | "skipComposite"
    | "skipPrimitive"
    | "afterValue"
    | "done" = "root";
  private key = "";
  private seen = false;
  private unicode = "";
  private compositeDepth = 0;
  private skipReturn: "skipComposite" | "afterValue" = "afterValue";
  private outputBytes = 0;
  private pendingHigh: number | undefined;

  constructor(
    private readonly field: string,
    private readonly maxRawBytes = 256 * 1024,
    private readonly maxOutputBytes = 64 * 1024,
  ) {}

  push(fragment: string): string[] {
    this.raw += fragment;
    if (utf8Bytes(this.raw) > this.maxRawBytes)
      throw new RuntimeError("REQUEST_TOO_LARGE", "The streamed provider response is too large.");
    const output: string[] = [];
    while (this.cursor < this.raw.length) {
      const ch = this.raw[this.cursor++];
      if (ch === undefined) break;
      if (this.state === "root") {
        if (isSpace(ch)) continue;
        if (ch !== "{") this.invalid();
        this.state = "member";
      } else if (this.state === "member") {
        if (isSpace(ch)) continue;
        if (ch === "}") {
          this.state = "done";
          continue;
        }
        if (ch !== '"') this.invalid();
        this.key = "";
        this.state = "key";
      } else if (this.state === "key") {
        if (ch === "\\") this.state = "escape";
        else if (ch === '"') {
          this.state = "colon";
        } else if (ch < " ") this.invalid();
        else this.key += ch;
      } else if (this.state === "escape") {
        if (ch === "u") {
          this.unicode = "";
          this.state = "unicode";
        } else if ('\\"/bfnrt'.includes(ch)) {
          this.key += decodeEscape(ch);
          this.state = "key";
        } else this.invalid();
      } else if (this.state === "unicode") {
        if (!/[0-9a-f]/i.test(ch)) this.invalid();
        this.unicode += ch;
        if (this.unicode.length === 4) {
          this.key += String.fromCharCode(Number.parseInt(this.unicode, 16));
          this.unicode = "";
          this.state = "key";
        }
      } else if (this.state === "colon") {
        if (isSpace(ch)) continue;
        if (ch !== ":") this.invalid();
        if (this.key === this.field) {
          if (this.seen) this.invalid("Duplicate streamed field.");
          this.seen = true;
        }
        this.state = "valueStart";
      } else if (this.state === "valueStart") {
        if (isSpace(ch)) continue;
        if (this.key === this.field) {
          if (ch !== '"') this.invalid("The streamed field is not a string.");
          this.state = "target";
        } else if (ch === '"') {
          this.skipReturn = "afterValue";
          this.state = "skipString";
        } else if (ch === "{" || ch === "[") {
          this.compositeDepth = 1;
          this.state = "skipComposite";
        } else {
          this.state = "skipPrimitive";
        }
      } else if (this.state === "target") {
        if (this.pendingHigh !== undefined && ch !== "\\") {
          output.push(String.fromCharCode(this.pendingHigh));
          this.pendingHigh = undefined;
        }
        if (ch === "\\") this.state = "targetEscape";
        else if (ch === '"') this.state = "afterValue";
        else {
          this.emit(ch, output);
        }
      } else if (this.state === "targetEscape") {
        if (this.pendingHigh !== undefined && ch !== "u") {
          this.emit(String.fromCharCode(this.pendingHigh), output);
          this.pendingHigh = undefined;
        }
        if (ch === "u") {
          this.unicode = "";
          this.state = "targetUnicode";
        } else if ('\\"/bfnrt'.includes(ch)) {
          this.emit(decodeEscape(ch), output);
          this.state = "target";
        } else this.invalid();
      } else if (this.state === "targetUnicode") {
        if (!/[0-9a-f]/i.test(ch)) this.invalid();
        this.unicode += ch;
        if (this.unicode.length === 4) {
          const code = Number.parseInt(this.unicode, 16);
          if (code >= 0xd800 && code <= 0xdbff) {
            if (this.pendingHigh !== undefined)
              this.emit(String.fromCharCode(this.pendingHigh), output);
            this.pendingHigh = code;
          } else if (code >= 0xdc00 && code <= 0xdfff && this.pendingHigh !== undefined) {
            this.emit(String.fromCharCode(this.pendingHigh, code), output);
            this.pendingHigh = undefined;
          } else {
            if (this.pendingHigh !== undefined) {
              this.emit(String.fromCharCode(this.pendingHigh), output);
              this.pendingHigh = undefined;
            }
            this.emit(String.fromCharCode(code), output);
          }
          this.unicode = "";
          this.state = "target";
        }
      } else if (this.state === "skipString") {
        if (ch === "\\") this.state = "skipEscape";
        else if (ch === '"') this.state = this.skipReturn;
        else if (ch < " ") this.invalid();
      } else if (this.state === "skipEscape") {
        if (ch === "u") {
          this.unicode = "";
          this.state = "skipUnicode";
        } else if ('\\"/bfnrt'.includes(ch)) this.state = "skipString";
        else this.invalid();
      } else if (this.state === "skipUnicode") {
        if (!/[0-9a-f]/i.test(ch)) this.invalid();
        this.unicode += ch;
        if (this.unicode.length === 4) {
          this.unicode = "";
          this.state = "skipString";
        }
      } else if (this.state === "skipComposite") {
        if (ch === '"') {
          this.skipReturn = "skipComposite";
          this.state = "skipString";
        } else if (ch === "{" || ch === "[") this.compositeDepth++;
        else if (ch === "}" || ch === "]") {
          if (--this.compositeDepth === 0) this.state = "afterValue";
        }
      } else if (this.state === "skipPrimitive") {
        if (ch === ",") this.state = "member";
        else if (ch === "}") this.state = "done";
      } else if (this.state === "afterValue") {
        if (isSpace(ch)) continue;
        if (ch === ",") this.state = "member";
        else if (ch === "}") this.state = "done";
        else this.invalid();
      }
    }
    return output;
  }

  finish(): void {
    try {
      const parsed = JSON.parse(this.raw) as Record<string, unknown>;
      if (typeof parsed[this.field] !== "string" || !this.seen || this.state !== "done")
        throw new Error();
    } catch {
      throw new RuntimeError(
        "OUTPUT_VALIDATION_FAILED",
        "The Endpoint could not produce output matching its contract.",
      );
    }
  }

  private emit(value: string, output: string[]): void {
    const bytes = utf8Bytes(value);
    this.outputBytes += bytes;
    if (this.outputBytes > this.maxOutputBytes)
      throw new RuntimeError("REQUEST_TOO_LARGE", "The streamed output is too large.");
    output.push(value);
  }

  private invalid(message = "Invalid structured stream."): never {
    throw new RuntimeError("OUTPUT_VALIDATION_FAILED", message);
  }
}

function isSpace(ch: string): boolean {
  return ch === " " || ch === "\n" || ch === "\r" || ch === "\t";
}
function utf8Bytes(value: string): number {
  return new TextEncoder().encode(value).length;
}
function decodeEscape(ch: string): string {
  return ch === "n"
    ? "\n"
    : ch === "r"
      ? "\r"
      : ch === "t"
        ? "\t"
        : ch === "b"
          ? "\b"
          : ch === "f"
            ? "\f"
            : ch;
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
