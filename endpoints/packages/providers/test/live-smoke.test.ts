import { describe, expect, it } from "vitest";
import type { ModelProvider, ProviderInvocation } from "@parish/runtime";
import { GoogleProvider, OpenAIProvider } from "../src/index.js";

const enabled = process.env.LIVE_PROVIDER_TESTS === "true";
const smokePng = Buffer.from(
  "iVBORw0KGgoAAAANSUhEUgAAAEAAAABAEAIAAAB1mzrKAAAAIGNIUk0AAHomAACAhAAA+gAAAIDoAAB1MAAA6mAAADqYAAAXcJy6UTwAAAAGYktHRP///////wlY99wAAAAHdElNRQfqCBkUCxH0ATMFAAAAJXRFWHRkYXRlOmNyZWF0ZQAyMDI2LTA4LTI1VDIwOjExOjE3KzAwOjAwH4YiZQAAACV0RVh0ZGF0ZTptb2RpZnkAMjAyNi0wOC0yNVQyMDoxMToxNyswMDowMG7bmtkAAAAodEVYdGRhdGU6dGltZXN0YW1wADIwMjYtMDgtMjVUMjA6MTE6MTcrMDA6MDA5zrsGAAABOUlEQVR42u3cy5EDIRAE0W6irNKJtWl0k1HgjOzRHjAiD+SzoCMmoPg006/X5/P9liCp1U9Nuox7pfd4fn90GfdKr37/Jl3GvVJ7PH4ATtoMQKWXGUBKbzOAlFpmACm9zQBSerUZAHIZCnMjBkuv8ZRTECa12ykIdDZiky7jXm7EYCkzAHX2AZMu417nQmbSZdwr5QhApXe/zQCOy1CYUxDMEIallxlAcgTAzACYnXEw+4JgKW/EUIYwzK4IWGoPMwBkVwTMwzjY6YowAzCnL2jSZdwrbV8QKrXG2ymI4wiAeRYEcxkKS+02A0COAJhvxGC+koR5HA1L7+FpKMj/BcHcCcP8WQfMrghY2jthlMtQmP+KgPlKEpbe420GcE57uhmA8Tga5k4Ydt6ITbqMe51lqBmAcRkK+wdNoYrA6JmQbwAAAABJRU5ErkJggg==",
  "base64",
);

async function smoke(provider: ModelProvider, model: string) {
  const invocation: ProviderInvocation = {
    model,
    instructions: "Return exactly one short schema-valid description of the supplied image.",
    values: {},
    attachments: [{ field: "image", mediaType: "image/png", bytes: smokePng }],
    outputSchema: {
      type: "object",
      properties: { result: { type: "string", maxLength: 80 } },
      required: ["result"],
      additionalProperties: false,
    },
    parameters: { maxOutputTokens: 64 },
  };
  const abort = new AbortController();
  const timeout = setTimeout(() => abort.abort(), 30_000);
  try {
    const result = await provider.execute(invocation, {
      requestId: "live_smoke",
      deadline: new Date(Date.now() + 30_000),
      signal: abort.signal,
    });
    expect(result.output).toMatchObject({ result: expect.any(String) });
    expect(result.providerRequestId).toBeTruthy();
  } finally {
    clearTimeout(timeout);
  }
}

describe.skipIf(
  !enabled ||
    process.env.OPENAI_API_KEY === undefined ||
    process.env.OPENAI_LIVE_TEST_MODEL === undefined,
)("OpenAI live smoke", () => {
  it("returns bounded structured image output", async () => {
    await smoke(
      OpenAIProvider.fromApiKey(process.env.OPENAI_API_KEY!),
      process.env.OPENAI_LIVE_TEST_MODEL!,
    );
  }, 35_000);
});

describe.skipIf(
  !enabled ||
    (process.env.GOOGLE_API_KEY === undefined &&
      process.env.GOOGLE_PROVIDER_AUTH !== "vertex-ai") ||
    (process.env.GOOGLE_PROVIDER_AUTH === "vertex-ai" &&
      process.env.GOOGLE_CLOUD_PROJECT === undefined) ||
    process.env.GOOGLE_LIVE_TEST_MODEL === undefined,
)("Google live smoke", () => {
  it("returns bounded structured image output", async () => {
    await smoke(
      process.env.GOOGLE_PROVIDER_AUTH === "vertex-ai"
        ? GoogleProvider.fromVertexAI(
            process.env.GOOGLE_CLOUD_PROJECT!,
            process.env.GOOGLE_CLOUD_LOCATION ?? "global",
          )
        : GoogleProvider.fromApiKey(process.env.GOOGLE_API_KEY!),
      process.env.GOOGLE_LIVE_TEST_MODEL!,
    );
  }, 35_000);
});
