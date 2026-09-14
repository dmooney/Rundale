import { describe, expect, it } from "vitest";
import type { EndpointVersionSnapshot } from "@parish/domain";
import {
  DeterministicRuntime,
  type CostCalculator,
  type ModelProvider,
  type ProviderRegistry,
  type RuntimeError,
} from "../src/index.js";

const version: EndpointVersionSnapshot = {
  id: "version_1",
  endpointId: "endpoint_1",
  organizationId: "org_1",
  version: 1,
  contentHash: "sha256:test",
  inputSchema: {
    type: "object",
    properties: { text: { type: "string" } },
    required: ["text"],
    additionalProperties: false,
  },
  outputSchema: {
    type: "object",
    properties: { result: { type: "string" } },
    required: ["result"],
    additionalProperties: false,
  },
  instructions: "Transform text.",
  providerConfig: { provider: "fake", model: "fake-v1" },
  inferenceConfig: { maxOutputTokens: 128, retryCount: 1 },
  publishedBy: "user_1",
  publishedAt: new Date(),
};

const costs: CostCalculator = { estimate: () => "0.000001" };

function runtimeWith(
  provider: ModelProvider,
  options: { allowedModels?: ReadonlySet<string> } = {},
  costCalculator: CostCalculator = costs,
): DeterministicRuntime {
  const registry: ProviderRegistry = { get: () => provider };
  return new DeterministicRuntime(registry, costCalculator, options);
}

const context = {
  requestId: "req_1",
  deadline: new Date(Date.now() + 1_000),
  signal: new AbortController().signal,
};

describe("deterministic runtime", () => {
  it("validates both input and output", async () => {
    const provider: ModelProvider = {
      id: "fake",
      execute: async () => ({ output: { result: "ok" }, usage: { totalTokens: 3 } }),
    };
    await expect(
      runtimeWith(provider).invoke(version, { values: {}, attachments: [] }, context),
    ).rejects.toMatchObject({ code: "INVALID_INPUT" });
    await expect(
      runtimeWith(provider).invoke(
        version,
        { values: { text: "hello" }, attachments: [] },
        context,
      ),
    ).resolves.toMatchObject({ output: { result: "ok" } });
  });

  it("uses one bounded retry for invalid provider output", async () => {
    let calls = 0;
    const provider: ModelProvider = {
      id: "fake",
      execute: async () => {
        calls += 1;
        return {
          output: calls === 1 ? {} : { result: "repaired" },
          usage: {
            inputTokens: calls * 10,
            outputTokens: calls * 5,
            totalTokens: calls * 15,
          },
          providerRequestId: `provider_${calls}`,
        };
      },
    };
    const result = await runtimeWith(provider).invoke(
      version,
      { values: { text: "hello" }, attachments: [] },
      context,
    );
    expect(result.attempts).toHaveLength(2);
    expect(calls).toBe(2);
    expect(result.attempts[0]).toMatchObject({
      status: "failed",
      usage: { inputTokens: 10, outputTokens: 5, totalTokens: 15 },
      estimatedCostUsd: "0.000001",
    });
    expect(result.usage).toEqual({ inputTokens: 30, outputTokens: 15, totalTokens: 45 });
    expect(result.estimatedCostUsd).toBe("0.000002");
  });

  it("retains usage and cost for every failed retry", async () => {
    let calls = 0;
    const provider: ModelProvider = {
      id: "fake",
      execute: async () => {
        calls += 1;
        return {
          output: {},
          usage: { inputTokens: calls, outputTokens: 2, totalTokens: calls + 2 },
        };
      },
    };
    await expect(
      runtimeWith(provider).invoke(
        version,
        { values: { text: "hello" }, attachments: [] },
        context,
      ),
    ).rejects.toMatchObject({
      code: "OUTPUT_VALIDATION_FAILED",
      attempts: [
        {
          status: "failed",
          usage: { inputTokens: 1, outputTokens: 2, totalTokens: 3 },
          estimatedCostUsd: "0.000001",
        },
        {
          status: "failed",
          usage: { inputTokens: 2, outputTokens: 2, totalTokens: 4 },
          estimatedCostUsd: "0.000001",
        },
      ],
    });
  });

  it("does not leak raw provider failures", async () => {
    const provider: ModelProvider = {
      id: "fake",
      execute: async () => {
        throw new Error("secret raw failure");
      },
    };
    await expect(
      runtimeWith(provider).invoke(
        version,
        { values: { text: "hello" }, attachments: [] },
        context,
      ),
    ).rejects.toEqual(
      expect.objectContaining<Partial<RuntimeError>>({
        code: "MODEL_ERROR",
        message: "The model provider could not complete the request.",
        attempts: [expect.objectContaining({ usage: {}, estimatedCostUsd: "0.000000" })],
      }),
    );
  });

  it("rejects a model removed from the current allowlist before provider execution", async () => {
    let calls = 0;
    const provider: ModelProvider = {
      id: "fake",
      execute: async () => {
        calls += 1;
        return { output: { result: "unexpected" }, usage: {} };
      },
    };
    await expect(
      runtimeWith(provider, { allowedModels: new Set(["fake/other-model"]) }).invoke(
        version,
        { values: { text: "hello" }, attachments: [] },
        context,
      ),
    ).rejects.toMatchObject({ code: "PROVIDER_UNAVAILABLE" });
    expect(calls).toBe(0);
  });

  it("rejects an unpriced live model before provider execution", async () => {
    let calls = 0;
    const provider: ModelProvider = {
      id: "openai",
      execute: async () => {
        calls += 1;
        return { output: { result: "unexpected" }, usage: {} };
      },
    };
    const liveVersion = {
      ...version,
      providerConfig: { provider: "openai" as const, model: "gpt" },
    };
    const unpriced: CostCalculator = {
      estimate: () => "0.000000",
      isConfigured: () => false,
    };
    await expect(
      runtimeWith(provider, { allowedModels: new Set(["openai/gpt"]) }, unpriced).invoke(
        liveVersion,
        { values: { text: "hello" }, attachments: [] },
        context,
      ),
    ).rejects.toMatchObject({ code: "PROVIDER_UNAVAILABLE" });
    expect(calls).toBe(0);
  });
});
