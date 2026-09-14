import { describe, expect, it } from "vitest";
import {
  assertAliasRevision,
  assertDraftRevision,
  definitionContentHash,
  snapshotDraft,
  type EndpointDraft,
} from "../src/index.js";

const draft: EndpointDraft = {
  id: "draft_1",
  endpointId: "endpoint_1",
  revision: 3,
  inputSchema: { type: "object", properties: { value: { type: "string" } } },
  outputSchema: { type: "object", properties: { result: { type: "string" } } },
  instructions: "Return a result.",
  providerConfig: { provider: "fake", model: "fake-v1" },
  inferenceConfig: { maxOutputTokens: 128, retryCount: 0 },
  updatedBy: "user_1",
  updatedAt: new Date("2026-01-01T00:00:00Z"),
};

describe("domain invariants", () => {
  it("creates a stable hash independent of object key order", () => {
    const first = definitionContentHash(draft);
    const second = definitionContentHash({
      inferenceConfig: draft.inferenceConfig,
      providerConfig: draft.providerConfig,
      instructions: draft.instructions,
      outputSchema: draft.outputSchema,
      inputSchema: draft.inputSchema,
    });
    expect(first).toBe(second);
  });

  it("publishes an immutable snapshot", () => {
    const version = snapshotDraft(draft, "org_1", 1, "user_1", "version_1");
    expect(Object.isFrozen(version)).toBe(true);
    expect(version.contentHash).toMatch(/^sha256:[a-f0-9]{64}$/);
  });

  it("rejects stale draft and alias revisions", () => {
    expect(() => assertDraftRevision(4, 3)).toThrow("stale");
    expect(() => assertAliasRevision(2, 1)).toThrow("stale");
  });
});
