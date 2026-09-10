import { afterEach, describe, expect, it } from "vitest";
import type { FastifyInstance } from "fastify";
import type { CreatorAuthenticator } from "../src/auth/creator-auth.js";
import { buildServer } from "../src/app.js";
import type { ServerConfig } from "../src/config.js";
import { ControlService } from "../src/control/service.js";
import { PlaygroundService } from "../src/control/playground-service.js";
import { FakeProvider, FixedPriceCostCalculator, StaticProviderRegistry } from "@parish/providers";
import { DeterministicRuntime } from "@parish/runtime";
import { MemoryControlRepository } from "./memory-control-repository.js";
import { MemoryInvocationRepository } from "./memory-invocation-repository.js";

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
};
const actor = { userId: "user_1", organizationId: "org_1", role: "owner" as const };
const auth: CreatorAuthenticator = {
  authenticate: async (request) =>
    request.headers.authorization === "Bearer creator-session" ? actor : null,
};
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
  providerConfig: { provider: "fake", model: "fake-v1" },
  inferenceConfig: { maxOutputTokens: 128, retryCount: 0 },
};

function services(repository: MemoryControlRepository) {
  const invocations = new MemoryInvocationRepository(
    {
      id: "key_unused",
      organizationId: actor.organizationId,
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
  const runtime = new DeterministicRuntime(
    new StaticProviderRegistry([new FakeProvider()]),
    new FixedPriceCostCalculator({}),
  );
  return {
    service: new ControlService(repository, { allowedModels: config.allowedModels }),
    playground: new PlaygroundService(repository, invocations, runtime, {
      globalInferenceEnabled: true,
      timeoutMs: 1_000,
      requestsPerDay: 200,
    }),
    authenticator: auth,
    invocations,
  };
}

let server: FastifyInstance | undefined;
afterEach(async () => server?.close());

describe("control-plane workflow", () => {
  it("allows browser preflight for mutating owner routes", async () => {
    server = await buildServer(config, {
      health: { ready: async () => undefined },
      control: services(new MemoryControlRepository()),
    });
    const response = await server.inject({
      method: "OPTIONS",
      url: "/api/control/v1/endpoints/endpoint_1/draft",
      headers: {
        origin: config.webOrigin,
        "access-control-request-method": "PUT",
        "access-control-request-headers": "content-type,x-parish-owner-id",
      },
    });
    expect(response.statusCode).toBe(204);
    expect(response.headers["access-control-allow-methods"]).toContain("PUT");
    expect(response.headers["access-control-allow-headers"]).toContain("x-parish-owner-id");
  });

  it("requires creator authentication", async () => {
    server = await buildServer(config, {
      health: { ready: async () => undefined },
      control: services(new MemoryControlRepository()),
    });
    const response = await server.inject({ method: "GET", url: "/api/control/v1/endpoints" });
    expect(response.statusCode).toBe(401);
  });

  it("lists the configured models only for authenticated creators", async () => {
    server = await buildServer(config, {
      health: { ready: async () => undefined },
      control: services(new MemoryControlRepository()),
    });
    expect(
      (
        await server.inject({
          method: "GET",
          url: "/api/control/v1/models",
          headers: { authorization: "Bearer creator-session" },
        })
      ).json(),
    ).toEqual({ data: [{ provider: "fake", model: "fake-v1" }] });
    expect((await server.inject({ method: "GET", url: "/api/control/v1/models" })).statusCode).toBe(
      401,
    );
  });

  it("creates, publishes, promotes, rolls back, and reveals a key once", async () => {
    const repository = new MemoryControlRepository();
    server = await buildServer(config, {
      health: { ready: async () => undefined },
      control: services(repository),
    });
    const headers = { authorization: "Bearer creator-session" };
    const create = await server.inject({
      method: "POST",
      url: "/api/control/v1/endpoints",
      headers,
      payload: { name: "Generic Transform", slug: "generic-transform", definition },
    });
    expect(create.statusCode).toBe(201);
    const created = create.json<{
      data: { endpoint: { id: string }; draft: { revision: number } };
    }>();
    const endpointId = created.data.endpoint.id;

    const draftTest = await server.inject({
      method: "POST",
      url: `/api/control/v1/endpoints/${endpointId}/test`,
      headers,
      payload: { input: { text: "hello" } },
    });
    expect(draftTest.statusCode).toBe(200);
    expect(draftTest.json().data).toEqual({ result: "fake-provider-result" });

    const publishV1 = await server.inject({
      method: "POST",
      url: `/api/control/v1/endpoints/${endpointId}/publish`,
      headers,
      payload: { expectedRevision: 1 },
    });
    expect(publishV1.json().data.version).toBe(1);
    const promoteV1 = await server.inject({
      method: "PUT",
      url: `/api/control/v1/endpoints/${endpointId}/aliases/production`,
      headers,
      payload: { version: 1, expectedRevision: null },
    });
    expect(promoteV1.json().data).toEqual({ version: 1, revision: 1 });

    await server.inject({
      method: "PUT",
      url: `/api/control/v1/endpoints/${endpointId}/draft`,
      headers,
      payload: {
        expectedRevision: 1,
        definition: { ...definition, instructions: "Return a second typed result." },
      },
    });
    await server.inject({
      method: "POST",
      url: `/api/control/v1/endpoints/${endpointId}/publish`,
      headers,
      payload: { expectedRevision: 2 },
    });
    const promoteV2 = await server.inject({
      method: "PUT",
      url: `/api/control/v1/endpoints/${endpointId}/aliases/production`,
      headers,
      payload: { version: 2, expectedRevision: 1 },
    });
    expect(promoteV2.json().data).toEqual({ version: 2, revision: 2 });
    const rollback = await server.inject({
      method: "PUT",
      url: `/api/control/v1/endpoints/${endpointId}/aliases/production`,
      headers,
      payload: { version: 1, expectedRevision: 2 },
    });
    expect(rollback.json().data).toEqual({ version: 1, revision: 3 });

    const createKey = await server.inject({
      method: "POST",
      url: "/api/control/v1/api-keys",
      headers,
      payload: { name: "CLI", scopes: ["invoke:endpoint:generic-transform"] },
    });
    const secret = createKey.json().data.secret as string;
    expect(secret).toMatch(/^sfk_live_/);
    const listed = await server.inject({ method: "GET", url: "/api/control/v1/api-keys", headers });
    expect(listed.body).not.toContain(secret);
  });

  it("rejects a draft test image string before creating an invocation", async () => {
    const repository = new MemoryControlRepository();
    const dependencies = services(repository);
    server = await buildServer(config, {
      health: { ready: async () => undefined },
      control: dependencies,
    });
    const headers = { authorization: "Bearer creator-session" };
    const create = await server.inject({
      method: "POST",
      url: "/api/control/v1/endpoints",
      headers,
      payload: {
        name: "Image Transform",
        slug: "image-transform",
        definition: {
          ...definition,
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
        },
      },
    });
    const endpointId = create.json().data.endpoint.id as string;
    const response = await server.inject({
      method: "POST",
      url: `/api/control/v1/endpoints/${endpointId}/test`,
      headers,
      payload: { input: { image: "https://example.test/image.png" } },
    });
    expect(response.statusCode).toBe(400);
    expect(response.json().error.code).toBe("INVALID_INPUT");
    expect(dependencies.invocations.created).toBe(0);
  });
});
