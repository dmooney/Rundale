import { createHash } from "node:crypto";
import { File } from "node:buffer";
import { afterEach, describe, expect, it } from "vitest";
import type { FastifyInstance } from "fastify";
import type { EndpointVersionSnapshot } from "@parish/domain";
import type { ModelProvider } from "@parish/runtime";
import { DeterministicRuntime } from "@parish/runtime";
import { FixedPriceCostCalculator, StaticProviderRegistry } from "@parish/providers";
import { buildServer } from "../src/app.js";
import type { ServerConfig } from "../src/config.js";
import { normalizedRuntimeError, runtimeStatus } from "../src/invocation/routes.js";
import { InvocationService } from "../src/invocation/service.js";
import { MemoryInvocationRepository } from "./memory-invocation-repository.js";

const secret = "sfk_live_abcdefghijkl_abcdefghijklmnopqrstuvwxyzABCDEF";
const digest = createHash("sha256").update(secret).digest("hex");
const textInputSchema = {
  type: "object",
  properties: { text: { type: "string" } },
  required: ["text"],
  additionalProperties: false,
};
const outputSchema = {
  type: "object",
  properties: { result: { type: "string" } },
  required: ["result"],
  additionalProperties: false,
};
const baseVersion = {
  endpointId: "endpoint_1",
  organizationId: "org_1",
  contentHash: "sha256:test",
  inputSchema: textInputSchema,
  outputSchema,
  providerConfig: { provider: "fake" as const, model: "fake-v1" },
  inferenceConfig: { maxOutputTokens: 128, retryCount: 0 as const },
  publishedBy: "user_1",
  publishedAt: new Date(),
};
const versions: EndpointVersionSnapshot[] = [
  { ...baseVersion, id: "version_1", version: 1, instructions: "version one" },
  { ...baseVersion, id: "version_2", version: 2, instructions: "version two" },
];
const config: ServerConfig = {
  port: 3001,
  host: "127.0.0.1",
  databaseUrl: "postgres://unused",
  webOrigin: "http://localhost:3000",
  maxRequestBytes: 100_000,
  maxImageBytes: 50_000,
  maxImagePixels: 40_000_000,
  requestsPerMinute: 100,
  requestsPerDay: 200,
  globalInferenceEnabled: true,
  providerMode: "fake",
  authMode: "development",
  ownerFirebaseUid: "owner_external",
  allowedModels: new Set(["fake/fake-v1"]),
  providerTimeoutMs: 1_000,
  modelPrices: {},
  mobileAppBindings: [],
};

function repository() {
  return new MemoryInvocationRepository(
    {
      id: "key_1",
      organizationId: "org_1",
      keyDigest: digest,
      scopes: ["invoke:endpoint:generic-transform"],
      status: "active",
      organizationStatus: "active",
      dailyInvocationQuota: 100,
      organizationInferenceEnabled: true,
    },
    {
      endpointId: "endpoint_1",
      endpointSlug: "generic-transform",
      endpointStatus: "active",
      endpointInferenceEnabled: true,
      organizationStatus: "active",
      organizationInferenceEnabled: true,
    },
    structuredClone(versions),
  );
}

async function createServer(
  store: MemoryInvocationRepository,
  output: (instructions: string) => unknown = (instructions) => ({ result: instructions }),
) {
  const provider: ModelProvider = {
    id: "fake",
    execute: async (invocation) => ({
      output: output(invocation.instructions),
      usage: { inputTokens: 10, outputTokens: 5, totalTokens: 15 },
    }),
  };
  const service = new InvocationService(
    store,
    new DeterministicRuntime(
      new StaticProviderRegistry([provider]),
      new FixedPriceCostCalculator({}),
    ),
    {
      globalInferenceEnabled: true,
      requestsPerMinute: 100,
      requestsPerDay: 200,
      timeoutMs: 1_000,
    },
  );
  return buildServer(config, {
    health: { ready: async () => undefined },
    invocation: { service },
  });
}

const requiredImageInputSchema = {
  type: "object",
  properties: {
    image: {
      type: "string",
      contentMediaType: "image/*",
      "x-semantic-type": "image",
    },
  },
  required: ["image"],
  additionalProperties: false,
};

const tinyPng = Uint8Array.from([
  0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 13, 0x49, 0x48, 0x44, 0x52, 0, 0, 0, 1,
  0, 0, 0, 1,
]);

function requiredImageContract(store: MemoryInvocationRepository): void {
  store.versions[0] = { ...store.versions[0]!, inputSchema: requiredImageInputSchema };
}

function imageFile(filename: string): File {
  return new File([tinyPng], filename, { type: "image/png" });
}

async function encodeMultipart(form: FormData): Promise<{
  "content-type": string;
  payload: Buffer;
}> {
  const encoded = new Request("http://localhost", { method: "POST", body: form });
  return {
    "content-type": encoded.headers.get("content-type")!,
    payload: Buffer.from(await encoded.arrayBuffer()),
  };
}

let server: FastifyInstance | undefined;
afterEach(async () => server?.close());

describe("invocation data plane", () => {
  it("authenticates, resolves production and pinned versions, validates, and records", async () => {
    const store = repository();
    store.productionVersion = 2;
    server = await createServer(store);
    const production = await server.inject({
      method: "POST",
      url: "/v1/endpoints/acme/generic-transform",
      headers: { authorization: `Bearer ${secret}` },
      payload: { input: { text: "hello" } },
    });
    expect(production.json()).toEqual({ result: "version two" });
    const pinned = await server.inject({
      method: "POST",
      url: "/v1/endpoints/acme/generic-transform/versions/1",
      headers: { authorization: `Bearer ${secret}` },
      payload: { input: { text: "hello" } },
    });
    expect(pinned.json()).toEqual({ result: "version one" });
    expect(store.finalStatus).toBe("succeeded");
    expect(store.attempts).toHaveLength(2);
  });

  it("rejects invalid credentials before creating an invocation", async () => {
    const store = repository();
    server = await createServer(store);
    const response = await server.inject({
      method: "POST",
      url: "/v1/endpoints/acme/generic-transform",
      payload: { input: { text: "hello" } },
    });
    expect(response.statusCode).toBe(401);
    expect(store.created).toBe(0);
  });

  it("enforces quotas and kill switches before inference", async () => {
    const store = repository();
    store.used = 100;
    server = await createServer(store);
    const quota = await server.inject({
      method: "POST",
      url: "/v1/endpoints/acme/generic-transform",
      headers: { authorization: `Bearer ${secret}` },
      payload: { input: { text: "hello" } },
    });
    expect(quota.json().error.code).toBe("QUOTA_EXCEEDED");
    expect(store.created).toBe(0);
    await server.close();
    const disabledStore = repository();
    disabledStore.inferenceEnabled = false;
    server = await createServer(disabledStore);
    const disabled = await server.inject({
      method: "POST",
      url: "/v1/endpoints/acme/generic-transform",
      headers: { authorization: `Bearer ${secret}` },
      payload: { input: { text: "hello" } },
    });
    expect(disabled.json().error.code).toBe("ENDPOINT_DISABLED");
    expect(disabledStore.created).toBe(0);
  });

  it("hides an Endpoint owned by another organization", async () => {
    const store = repository();
    store.key = { ...store.key, organizationId: "org_2" };
    server = await createServer(store);
    const response = await server.inject({
      method: "POST",
      url: "/v1/endpoints/acme/generic-transform",
      headers: { authorization: `Bearer ${secret}` },
      payload: { input: { text: "hello" } },
    });
    expect(response.statusCode).toBe(404);
    expect(store.created).toBe(0);
  });

  it("never returns provider output that violates the contract", async () => {
    const store = repository();
    server = await createServer(store, () => ({ invalid: true }));
    const response = await server.inject({
      method: "POST",
      url: "/v1/endpoints/acme/generic-transform",
      headers: { authorization: `Bearer ${secret}` },
      payload: { input: { text: "hello" } },
    });
    expect(response.statusCode).toBe(502);
    expect(response.json().error.code).toBe("OUTPUT_VALIDATION_FAILED");
    expect(store.finalStatus).toBe("failed");
    expect(store.finalFailureResult).toMatchObject({
      inputTokens: 10,
      outputTokens: 5,
      totalTokens: 15,
      estimatedProviderCost: "0.000000",
    });
  });

  it("does not return a successful result after finalization is already terminal", async () => {
    const store = repository();
    store.forceFinalizationFailure = true;
    server = await createServer(store);
    const response = await server.inject({
      method: "POST",
      url: "/v1/endpoints/acme/generic-transform",
      headers: { authorization: `Bearer ${secret}` },
      payload: { input: { text: "hello" } },
    });
    expect(response.statusCode).toBe(500);
    expect(response.json().error.code).toBe("INTERNAL_ERROR");
  });

  it("accepts one signature-validated multipart image without retaining it", async () => {
    const store = repository();
    store.versions[0] = {
      ...store.versions[0]!,
      inputSchema: {
        type: "object",
        properties: {
          context: { type: "string" },
          sourceImage: {
            type: "string",
            contentMediaType: "image/*",
            "x-semantic-type": "image",
          },
        },
        required: ["sourceImage"],
        additionalProperties: false,
      },
    };
    server = await createServer(store);
    const form = new FormData();
    form.set("input", JSON.stringify({ context: "synthetic fixture" }));
    form.set(
      "image",
      new File(
        [
          Uint8Array.from([
            0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 13, 0x49, 0x48, 0x44, 0x52, 0,
            0, 0, 1, 0, 0, 0, 1,
          ]),
        ],
        "fixture.png",
        {
          type: "image/png",
        },
      ),
    );
    const encoded = new Request("http://localhost", { method: "POST", body: form });
    const response = await server.inject({
      method: "POST",
      url: "/v1/endpoints/acme/generic-transform/versions/1",
      headers: {
        authorization: `Bearer ${secret}`,
        "content-type": encoded.headers.get("content-type")!,
      },
      payload: Buffer.from(await encoded.arrayBuffer()),
    });
    expect(response.statusCode).toBe(200);
    expect(response.json()).toEqual({ result: "version one" });
    expect(store.finalStatus).toBe("succeeded");
  });

  it("rejects JSON input for an image contract before creating an invocation", async () => {
    const store = repository();
    store.versions[0] = {
      ...store.versions[0]!,
      inputSchema: {
        type: "object",
        properties: {
          sourceImage: {
            type: "string",
            contentMediaType: "image/*",
            "x-semantic-type": "image",
          },
        },
        required: ["sourceImage"],
        additionalProperties: false,
      },
    };
    let providerCalls = 0;
    server = await createServer(store, () => {
      providerCalls += 1;
      return { result: "should not execute" };
    });
    const response = await server.inject({
      method: "POST",
      url: "/v1/endpoints/acme/generic-transform/versions/1",
      headers: { authorization: `Bearer ${secret}` },
      payload: { input: { sourceImage: "https://example.test/image.png" } },
    });
    expect(response.statusCode).toBe(400);
    expect(response.json().error.code).toBe("INVALID_INPUT");
    expect(store.created).toBe(0);
    expect(providerCalls).toBe(0);
  });

  it("rejects a multipart image string without a binary attachment", async () => {
    const store = repository();
    store.versions[0] = {
      ...store.versions[0]!,
      inputSchema: {
        type: "object",
        properties: {
          image: {
            type: "string",
            contentMediaType: "image/*",
            "x-semantic-type": "image",
          },
        },
        required: ["image"],
        additionalProperties: false,
      },
    };
    let providerCalls = 0;
    server = await createServer(store, () => {
      providerCalls += 1;
      return { result: "should not execute" };
    });
    const form = new FormData();
    form.set("input", JSON.stringify({ image: "https://example.test/image.png" }));
    const encoded = new Request("http://localhost", { method: "POST", body: form });
    const response = await server.inject({
      method: "POST",
      url: "/v1/endpoints/acme/generic-transform/versions/1",
      headers: {
        authorization: `Bearer ${secret}`,
        "content-type": encoded.headers.get("content-type")!,
      },
      payload: Buffer.from(await encoded.arrayBuffer()),
    });
    expect(response.statusCode).toBe(400);
    expect(response.json().error.code).toBe("INVALID_INPUT");
    expect(store.created).toBe(0);
    expect(providerCalls).toBe(0);
  });

  it("allows an omitted optional image field without an attachment", async () => {
    const store = repository();
    store.versions[0] = {
      ...store.versions[0]!,
      inputSchema: {
        type: "object",
        properties: {
          image: {
            type: "string",
            contentMediaType: "image/*",
            "x-semantic-type": "image",
          },
        },
        additionalProperties: false,
      },
    };
    let providerCalls = 0;
    server = await createServer(store, () => {
      providerCalls += 1;
      return { result: "optional image omitted" };
    });
    const form = new FormData();
    form.set("input", JSON.stringify({}));
    const encoded = new Request("http://localhost", { method: "POST", body: form });
    const response = await server.inject({
      method: "POST",
      url: "/v1/endpoints/acme/generic-transform/versions/1",
      headers: {
        authorization: `Bearer ${secret}`,
        "content-type": encoded.headers.get("content-type")!,
      },
      payload: Buffer.from(await encoded.arrayBuffer()),
    });
    expect(response.statusCode).toBe(200);
    expect(response.json()).toEqual({ result: "optional image omitted" });
    expect(store.created).toBe(1);
    expect(providerCalls).toBe(1);
  });

  it("rejects excess multipart files before creating an invocation", async () => {
    const store = repository();
    requiredImageContract(store);
    let providerCalls = 0;
    server = await createServer(store, () => {
      providerCalls += 1;
      return { result: "should not execute" };
    });
    const form = new FormData();
    form.set("input", JSON.stringify({}));
    form.append("image", imageFile("first.png"));
    form.append("image", imageFile("second.png"));
    const encoded = await encodeMultipart(form);
    const response = await server.inject({
      method: "POST",
      url: "/v1/endpoints/acme/generic-transform/versions/1",
      headers: { authorization: `Bearer ${secret}`, "content-type": encoded["content-type"] },
      payload: encoded.payload,
    });
    expect(response.statusCode).toBe(400);
    expect(response.json().error.code).toBe("INVALID_INPUT");
    expect(store.created).toBe(0);
    expect(providerCalls).toBe(0);
  });

  it("normalizes the multipart fields limit before creating an invocation", async () => {
    const store = repository();
    let providerCalls = 0;
    server = await createServer(store, () => {
      providerCalls += 1;
      return { result: "should not execute" };
    });
    const form = new FormData();
    for (let index = 0; index < 11; index += 1) {
      form.append("input", JSON.stringify({ text: "hello" }));
    }
    const encoded = await encodeMultipart(form);
    const response = await server.inject({
      method: "POST",
      url: "/v1/endpoints/acme/generic-transform/versions/1",
      headers: { authorization: `Bearer ${secret}`, "content-type": encoded["content-type"] },
      payload: encoded.payload,
    });
    expect(response.statusCode).toBe(413);
    expect(response.json().error.code).toBe("REQUEST_TOO_LARGE");
    expect(store.created).toBe(0);
    expect(providerCalls).toBe(0);
  });

  it("rejects excess multipart parts before creating an invocation", async () => {
    const store = repository();
    requiredImageContract(store);
    let providerCalls = 0;
    server = await createServer(store, () => {
      providerCalls += 1;
      return { result: "should not execute" };
    });
    const form = new FormData();
    for (let index = 0; index < 10; index += 1) {
      form.append("input", JSON.stringify({}));
    }
    form.append("image", imageFile("image.png"));
    form.append("input", JSON.stringify({}));
    const encoded = await encodeMultipart(form);
    const response = await server.inject({
      method: "POST",
      url: "/v1/endpoints/acme/generic-transform/versions/1",
      headers: { authorization: `Bearer ${secret}`, "content-type": encoded["content-type"] },
      payload: encoded.payload,
    });
    expect(response.statusCode).toBe(400);
    expect(response.json().error.code).toBe("INVALID_INPUT");
    expect(store.created).toBe(0);
    expect(providerCalls).toBe(0);
  });

  it.each(["FST_REQ_FILE_TOO_LARGE", "FST_FILES_LIMIT", "FST_PARTS_LIMIT", "FST_FIELDS_LIMIT"])(
    "normalizes multipart limit error %s to a 413 response",
    (code) => {
      const normalized = normalizedRuntimeError({ code });
      expect(normalized.code).toBe("REQUEST_TOO_LARGE");
      expect(runtimeStatus(normalized.code)).toBe(413);
    },
  );

  it.each(["FST_MP_PREMATURE_CLOSE", "ERR_STREAM_PREMATURE_CLOSE"])(
    "normalizes malformed multipart stream error %s to a client error",
    (code) => {
      const normalized = normalizedRuntimeError({ code });
      expect(normalized.code).toBe("INVALID_INPUT");
      expect(runtimeStatus(normalized.code)).toBe(400);
    },
  );
});
