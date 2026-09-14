import { GoogleGenAI } from "@google/genai";
import {
  RuntimeError,
  type ModelProvider,
  type ProviderExecutionContext,
  type ProviderErrorMetadata,
  type ProviderInvocation,
  type ProviderResult,
  type ProviderStreamEvent,
} from "@parish/runtime";

export class GoogleProvider implements ModelProvider {
  readonly id = "google" as const;
  readonly supportsStreaming = true;

  constructor(
    private readonly client: GoogleGenAI,
    private readonly transport: "interactions" | "generate-content" = "interactions",
  ) {}

  static fromApiKey(apiKey: string): GoogleProvider {
    return new GoogleProvider(new GoogleGenAI({ apiKey }));
  }

  static fromVertexAI(project: string, location: string): GoogleProvider {
    return new GoogleProvider(
      new GoogleGenAI({
        vertexai: true,
        project,
        location,
      }),
      "generate-content",
    );
  }

  async execute(
    invocation: ProviderInvocation,
    context: ProviderExecutionContext,
  ): Promise<ProviderResult> {
    const input = [
      {
        type: "text" as const,
        text: `Endpoint input values:\n${JSON.stringify(invocation.values)}`,
      },
      ...invocation.attachments.map((attachment) => ({
        type: "image" as const,
        mime_type: attachment.mediaType,
        data: Buffer.from(attachment.bytes).toString("base64"),
      })),
    ];
    try {
      if (this.transport === "generate-content") {
        const response = await this.client.models.generateContent({
          model: invocation.model,
          contents: [
            {
              role: "user",
              parts: [
                {
                  text: `Endpoint input values:\n${JSON.stringify(invocation.values)}`,
                },
                ...invocation.attachments.map((attachment) => ({
                  inlineData: {
                    mimeType: attachment.mediaType,
                    data: Buffer.from(attachment.bytes).toString("base64"),
                  },
                })),
              ],
            },
          ],
          config: {
            systemInstruction: invocation.instructions,
            responseMimeType: "application/json",
            responseJsonSchema: invocation.outputSchema,
            maxOutputTokens: invocation.parameters.maxOutputTokens,
            httpOptions: { retryOptions: { attempts: 1 } },
            abortSignal: context.signal,
          },
        });
        const usage = normalizeGenerateContentUsage(response.usageMetadata);
        const finishReason = response.candidates?.[0]?.finishReason;
        const metadata = providerMetadata(usage, response.responseId, finishReason);
        const outputText: unknown = response.text;
        if (finishReason !== "STOP") {
          throw new RuntimeError(
            "MODEL_ERROR",
            "The model did not complete a structured response.",
            true,
            undefined,
            metadata,
          );
        }
        if (typeof outputText !== "string" || outputText.length === 0) {
          throw new RuntimeError(
            "MODEL_ERROR",
            "The model did not complete a structured response.",
            true,
            undefined,
            metadata,
          );
        }
        return {
          output: parseStructuredOutput(outputText, metadata),
          usage,
          ...(response.responseId === undefined ? {} : { providerRequestId: response.responseId }),
          finishReason: finishReason ?? "completed",
        };
      }
      const interaction = await this.client.interactions.create(
        {
          model: invocation.model,
          system_instruction: invocation.instructions,
          input,
          response_format: [
            {
              type: "text",
              mime_type: "application/json",
              schema: invocation.outputSchema,
            },
          ],
          generation_config: { max_output_tokens: invocation.parameters.maxOutputTokens },
          store: false,
        },
        { retries: { strategy: "none" }, fetchOptions: { signal: context.signal } },
      );
      const usage = normalizeInteractionUsage(interaction.usage);
      const metadata = providerMetadata(usage, interaction.id, interaction.status);
      const outputText: unknown = interaction.output_text;
      if (
        interaction.status !== "completed" ||
        typeof outputText !== "string" ||
        outputText.length === 0
      ) {
        throw new RuntimeError(
          "MODEL_ERROR",
          "The model did not complete a structured response.",
          true,
          undefined,
          metadata,
        );
      }
      return {
        output: parseStructuredOutput(outputText, metadata),
        usage,
        providerRequestId: interaction.id,
        finishReason: interaction.status,
      };
    } catch (error) {
      throw normalizeGoogleError(error);
    }
  }

  async *stream(
    invocation: ProviderInvocation,
    context: ProviderExecutionContext,
  ): AsyncIterable<ProviderStreamEvent> {
    const input = [
      {
        type: "text" as const,
        text: `Endpoint input values:\n${JSON.stringify(invocation.values)}`,
      },
      ...invocation.attachments.map((attachment) => ({
        type: "image" as const,
        mime_type: attachment.mediaType,
        data: Buffer.from(attachment.bytes).toString("base64"),
      })),
    ];
    let raw = "";
    let requestId: string | undefined;
    let usage: ProviderResult["usage"] = {};
    let finishReason: string | undefined;
    const maxRawBytes = 256 * 1024;
    try {
      if (this.transport === "generate-content") {
        const chunks = await this.client.models.generateContentStream({
          model: invocation.model,
          contents: [
            {
              role: "user",
              parts: [
                { text: `Endpoint input values:\n${JSON.stringify(invocation.values)}` },
                ...invocation.attachments.map((attachment) => ({
                  inlineData: {
                    mimeType: attachment.mediaType,
                    data: Buffer.from(attachment.bytes).toString("base64"),
                  },
                })),
              ],
            },
          ],
          config: {
            systemInstruction: invocation.instructions,
            responseMimeType: "application/json",
            responseJsonSchema: invocation.outputSchema,
            maxOutputTokens: invocation.parameters.maxOutputTokens,
            httpOptions: { retryOptions: { attempts: 1 } },
            abortSignal: context.signal,
          },
        });
        for await (const chunk of chunks) {
          if (typeof chunk.text === "string" && chunk.text.length > 0) {
            raw += chunk.text;
            if (new TextEncoder().encode(raw).length > maxRawBytes)
              throw new RuntimeError(
                "REQUEST_TOO_LARGE",
                "The streamed provider response is too large.",
              );
            yield { type: "delta", text: chunk.text };
          }
          usage = normalizeGenerateContentUsage(chunk.usageMetadata);
          finishReason = chunk.candidates?.[0]?.finishReason ?? finishReason;
          requestId = (chunk as { responseId?: string }).responseId ?? requestId;
        }
        if (finishReason !== "STOP")
          throw new RuntimeError(
            "MODEL_ERROR",
            "The model did not complete a structured response.",
            false,
            undefined,
            providerMetadata(usage, requestId, finishReason),
          );
      } else {
        const stream = await this.client.interactions.create(
          {
            model: invocation.model,
            system_instruction: invocation.instructions,
            input,
            response_format: [
              { type: "text", mime_type: "application/json", schema: invocation.outputSchema },
            ],
            generation_config: { max_output_tokens: invocation.parameters.maxOutputTokens },
            store: false,
            stream: true,
          },
          { retries: { strategy: "none" }, fetchOptions: { signal: context.signal } },
        );
        for await (const event of stream as AsyncIterable<Record<string, unknown>>) {
          const data = event;
          if (typeof data.interaction_id === "string") requestId = data.interaction_id;
          const delta = data.delta as { text?: string; type?: string } | undefined;
          if (delta?.type === "text" && typeof delta.text === "string") {
            raw += delta.text;
            if (new TextEncoder().encode(raw).length > maxRawBytes)
              throw new RuntimeError(
                "REQUEST_TOO_LARGE",
                "The streamed provider response is too large.",
              );
            yield { type: "delta", text: delta.text };
          }
          const interaction = data.interaction as
            { id?: string; status?: string; usage?: unknown } | undefined;
          if (interaction?.id) requestId = interaction.id;
          if (interaction?.usage)
            usage = normalizeInteractionUsage(
              interaction.usage as Parameters<typeof normalizeInteractionUsage>[0],
            );
          if (data.event_type === "interaction.completed") finishReason = "completed";
          if (
            data.event_type === "error" ||
            ["failed", "cancelled", "incomplete", "budget_exceeded"].includes(
              interaction?.status ?? "",
            ) ||
            (data.event_type === "interaction.status_update" &&
              ["failed", "cancelled", "incomplete", "budget_exceeded"].includes(
                String(data.status),
              ))
          )
            throw new RuntimeError(
              "MODEL_ERROR",
              "The model did not complete a structured response.",
              false,
              undefined,
              providerMetadata(usage, requestId, interaction?.status ?? String(data.status)),
            );
        }
        if (finishReason !== "completed")
          throw new RuntimeError(
            "MODEL_ERROR",
            "The model did not complete a structured response.",
            false,
            undefined,
            providerMetadata(usage, requestId, finishReason),
          );
      }
      const output = parseStructuredOutput(raw, providerMetadata(usage, requestId, finishReason));
      yield {
        type: "completed",
        result: {
          output,
          usage,
          ...(requestId === undefined ? {} : { providerRequestId: requestId }),
          finishReason,
        },
      };
    } catch (error) {
      if (requestId !== undefined && context.signal.aborted && this.transport === "interactions") {
        try {
          await this.client.interactions.cancel(requestId);
        } catch {
          /* preserve original cancellation */
        }
      }
      throw normalizeGoogleError(error);
    }
  }
}

function normalizeGenerateContentUsage(
  usage: GenerateContentUsageMetadata | null | undefined,
): ProviderResult["usage"] {
  const outputTokens = sumKnownTokens(usage?.candidatesTokenCount, usage?.thoughtsTokenCount);
  return {
    ...(usage?.promptTokenCount === undefined ? {} : { inputTokens: usage.promptTokenCount }),
    ...(outputTokens === undefined ? {} : { outputTokens }),
    ...(usage?.totalTokenCount === undefined ? {} : { totalTokens: usage.totalTokenCount }),
  };
}

interface GenerateContentUsageMetadata {
  promptTokenCount?: number | undefined;
  candidatesTokenCount?: number | undefined;
  thoughtsTokenCount?: number | undefined;
  totalTokenCount?: number | undefined;
}

function normalizeInteractionUsage(
  usage:
    | {
        total_input_tokens?: number | undefined;
        total_output_tokens?: number | undefined;
        total_thought_tokens?: number | undefined;
        total_tokens?: number | undefined;
      }
    | null
    | undefined,
): ProviderResult["usage"] {
  const outputTokens = sumKnownTokens(usage?.total_output_tokens, usage?.total_thought_tokens);
  return {
    ...(usage?.total_input_tokens === undefined ? {} : { inputTokens: usage.total_input_tokens }),
    ...(outputTokens === undefined ? {} : { outputTokens }),
    ...(usage?.total_tokens === undefined ? {} : { totalTokens: usage.total_tokens }),
  };
}

function sumKnownTokens(...tokens: Array<number | undefined>): number | undefined {
  const knownTokens = tokens.filter((token): token is number => token !== undefined);
  return knownTokens.length === 0 ? undefined : knownTokens.reduce((sum, token) => sum + token, 0);
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

function parseStructuredOutput(value: string, metadata?: ProviderErrorMetadata): unknown {
  try {
    return JSON.parse(value);
  } catch {
    throw new RuntimeError(
      "OUTPUT_VALIDATION_FAILED",
      "The Endpoint could not produce output matching its contract.",
      true,
      undefined,
      metadata,
    );
  }
}

export function normalizeGoogleError(error: unknown): RuntimeError {
  if (error instanceof RuntimeError) return error;
  const status =
    error !== null && typeof error === "object" && "status" in error
      ? Number((error as { status?: unknown }).status)
      : error !== null && typeof error === "object" && "statusCode" in error
        ? Number((error as { statusCode?: unknown }).statusCode)
        : undefined;
  if (status === 429) {
    return new RuntimeError("PROVIDER_RATE_LIMITED", "The model provider is rate limited.", true);
  }
  if (status === 408) {
    return new RuntimeError("REQUEST_TIMEOUT", "The model provider timed out.", true);
  }
  if (status !== undefined && status >= 500) {
    return new RuntimeError("PROVIDER_UNAVAILABLE", "The model provider is unavailable.", true);
  }
  const errorName =
    error !== null && typeof error === "object" && "name" in error
      ? String((error as { name?: unknown }).name)
      : undefined;
  if (
    (error instanceof DOMException &&
      (error.name === "AbortError" || error.name === "TimeoutError")) ||
    errorName === "APIUserAbortError" ||
    errorName === "APIConnectionTimeoutError" ||
    errorName === "RequestAbortedError" ||
    errorName === "RequestTimeoutError"
  ) {
    return new RuntimeError("REQUEST_TIMEOUT", "The model provider timed out.", true);
  }
  return new RuntimeError("MODEL_ERROR", "The model provider could not complete the request.");
}
