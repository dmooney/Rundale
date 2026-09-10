import { describe, expect, it } from "vitest";
import OpenAI from "openai";
import { GoogleGenAI } from "@google/genai";
import { RuntimeError, type ProviderInvocation } from "@parish/runtime";
import {
  GoogleProvider,
  OpenAIProvider,
  normalizeGoogleError,
  normalizeOpenAIError,
} from "../src/index.js";

const invocation: ProviderInvocation = {
  model: "configured-model",
  instructions: "Private creator instructions",
  values: { context: "synthetic" },
  attachments: [
    {
      field: "sourceImage",
      mediaType: "image/png",
      bytes: Uint8Array.from([0x89, 0x50, 0x4e, 0x47]),
    },
  ],
  outputSchema: {
    type: "object",
    properties: { result: { type: "string" } },
    required: ["result"],
    additionalProperties: false,
  },
  parameters: { maxOutputTokens: 128 },
};
const context = {
  requestId: "req_1",
  deadline: new Date(Date.now() + 1_000),
  signal: new AbortController().signal,
};

describe("live provider adapter contracts", () => {
  it("disables SDK retries so runtime retryCount owns request attempts", () => {
    const provider = OpenAIProvider.fromApiKey("synthetic-key");
    expect((provider as unknown as { client: OpenAI }).client.maxRetries).toBe(0);
  });

  it("bounds OpenAI SDK requests to one HTTP attempt per runtime attempt", async () => {
    let calls = 0;
    const client = new OpenAI({
      apiKey: "synthetic-key",
      baseURL: "https://example.invalid/v1",
      fetch: async () => {
        calls += 1;
        return new Response(JSON.stringify({ error: { message: "synthetic failure" } }), {
          status: 500,
          headers: { "content-type": "application/json" },
        });
      },
    });
    await expect(new OpenAIProvider(client).execute(invocation, context)).rejects.toMatchObject({
      code: "PROVIDER_UNAVAILABLE",
    });
    expect(calls).toBe(1);
  });

  it("bounds Google Interactions requests to one HTTP attempt per runtime attempt", async () => {
    let calls = 0;
    const originalFetch = globalThis.fetch;
    globalThis.fetch = async () => {
      calls += 1;
      return new Response(JSON.stringify({ error: { message: "synthetic failure" } }), {
        status: 500,
        headers: { "content-type": "application/json" },
      });
    };
    try {
      const client = new GoogleGenAI({
        apiKey: "synthetic-key",
        httpOptions: { baseUrl: "https://example.invalid", apiVersion: "v1beta" },
      });
      await expect(new GoogleProvider(client).execute(invocation, context)).rejects.toMatchObject({
        code: "PROVIDER_UNAVAILABLE",
      });
    } finally {
      globalThis.fetch = originalFetch;
    }
    expect(calls).toBe(1);
  });

  it("bounds Vertex generateContent requests to one HTTP attempt per runtime attempt", async () => {
    let calls = 0;
    const originalFetch = globalThis.fetch;
    globalThis.fetch = async () => {
      calls += 1;
      return new Response(JSON.stringify({ error: { message: "synthetic failure" } }), {
        status: 500,
        headers: { "content-type": "application/json" },
      });
    };
    try {
      const client = new GoogleGenAI({
        vertexai: true,
        apiKey: "synthetic-key",
        project: "synthetic-project",
        location: "us-central1",
        httpOptions: { baseUrl: "https://example.invalid" },
      });
      await expect(
        new GoogleProvider(client, "generate-content").execute(invocation, context),
      ).rejects.toMatchObject({ code: "PROVIDER_UNAVAILABLE" });
    } finally {
      globalThis.fetch = originalFetch;
    }
    expect(calls).toBe(1);
  });

  it("builds a stateless OpenAI Responses request and normalizes its result", async () => {
    let captured: unknown;
    let capturedOptions: unknown;
    const client = {
      responses: {
        create: async (parameters: unknown, options: unknown) => {
          captured = parameters;
          capturedOptions = options;
          return {
            id: "resp_1",
            status: "completed",
            output_text: '{"result":"ok"}',
            usage: { input_tokens: 12, output_tokens: 4, total_tokens: 16 },
            incomplete_details: null,
          };
        },
      },
    } as unknown as OpenAI;
    const result = await new OpenAIProvider(client).execute(invocation, context);
    expect(captured).toMatchObject({
      model: "configured-model",
      instructions: "Private creator instructions",
      store: false,
      text: { format: { type: "json_schema", strict: true } },
    });
    expect(capturedOptions).toEqual({ maxRetries: 0, signal: context.signal });
    expect(JSON.stringify(captured)).toContain("data:image/png;base64,");
    expect(result).toEqual({
      output: { result: "ok" },
      usage: { inputTokens: 12, outputTokens: 4, totalTokens: 16 },
      providerRequestId: "resp_1",
      finishReason: "completed",
    });
  });

  it("uses the Google Interactions API with inline image and JSON Schema", async () => {
    let captured: unknown;
    let capturedOptions: unknown;
    const client = {
      interactions: {
        create: async (parameters: unknown, options: unknown) => {
          captured = parameters;
          capturedOptions = options;
          return {
            id: "int_1",
            status: "completed",
            output_text: '{"result":"ok"}',
            usage: {
              total_input_tokens: 10,
              total_output_tokens: 5,
              total_thought_tokens: 2,
              total_tokens: 17,
            },
          };
        },
      },
    } as unknown as GoogleGenAI;
    const result = await new GoogleProvider(client).execute(invocation, context);
    expect(captured).toMatchObject({
      model: "configured-model",
      system_instruction: "Private creator instructions",
      store: false,
      response_format: [{ type: "text", mime_type: "application/json" }],
      generation_config: { max_output_tokens: 128 },
    });
    expect(JSON.stringify(captured)).toContain('"type":"image"');
    expect(capturedOptions).toEqual({
      retries: { strategy: "none" },
      fetchOptions: { signal: context.signal },
    });
    expect(result.usage).toEqual({ inputTokens: 10, outputTokens: 7, totalTokens: 17 });
    expect(result.providerRequestId).toBe("int_1");
  });

  it("uses stateless Vertex generateContent with inline image and JSON Schema", async () => {
    let captured: unknown;
    const client = {
      models: {
        generateContent: async (parameters: unknown) => {
          captured = parameters;
          return {
            text: '{"result":"ok"}',
            responseId: "vertex_1",
            candidates: [{ finishReason: "STOP" }],
            usageMetadata: {
              promptTokenCount: 11,
              candidatesTokenCount: 6,
              totalTokenCount: 17,
            },
          };
        },
      },
    } as unknown as GoogleGenAI;
    const result = await new GoogleProvider(client, "generate-content").execute(
      invocation,
      context,
    );
    expect(captured).toMatchObject({
      model: "configured-model",
      contents: [{ role: "user" }],
      config: {
        systemInstruction: "Private creator instructions",
        responseMimeType: "application/json",
        responseJsonSchema: invocation.outputSchema,
        maxOutputTokens: 128,
        httpOptions: { retryOptions: { attempts: 1 } },
      },
    });
    expect(JSON.stringify(captured)).toContain('"inlineData"');
    expect(result).toEqual({
      output: { result: "ok" },
      usage: { inputTokens: 11, outputTokens: 6, totalTokens: 17 },
      providerRequestId: "vertex_1",
      finishReason: "STOP",
    });
  });

  it("normalizes malformed provider JSON without returning raw content", async () => {
    const client = {
      responses: {
        create: async () => ({
          id: "resp_bad",
          status: "completed",
          output_text: "private malformed output",
          usage: null,
          incomplete_details: null,
        }),
      },
    } as unknown as OpenAI;
    await expect(new OpenAIProvider(client).execute(invocation, context)).rejects.toMatchObject({
      code: "OUTPUT_VALIDATION_FAILED",
      message: "The Endpoint could not produce output matching its contract.",
      providerMetadata: { usage: {} },
    });
  });

  it("includes Google thoughts in output usage and rejects non-success finishes", async () => {
    const client = {
      models: {
        generateContent: async () => ({
          text: '{"result":"should not be accepted"}',
          candidates: [{ finishReason: "MAX_TOKENS" }],
          usageMetadata: {
            promptTokenCount: 11,
            candidatesTokenCount: 6,
            thoughtsTokenCount: 4,
            totalTokenCount: 21,
          },
        }),
      },
    } as unknown as GoogleGenAI;
    await expect(
      new GoogleProvider(client, "generate-content").execute(invocation, context),
    ).rejects.toMatchObject({
      code: "MODEL_ERROR",
      providerMetadata: {
        usage: { inputTokens: 11, outputTokens: 10, totalTokens: 21 },
      },
    });
  });

  it("rejects Vertex responses with no finish reason", async () => {
    const client = {
      models: {
        generateContent: async () => ({
          text: '{"result":"should not be accepted"}',
          usageMetadata: {
            promptTokenCount: 11,
            candidatesTokenCount: 6,
            totalTokenCount: 17,
          },
        }),
      },
    } as unknown as GoogleGenAI;
    await expect(
      new GoogleProvider(client, "generate-content").execute(invocation, context),
    ).rejects.toMatchObject({
      code: "MODEL_ERROR",
      providerMetadata: {
        usage: { inputTokens: 11, outputTokens: 6, totalTokens: 17 },
      },
    });
  });

  it("normalizes SDK timeout and abort errors without exposing provider details", () => {
    const openAiAbort = normalizeOpenAIError(new OpenAI.APIUserAbortError());
    expect(openAiAbort).toMatchObject({ code: "REQUEST_TIMEOUT", retryable: true });
    expect(normalizeGoogleError({ name: "APIUserAbortError", message: "private details" })).toEqual(
      expect.objectContaining({
        code: "REQUEST_TIMEOUT",
        message: "The model provider timed out.",
      }),
    );
    expect(normalizeGoogleError({ name: "APIConnectionTimeoutError" })).toBeInstanceOf(
      RuntimeError,
    );
  });
});
