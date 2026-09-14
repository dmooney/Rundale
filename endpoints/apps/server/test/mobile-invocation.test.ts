import { readFile } from "node:fs/promises";
import { afterEach, describe, expect, it } from "vitest";
import type { FastifyInstance } from "fastify";
import type { EndpointVersionSnapshot } from "@parish/domain";
import { FixedPriceCostCalculator, StaticProviderRegistry } from "@parish/providers";
import { DeterministicRuntime, RuntimeError, type ModelProvider } from "@parish/runtime";
import { compileSchema } from "@parish/schemas";
import { FirebaseMobileAuthenticator } from "../src/auth/mobile-auth.js";
import { buildServer } from "../src/app.js";
import { readServerConfig } from "../src/config.js";
import type { ServerConfig } from "../src/config.js";
import { InvocationService } from "../src/invocation/service.js";
import { MemoryInvocationRepository } from "./memory-invocation-repository.js";

const binding = {
  appId: "app-ios",
  organizationId: "org_1",
  organizationSlug: "acme",
  endpointVersions: { "generic-transform": [1] },
  dailyInvocationQuota: 10,
};

function auth() {
  return new FirebaseMobileAuthenticator(
    {
      verifyIdToken: async (token) => {
        if (token !== "id") throw new Error("bad id");
        return { uid: "user-1" };
      },
      verifyAppCheckToken: async (token) => {
        if (token !== "app-check") throw new Error("bad app check");
        return { appId: "app-ios" };
      },
    },
    [binding],
  );
}

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
  ownerFirebaseUid: "owner",
  allowedModels: new Set(["fake/fake-v1"]),
  providerTimeoutMs: 1_000,
  modelPrices: {},
  mobileAppBindings: [binding],
};

const version: EndpointVersionSnapshot = {
  id: "version_1",
  endpointId: "endpoint_1",
  organizationId: "org_1",
  version: 1,
  contentHash: "sha256:test",
  inputSchema: {
    type: "object",
    properties: { playerInput: { type: "string" } },
    required: ["playerInput"],
    additionalProperties: false,
  },
  outputSchema: {
    type: "object",
    properties: { dialogue: { type: "string" } },
    required: ["dialogue"],
    additionalProperties: false,
  },
  instructions: "Answer as Peig.",
  providerConfig: { provider: "fake", model: "fake-v1" },
  inferenceConfig: {
    maxOutputTokens: 128,
    retryCount: 0,
    streaming: { version: 1, textField: "dialogue" },
  },
  publishedBy: "user_1",
  publishedAt: new Date(),
};

function repository() {
  return new MemoryInvocationRepository(
    {
      id: "unused_key",
      organizationId: "org_1",
      keyDigest: "unused",
      scopes: [],
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
    [structuredClone(version)],
  );
}

function provider(
  stream: ModelProvider["stream"] = async function* () {
    const output = { dialogue: "Hello from Peig. 🌧" };
    yield { type: "delta", text: '{"unrelated":"private","dia' };
    yield { type: "delta", text: 'logue":"Hello from Peig. \\uD83C\\uDF27"}' };
    yield {
      type: "completed",
      result: {
        output,
        usage: { inputTokens: 4, outputTokens: 5, totalTokens: 9 },
        providerRequestId: "provider_1",
      },
    };
  },
): ModelProvider {
  return {
    id: "fake",
    supportsStreaming: true,
    execute: async () => ({ output: { dialogue: "completed" }, usage: {} }),
    stream,
  };
}

async function createServer(
  store: MemoryInvocationRepository,
  options: {
    modelProvider?: ModelProvider;
    requestsPerMinute?: number;
    mobileAuth?: FirebaseMobileAuthenticator;
  } = {},
): Promise<FastifyInstance> {
  const runtime = new DeterministicRuntime(
    new StaticProviderRegistry([options.modelProvider ?? provider()]),
    new FixedPriceCostCalculator({}),
  );
  const service = new InvocationService(store, runtime, {
    globalInferenceEnabled: true,
    requestsPerMinute: options.requestsPerMinute ?? 100,
    requestsPerDay: 200,
    timeoutMs: 1_000,
    mobileAuthenticator: options.mobileAuth ?? auth(),
  });
  return buildServer(config, {
    health: { ready: async () => undefined },
    invocation: { service },
  });
}

function mobileHeaders(attempt = "attempt-1") {
  return {
    authorization: "Bearer id",
    "x-firebase-appcheck": "app-check",
    "x-request-id": "request-1",
    "x-attempt-id": attempt,
  };
}

function decodeSse(payload: string) {
  return payload
    .trim()
    .split(/\r?\n\r?\n/)
    .filter((block) => block.split(/\r?\n/).some((line) => line.startsWith("data: ")))
    .map((block) => {
      const lines = block.split(/\r?\n/);
      const event = lines.find((line) => line.startsWith("event: "))?.slice(7);
      const id = lines.find((line) => line.startsWith("id: "))?.slice(4);
      const data = lines.find((line) => line.startsWith("data: "))?.slice(6);
      if (event === undefined || id === undefined || data === undefined)
        throw new Error("Malformed test SSE payload.");
      return { event, id, data: JSON.parse(data) as Record<string, unknown> };
    });
}

let server: FastifyInstance | undefined;
afterEach(async () => server?.close());

describe("mobile invocation authentication", () => {
  it("requires both Firebase ID token and App Check", async () => {
    const result = await auth().authenticate({ headers: { authorization: "Bearer id" } });
    expect(result).toBeNull();
  });

  it("rejects an unbound App Check app and accepts the bound app", async () => {
    const result = await auth().authenticate({
      headers: { authorization: "Bearer id", "x-firebase-appcheck": "wrong" },
    });
    expect(result).toBeNull();
    const accepted = await auth().authenticate({
      headers: { authorization: "Bearer id", "x-firebase-appcheck": "app-check" },
    });
    expect(accepted).toMatchObject({
      kind: "mobile",
      uid: "user-1",
      appId: "app-ios",
      organizationId: "org_1",
    });
  });

  it("rejects unknown mobile binding keys", () => {
    expect(() =>
      readServerConfig({
        AUTH_MODE: "development",
        MOBILE_APP_BINDINGS_JSON: JSON.stringify([{ ...binding, extra: true }]),
      }),
    ).toThrow(/Invalid mobile app binding/);
  });

  it("streams the frozen v1 contract for a bound mobile principal", async () => {
    const store = repository();
    server = await createServer(store);
    const response = await server.inject({
      method: "POST",
      url: "/v1/endpoints/acme/generic-transform/versions/1/stream",
      headers: mobileHeaders(),
      payload: { input: { playerInput: "hello" } },
    });
    expect(response.statusCode).toBe(200);
    expect(response.headers["content-type"]).toContain("text/event-stream");
    const frames = decodeSse(response.payload);
    expect(frames.map((frame) => frame.event)).toEqual(["progress", "text_delta", "final"]);
    expect(frames.map((frame) => frame.data.sequence)).toEqual([1, 2, 3]);
    for (const frame of frames) {
      expect(frame.data).toMatchObject({
        contract_version: 1,
        endpoint_version: 1,
        request_id: "request-1",
        attempt_id: "attempt-1",
        invocation_id: "invocation_1",
        event_id: frame.id,
      });
    }
    expect(frames[1]!.data).toMatchObject({ text: "Hello from Peig. 🌧", terminal: false });
    expect(frames[2]!.data).toMatchObject({
      output: { dialogue: "Hello from Peig. 🌧" },
      terminal: true,
    });
    expect(response.payload).not.toContain("private");
    expect(store.finalStatus).toBe("succeeded");
    expect(store.lastStart).toMatchObject({
      requestId: "request-1:attempt-1",
      apiKeyId: null,
      callerOrganizationId: "org_1",
    });
  });

  it("hides unbound organizations and cross-tenant endpoint versions", async () => {
    const wrongSlugStore = repository();
    server = await createServer(wrongSlugStore);
    const wrongSlug = await server.inject({
      method: "POST",
      url: "/v1/endpoints/other/generic-transform/versions/1/stream",
      headers: mobileHeaders(),
      payload: { input: { playerInput: "hello" } },
    });
    expect(wrongSlug.statusCode).toBe(404);
    expect(wrongSlugStore.created).toBe(0);
    await server.close();

    const crossTenantStore = repository();
    crossTenantStore.versions[0] = { ...crossTenantStore.versions[0]!, organizationId: "org_2" };
    server = await createServer(crossTenantStore);
    const crossTenant = await server.inject({
      method: "POST",
      url: "/v1/endpoints/acme/generic-transform/versions/1/stream",
      headers: mobileHeaders(),
      payload: { input: { playerInput: "hello" } },
    });
    expect(crossTenant.statusCode).toBe(404);
    expect(crossTenantStore.created).toBe(0);

    await server.close();
    const unboundVersionStore = repository();
    unboundVersionStore.versions.push({ ...structuredClone(version), id: "version_2", version: 2 });
    server = await createServer(unboundVersionStore);
    const unboundVersion = await server.inject({
      method: "POST",
      url: "/v1/endpoints/acme/generic-transform/versions/2/stream",
      headers: mobileHeaders(),
      payload: { input: { playerInput: "hello" } },
    });
    expect(unboundVersion.statusCode).toBe(404);
    expect(unboundVersionStore.created).toBe(0);

    const alias = await server.inject({
      method: "POST",
      url: "/v1/endpoints/acme/generic-transform/stream",
      headers: mobileHeaders("attempt-alias"),
      payload: { input: { playerInput: "hello" } },
    });
    expect(alias.statusCode).toBe(404);
    expect(unboundVersionStore.created).toBe(0);
  });

  it("applies mobile quotas, rate limits, and organization kill switches", async () => {
    const quotaStore = repository();
    quotaStore.used = 10;
    server = await createServer(quotaStore);
    const quota = await server.inject({
      method: "POST",
      url: "/v1/endpoints/acme/generic-transform/versions/1/stream",
      headers: mobileHeaders(),
      payload: { input: { playerInput: "hello" } },
    });
    expect(quota.statusCode).toBe(429);
    expect(quota.json().error.code).toBe("QUOTA_EXCEEDED");
    await server.close();

    const rateStore = repository();
    server = await createServer(rateStore, { requestsPerMinute: 1 });
    const first = await server.inject({
      method: "POST",
      url: "/v1/endpoints/acme/generic-transform/versions/1/stream",
      headers: mobileHeaders("attempt-rate-1"),
      payload: { input: { playerInput: "hello" } },
    });
    expect(first.statusCode).toBe(200);
    const limited = await server.inject({
      method: "POST",
      url: "/v1/endpoints/acme/generic-transform/versions/1/stream",
      headers: mobileHeaders("attempt-rate-2"),
      payload: { input: { playerInput: "hello" } },
    });
    expect(limited.statusCode).toBe(429);
    expect(limited.json().error.code).toBe("RATE_LIMITED");
    await server.close();

    const disabledStore = repository();
    disabledStore.endpoint.organizationStatus = "suspended";
    server = await createServer(disabledStore);
    const disabled = await server.inject({
      method: "POST",
      url: "/v1/endpoints/acme/generic-transform/versions/1/stream",
      headers: mobileHeaders(),
      payload: { input: { playerInput: "hello" } },
    });
    expect(disabled.statusCode).toBe(404);
    expect(disabledStore.created).toBe(0);
  });

  it("rejects unsupported streaming before creating an invocation", async () => {
    const store = repository();
    server = await createServer(store, {
      modelProvider: { ...provider(), supportsStreaming: false },
    });
    const response = await server.inject({
      method: "POST",
      url: "/v1/endpoints/acme/generic-transform/versions/1/stream",
      headers: mobileHeaders(),
      payload: { input: { playerInput: "hello" } },
    });
    expect(response.statusCode).toBe(502);
    expect(response.json().error.code).toBe("MODEL_ERROR");
    expect(store.created).toBe(0);
  });

  it("rejects extra envelope fields and enforces authored collection bounds", async () => {
    const store = repository();
    server = await createServer(store);
    const response = await server.inject({
      method: "POST",
      url: "/v1/endpoints/acme/generic-transform/versions/1/stream",
      headers: mobileHeaders(),
      payload: { input: { playerInput: "hello" }, unexpected: true },
    });
    expect(response.statusCode).toBe(400);
    expect(response.json().error.code).toBe("INVALID_INPUT");
    expect(store.created).toBe(0);

    const definition = JSON.parse(
      await readFile(
        new URL("../../../../mobile/endpoint/rundale-dialogue-v1.json", import.meta.url),
        "utf8",
      ),
    ) as { inputSchema: Record<string, unknown> };
    const input = JSON.parse(
      await readFile(
        new URL("../../../../mobile/endpoint/example-engine-invocation.json", import.meta.url),
        "utf8",
      ),
    ) as Record<string, unknown>;
    const validate = compileSchema(definition.inputSchema);
    expect(validate(input)).toBe(true);
    input.knownPeople = Array.from({ length: 33 }, () => structuredClone(input.speaker));
    expect(validate(input)).toBe(false);
  });

  it("emits exactly one terminal error and records provider failure", async () => {
    const store = repository();
    server = await createServer(store, {
      modelProvider: provider(async function* () {
        yield* [];
        throw new RuntimeError("PROVIDER_UNAVAILABLE", "Provider unavailable.");
      }),
    });
    const response = await server.inject({
      method: "POST",
      url: "/v1/endpoints/acme/generic-transform/versions/1/stream",
      headers: mobileHeaders(),
      payload: { input: { playerInput: "hello" } },
    });
    const frames = decodeSse(response.payload);
    expect(frames.map((frame) => frame.event)).toEqual(["progress", "error"]);
    expect(frames.filter((frame) => frame.data.terminal === true)).toHaveLength(1);
    expect(frames[1]!.data).toMatchObject({
      type: "error",
      terminal: true,
      error: { code: "PROVIDER_UNAVAILABLE" },
    });
    expect(store.finalStatus).toBe("failed");
    expect(store.finalFailureResult?.errorCode).toBe("PROVIDER_UNAVAILABLE");
  });

  it("propagates a socket disconnect and records cancellation", async () => {
    const store = repository();
    let resolveFinalized!: () => void;
    const finalized = new Promise<void>((resolve) => {
      resolveFinalized = resolve;
    });
    const finalizeFailure = store.finalizeFailure.bind(store);
    store.finalizeFailure = async (...arguments_) => {
      const result = await finalizeFailure(...arguments_);
      resolveFinalized();
      return result;
    };
    server = await createServer(store, {
      modelProvider: provider(async function* (_invocation, context) {
        yield* [];
        await new Promise<void>((_resolve, reject) => {
          context.signal.addEventListener(
            "abort",
            () => reject(new DOMException("Aborted", "AbortError")),
            { once: true },
          );
        });
      }),
    });
    const address = await server.listen({ port: 0, host: "127.0.0.1" });
    const controller = new AbortController();
    const response = await fetch(
      `${address}/v1/endpoints/acme/generic-transform/versions/1/stream`,
      {
        method: "POST",
        headers: { ...mobileHeaders(), "content-type": "application/json" },
        body: JSON.stringify({ input: { playerInput: "hello" } }),
        signal: controller.signal,
      },
    );
    expect(response.status).toBe(200);
    const reader = response.body!.getReader();
    expect((await reader.read()).done).toBe(false);
    controller.abort();
    await finalized;
    expect(store.finalStatus).toBe("failed");
    expect(store.finalFailureResult?.errorCode).toBe("REQUEST_CANCELLED");
  });

  it("authenticates an explicit Stop request and aborts the matching stream", async () => {
    const store = repository();
    let resolveFinalized!: () => void;
    const finalized = new Promise<void>((resolve) => {
      resolveFinalized = resolve;
    });
    const finalizeFailure = store.finalizeFailure.bind(store);
    store.finalizeFailure = async (...arguments_) => {
      const result = await finalizeFailure(...arguments_);
      resolveFinalized();
      return result;
    };
    server = await createServer(store, {
      modelProvider: provider(async function* (_invocation, context) {
        yield* [];
        await new Promise<void>((_resolve, reject) => {
          context.signal.addEventListener(
            "abort",
            () => reject(new DOMException("Aborted", "AbortError")),
            { once: true },
          );
        });
      }),
    });
    const address = await server.listen({ port: 0, host: "127.0.0.1" });
    const response = await fetch(
      `${address}/v1/endpoints/acme/generic-transform/versions/1/stream`,
      {
        method: "POST",
        headers: { ...mobileHeaders(), "content-type": "application/json" },
        body: JSON.stringify({ input: { playerInput: "hello" } }),
      },
    );
    expect(response.status).toBe(200);
    const reader = response.body!.getReader();
    expect((await reader.read()).done).toBe(false);

    const rejected = await fetch(
      `${address}/v1/endpoints/acme/generic-transform/versions/1/stream`,
      { method: "DELETE", headers: { ...mobileHeaders(), "x-firebase-appcheck": "wrong" } },
    );
    expect(rejected.status).toBe(401);
    expect(store.finalStatus).toBeNull();

    const cancellation = await fetch(
      `${address}/v1/endpoints/acme/generic-transform/versions/1/stream`,
      { method: "DELETE", headers: mobileHeaders() },
    );
    expect(cancellation.status).toBe(202);
    await finalized;
    expect(store.finalStatus).toBe("failed");
    expect(store.finalFailureResult?.errorCode).toBe("REQUEST_CANCELLED");
    expect((await reader.read()).done).toBe(true);
  });

  it("lets a persisted Stop win while completed output is awaiting accounting", async () => {
    const store = repository();
    let releaseAccounting!: () => void;
    const accountingReleased = new Promise<void>((resolve) => {
      releaseAccounting = resolve;
    });
    let accountingStarted!: () => void;
    const accountingReached = new Promise<void>((resolve) => {
      accountingStarted = resolve;
    });
    const recordAttempts = store.recordAttempts.bind(store);
    store.recordAttempts = async (...arguments_) => {
      accountingStarted();
      await accountingReleased;
      await recordAttempts(...arguments_);
    };
    server = await createServer(store);
    const address = await server.listen({ port: 0, host: "127.0.0.1" });
    const response = await fetch(
      `${address}/v1/endpoints/acme/generic-transform/versions/1/stream`,
      {
        method: "POST",
        headers: { ...mobileHeaders(), "content-type": "application/json" },
        body: JSON.stringify({ input: { playerInput: "hello" } }),
      },
    );
    expect(response.status).toBe(200);
    const reader = response.body!.getReader();
    expect((await reader.read()).done).toBe(false);
    await accountingReached;

    const cancellation = await fetch(
      `${address}/v1/endpoints/acme/generic-transform/versions/1/stream`,
      { method: "DELETE", headers: mobileHeaders() },
    );
    expect(cancellation.status).toBe(202);
    releaseAccounting();

    let remainder = "";
    for (;;) {
      const chunk = await reader.read();
      if (chunk.done) break;
      remainder += new TextDecoder().decode(chunk.value);
    }
    expect(remainder).not.toContain("event: final");
    expect(store.finalStatus).toBe("failed");
    expect(store.finalSuccessResult).toBeNull();
    expect(store.finalFailureResult?.errorCode).toBe("REQUEST_CANCELLED");
  });

  it("aborts and records cancellation when the stream consumer closes after a delta", async () => {
    const store = repository();
    let providerSignal: AbortSignal | undefined;
    const streamingProvider = provider(async function* (_invocation, context) {
      providerSignal = context.signal;
      yield { type: "delta", text: `{"dialogue":"${"a".repeat(40)}` };
      await new Promise<void>((_resolve, reject) => {
        context.signal.addEventListener(
          "abort",
          () => reject(new DOMException("Aborted", "AbortError")),
          { once: true },
        );
      });
    });
    const runtime = new DeterministicRuntime(
      new StaticProviderRegistry([streamingProvider]),
      new FixedPriceCostCalculator({}),
    );
    const service = new InvocationService(store, runtime, {
      globalInferenceEnabled: true,
      requestsPerMinute: 100,
      requestsPerDay: 200,
      timeoutMs: 1_000,
      mobileAuthenticator: auth(),
    });
    const request = {
      authorization: "Bearer id",
      appCheck: "app-check",
      organizationSlug: "acme",
      endpointSlug: "generic-transform",
      version: 1,
      requestId: "request-consumer-close",
      attemptId: "attempt-consumer-close",
      input: { values: { playerInput: "hello" }, attachments: [] },
      inputBytes: 27,
    };
    const authorized = await service.resolveAuthorized(request);
    const handle = await service.openStream(request, authorized);
    const iterator = handle.events[Symbol.asyncIterator]();
    expect(await iterator.next()).toMatchObject({ done: false, value: { type: "delta" } });
    await iterator.return?.();
    expect(providerSignal?.aborted).toBe(true);
    expect(store.finalStatus).toBe("failed");
    expect(store.finalFailureResult?.errorCode).toBe("REQUEST_CANCELLED");
  });

  it("keeps mobile invocation authentication separate from creator routes", async () => {
    server = await buildServer(config, {
      health: { ready: async () => undefined },
      control: {
        service: {} as never,
        authenticator: { authenticate: async () => null },
      },
    });
    const response = await server.inject({
      method: "GET",
      url: "/api/control/v1/endpoints",
      headers: mobileHeaders(),
    });
    expect(response.statusCode).toBe(401);
  });

  it("consumes the same frozen wire fixture as Swift and Rust", async () => {
    const fixture = await readFile(
      new URL("../../../../mobile/endpoint/fixtures/dialogue-v1.sse", import.meta.url),
      "utf8",
    );
    const frames = decodeSse(fixture);
    expect(frames.map((frame) => frame.event)).toEqual([
      "progress",
      "text_delta",
      "text_delta",
      "final",
    ]);
    frames.forEach((frame, index) => {
      expect(frame.data).toMatchObject({
        contract_version: 1,
        endpoint_version: 1,
        request_id: "request-fixture",
        attempt_id: "attempt-fixture",
        invocation_id: "invocation-fixture",
        event_id: frame.id,
        sequence: index + 1,
        type: frame.event,
        terminal: index === frames.length - 1,
      });
    });
    expect(frames.at(-1)!.data.output).toEqual({
      dialogue: "The rain keeps the old road quiet. 🌧",
    });
  });
});
