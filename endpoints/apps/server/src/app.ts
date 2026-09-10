import { randomUUID } from "node:crypto";
import cors from "@fastify/cors";
import multipart from "@fastify/multipart";
import rateLimit from "@fastify/rate-limit";
import Fastify, { type FastifyInstance } from "fastify";
import { createLoggerOptions } from "@parish/observability";
import type { ServerConfig } from "./config.js";
import type { CreatorAuthenticator } from "./auth/creator-auth.js";
import { registerControlRoutes } from "./control/routes.js";
import type { ControlService } from "./control/service.js";
import type { PlaygroundService } from "./control/playground-service.js";
import { registerInvocationRoutes } from "./invocation/routes.js";
import type { InvocationService } from "./invocation/service.js";

export interface HealthProbe {
  ready(): Promise<void>;
}

export interface AppDependencies {
  health: HealthProbe;
  control?: {
    service: ControlService;
    playground?: PlaygroundService;
    authenticator: CreatorAuthenticator;
  };
  invocation?: { service: InvocationService };
}

export async function buildServer(
  config: ServerConfig,
  dependencies: AppDependencies,
): Promise<FastifyInstance> {
  const server = Fastify({
    bodyLimit: config.maxRequestBytes,
    requestIdHeader: "x-request-id",
    genReqId: () => `req_${randomUUID()}`,
    logger: createLoggerOptions(),
  });

  await server.register(cors, {
    origin: config.webOrigin,
    credentials: true,
    methods: ["GET", "HEAD", "POST", "PUT", "DELETE", "OPTIONS"],
    allowedHeaders: [
      "authorization",
      "content-type",
      "x-request-id",
      "idempotency-key",
      "x-parish-owner-id",
    ],
  });
  await server.register(rateLimit, {
    global: false,
    max: config.requestsPerMinute,
    timeWindow: "1 minute",
  });
  await server.register(multipart, {
    limits: {
      files: 1,
      fileSize: config.maxImageBytes,
      fields: 10,
      parts: 11,
    },
  });

  server.get("/health/live", async () => ({ status: "ok" }));
  server.get("/health/ready", async (_request, reply) => {
    try {
      await dependencies.health.ready();
      return { status: "ready" };
    } catch {
      return reply.status(503).send({ status: "not_ready" });
    }
  });

  if (dependencies.control !== undefined) {
    await registerControlRoutes(
      server,
      dependencies.control.service,
      dependencies.control.authenticator,
      dependencies.control.playground,
      config.maxImagePixels,
    );
  }
  if (dependencies.invocation !== undefined) {
    await registerInvocationRoutes(
      server,
      dependencies.invocation.service,
      config.requestsPerMinute,
      config.maxImagePixels,
    );
  }

  server.setNotFoundHandler(async (request, reply) => {
    return reply.status(404).send({
      error: {
        code: "ENDPOINT_NOT_FOUND",
        message: "The requested resource was not found.",
        request_id: request.id,
      },
    });
  });

  return server;
}
