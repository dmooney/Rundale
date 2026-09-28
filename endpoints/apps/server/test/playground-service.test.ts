import { describe, expect, it } from "vitest";
import { RuntimeError, type RuntimeAttempt, type SemanticRuntime } from "@parish/runtime";
import { FakeProvider, FixedPriceCostCalculator, StaticProviderRegistry } from "@parish/providers";
import { DeterministicRuntime } from "@parish/runtime";
import { PlaygroundService } from "../src/control/playground-service.js";
import type { CreatorPrincipal } from "../src/control/contracts.js";
import { MemoryControlRepository } from "./memory-control-repository.js";
import { MemoryInvocationRepository } from "./memory-invocation-repository.js";

const principal: CreatorPrincipal = { userId: "user_1", organizationId: "org_1", role: "owner" };
const definition = {
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
  instructions: "Return a typed result.",
  providerConfig: { provider: "fake" as const, model: "fake-v1" },
  inferenceConfig: { maxOutputTokens: 128, retryCount: 0 as const },
};

async function setup(
  runtime: SemanticRuntime = new DeterministicRuntime(
    new StaticProviderRegistry([new FakeProvider()]),
    new FixedPriceCostCalculator({}),
  ),
) {
  const control = new MemoryControlRepository();
  const created = await control.createEndpoint(principal, {
    name: "Generic Transform",
    slug: "generic-transform",
    description: "",
    definition,
  });
  const invocations = new MemoryInvocationRepository(
    {
      id: "key_unused",
      organizationId: principal.organizationId,
      keyDigest: "unused",
      scopes: ["invoke:endpoint:*"],
      status: "active",
      organizationStatus: "active",
      dailyInvocationQuota: 100,
      organizationInferenceEnabled: true,
    },
    {
      endpointId: "unused",
      endpointSlug: "unused",
      endpointStatus: "active",
      endpointInferenceEnabled: true,
    },
    [],
  );
  const playground = new PlaygroundService(control, invocations, runtime, {
    globalInferenceEnabled: true,
    timeoutMs: 1_000,
    requestsPerDay: 100,
  });
  return { control, created, invocations, playground };
}

function request(endpointId: string) {
  return {
    endpointId,
    requestId: `test_${endpointId}`,
    input: { values: { text: "hello" }, attachments: [] },
    inputBytes: 20,
  };
}

describe("PlaygroundService", () => {
  it.each([
    [
      "organization status",
      (endpoint: Awaited<ReturnType<MemoryControlRepository["getEndpoint"]>>) => {
        endpoint!.organizationStatus = "suspended";
      },
    ],
    [
      "organization inference switch",
      (endpoint: Awaited<ReturnType<MemoryControlRepository["getEndpoint"]>>) => {
        endpoint!.organizationInferenceEnabled = false;
      },
    ],
    [
      "Endpoint inference switch",
      (endpoint: Awaited<ReturnType<MemoryControlRepository["getEndpoint"]>>) => {
        endpoint!.inferenceEnabled = false;
      },
    ],
  ])("rejects draft tests when %s is disabled", async (_name, disable) => {
    const { control, created, invocations, playground } = await setup();
    disable(await control.getEndpoint(principal.organizationId, created.endpoint.id));
    await expect(
      playground.invokeDraft(principal, request(created.endpoint.id)),
    ).rejects.toMatchObject({
      code: "ENDPOINT_DISABLED",
    });
    expect(invocations.created).toBe(0);
  });

  it("returns an internal error when success finalization is lost", async () => {
    const { created, invocations, playground } = await setup();
    invocations.forceFinalizationFailure = true;
    await expect(
      playground.invokeDraft(principal, request(created.endpoint.id)),
    ).rejects.toMatchObject({
      code: "INTERNAL_ERROR",
    });
  });

  it("persists usage and cost from exhausted failed attempts", async () => {
    const attempts: RuntimeAttempt[] = [
      {
        attempt: 1,
        provider: "fake",
        model: "fake-v1",
        durationMs: 4,
        status: "failed",
        usage: { inputTokens: 10, outputTokens: 5, totalTokens: 15 },
        estimatedCostUsd: "0.000001",
        errorCode: "OUTPUT_VALIDATION_FAILED",
      },
      {
        attempt: 2,
        provider: "fake",
        model: "fake-v1",
        durationMs: 5,
        status: "failed",
        usage: { inputTokens: 20, outputTokens: 6, totalTokens: 26 },
        estimatedCostUsd: "0.000002",
        errorCode: "OUTPUT_VALIDATION_FAILED",
      },
    ];
    const runtime: SemanticRuntime = {
      invoke: async () => {
        const error = new RuntimeError("OUTPUT_VALIDATION_FAILED", "Output was invalid.");
        error.attempts = attempts;
        throw error;
      },
    };
    const { created, invocations, playground } = await setup(runtime);
    await expect(
      playground.invokeDraft(principal, request(created.endpoint.id)),
    ).rejects.toMatchObject({
      code: "OUTPUT_VALIDATION_FAILED",
    });
    expect(invocations.finalFailureResult).toMatchObject({
      inputTokens: 30,
      outputTokens: 11,
      totalTokens: 41,
      estimatedProviderCost: "0.000003",
    });
  });
});
