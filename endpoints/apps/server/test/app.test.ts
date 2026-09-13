import { afterEach, describe, expect, it } from "vitest";
import type { FastifyInstance } from "fastify";
import { buildServer } from "../src/app.js";
import type { ServerConfig } from "../src/config.js";

const config: ServerConfig = {
  port: 3001,
  host: "127.0.0.1",
  databaseUrl: "postgres://unused",
  webOrigin: "http://localhost:3000",
  maxRequestBytes: 1024,
  maxImageBytes: 512,
  maxImagePixels: 40_000_000,
  requestsPerMinute: 10,
  requestsPerDay: 20,
  globalInferenceEnabled: true,
  providerMode: "fake",
  authMode: "development",
  ownerFirebaseUid: "user_synthetic_owner",
  allowedModels: new Set(["fake/fake-v1"]),
  providerTimeoutMs: 1_000,
  modelPrices: {},
  mobileAppBindings: [],
};

let server: FastifyInstance | undefined;
afterEach(async () => server?.close());

describe("server health", () => {
  it("separates liveness from dependency readiness", async () => {
    server = await buildServer(config, { health: { ready: async () => undefined } });
    expect((await server.inject({ method: "GET", url: "/health/live" })).statusCode).toBe(200);
    expect((await server.inject({ method: "GET", url: "/health/ready" })).statusCode).toBe(200);
  });

  it("returns unavailable when PostgreSQL is not ready", async () => {
    server = await buildServer(config, {
      health: { ready: async () => Promise.reject(new Error("database unavailable")) },
    });
    expect((await server.inject({ method: "GET", url: "/health/ready" })).statusCode).toBe(503);
  });
});
