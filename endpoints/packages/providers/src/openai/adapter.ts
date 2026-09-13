import OpenAI from "openai";
import {
  RuntimeError,
  type ModelProvider,
  type ProviderExecutionContext,
  type ProviderErrorMetadata,
  type ProviderInvocation,
  type ProviderResult,
} from "@parish/runtime";

export class OpenAIProvider implements ModelProvider {
  readonly id = "openai" as const;
  readonly supportsStreaming = false;

  constructor(private readonly client: OpenAI) {}

  static fromApiKey(apiKey: string): OpenAIProvider {
    return new OpenAIProvider(new OpenAI({ apiKey, maxRetries: 0 }));
  }

  async execute(
    invocation: ProviderInvocation,
    context: ProviderExecutionContext,
  ): Promise<ProviderResult> {
    const content: OpenAI.Responses.ResponseInputContent[] = [
      {
        type: "input_text",
        text: `Endpoint input values:\n${JSON.stringify(invocation.values)}`,
      },
      ...invocation.attachments.map((attachment): OpenAI.Responses.ResponseInputImage => ({
        type: "input_image",
        detail: "auto",
        image_url: `data:${attachment.mediaType};base64,${Buffer.from(attachment.bytes).toString("base64")}`,
      })),
    ];
    try {
      const response = await this.client.responses.create(
        {
          model: invocation.model,
          instructions: invocation.instructions,
          input: [{ role: "user", content }],
          text: {
            format: {
              type: "json_schema",
              name: "endpoint_output",
              schema: invocation.outputSchema,
              strict: true,
            },
          },
          max_output_tokens: invocation.parameters.maxOutputTokens,
          ...(invocation.parameters.temperature === undefined
            ? {}
            : { temperature: invocation.parameters.temperature }),
          store: false,
        },
        { maxRetries: 0, signal: context.signal },
      );
      const usage = normalizeOpenAIUsage(response.usage);
      const finishReason = response.incomplete_details?.reason ?? response.status;
      const metadata = providerMetadata(usage, response.id, finishReason);
      if (
        response.status !== "completed" ||
        typeof response.output_text !== "string" ||
        response.output_text.length === 0
      ) {
        throw new RuntimeError(
          "MODEL_ERROR",
          "The model did not complete a structured response.",
          true,
          undefined,
          metadata,
        );
      }
      let output: unknown;
      try {
        output = JSON.parse(response.output_text);
      } catch {
        throw new RuntimeError(
          "OUTPUT_VALIDATION_FAILED",
          "The Endpoint could not produce output matching its contract.",
          true,
          undefined,
          metadata,
        );
      }
      return {
        output,
        usage,
        providerRequestId: response.id,
        finishReason: response.incomplete_details?.reason ?? "completed",
      };
    } catch (error) {
      throw normalizeOpenAIError(error);
    }
  }

  async *stream(
    _invocation: ProviderInvocation,
    _context: ProviderExecutionContext,
  ): AsyncIterable<never> {
    void _invocation;
    void _context;
    yield* [];
    throw new RuntimeError("MODEL_ERROR", "OpenAI streaming is not supported.");
  }
}

function normalizeOpenAIUsage(
  usage: OpenAI.Responses.Response["usage"] | null | undefined,
): ProviderResult["usage"] {
  return {
    ...(usage?.input_tokens === undefined ? {} : { inputTokens: usage.input_tokens }),
    ...(usage?.output_tokens === undefined ? {} : { outputTokens: usage.output_tokens }),
    ...(usage?.total_tokens === undefined ? {} : { totalTokens: usage.total_tokens }),
  };
}

function providerMetadata(
  usage: ProviderResult["usage"],
  providerRequestId: string | undefined,
  finishReason: string | undefined,
): ProviderErrorMetadata {
  return {
    usage,
    ...(providerRequestId === undefined ? {} : { providerRequestId }),
    ...(finishReason === undefined ? {} : { finishReason }),
  };
}

export function normalizeOpenAIError(error: unknown): RuntimeError {
  if (error instanceof RuntimeError) return error;
  if (
    error instanceof OpenAI.APIUserAbortError ||
    error instanceof OpenAI.APIConnectionTimeoutError
  ) {
    return new RuntimeError("REQUEST_TIMEOUT", "The model provider timed out.", true);
  }
  if (error instanceof OpenAI.APIError) {
    if (error.status === 429) {
      return new RuntimeError("PROVIDER_RATE_LIMITED", "The model provider is rate limited.", true);
    }
    if (error.status !== undefined && error.status >= 500) {
      return new RuntimeError("PROVIDER_UNAVAILABLE", "The model provider is unavailable.", true);
    }
    if (error.status === 408) {
      return new RuntimeError("REQUEST_TIMEOUT", "The model provider timed out.", true);
    }
  }
  if (
    error instanceof DOMException &&
    (error.name === "AbortError" || error.name === "TimeoutError")
  ) {
    return new RuntimeError("REQUEST_TIMEOUT", "The model provider timed out.", true);
  }
  return new RuntimeError("MODEL_ERROR", "The model provider could not complete the request.");
}
