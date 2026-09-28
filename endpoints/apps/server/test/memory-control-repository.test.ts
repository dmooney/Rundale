import { describe, expect, it } from "vitest";
import { MemoryControlRepository } from "./memory-control-repository.js";
import type { InvocationSummary } from "../src/control/contracts.js";

describe("MemoryControlRepository API key tenancy", () => {
  it("lists and revokes keys only within their organization", async () => {
    const repository = new MemoryControlRepository();
    const first = { userId: "user_1", organizationId: "org_1", role: "owner" as const };
    const second = { userId: "user_2", organizationId: "org_2", role: "owner" as const };
    const keyOne = await repository.createApiKey(first, {
      name: "First",
      keyPrefix: "prefix_one",
      keyDigest: "digest_one",
      scopes: ["invoke:endpoint:first"],
    });
    const keyTwo = await repository.createApiKey(second, {
      name: "Second",
      keyPrefix: "prefix_two",
      keyDigest: "digest_two",
      scopes: ["invoke:endpoint:second"],
    });

    expect(await repository.listApiKeys(first.organizationId)).toEqual([keyOne]);
    expect(await repository.listApiKeys(second.organizationId)).toEqual([keyTwo]);
    expect(await repository.revokeApiKey(second, keyOne.id)).toBe(false);
    expect((await repository.listApiKeys(first.organizationId))[0]?.status).toBe("active");
    expect(await repository.revokeApiKey(first, keyOne.id)).toBe(true);
    expect((await repository.listApiKeys(first.organizationId))[0]?.status).toBe("revoked");
  });

  it("preserves the published version number and draft nullability", async () => {
    const repository = new MemoryControlRepository();
    const base: InvocationSummary = {
      id: "invocation_1",
      requestId: "request_1",
      endpointId: "endpoint_1",
      endpointVersionId: "version_1",
      endpointVersionNumber: 2,
      endpointDraftId: null,
      status: "succeeded",
      isTest: false,
      provider: "fake",
      model: "fake-v1",
      durationMs: 10,
      estimatedProviderCost: "0.000000",
      validationStatus: "valid",
      errorCode: null,
      startedAt: new Date(),
    };
    repository.addInvocation("org_1", base);
    repository.addInvocation("org_1", {
      ...base,
      id: "invocation_2",
      requestId: "request_2",
      endpointVersionId: null,
      endpointVersionNumber: null,
      endpointDraftId: "draft_1",
      isTest: true,
    });

    expect(await repository.listInvocations("org_1", 10)).toMatchObject([
      { endpointVersionId: "version_1", endpointVersionNumber: 2 },
      { endpointVersionId: null, endpointVersionNumber: null, endpointDraftId: "draft_1" },
    ]);
  });
});
