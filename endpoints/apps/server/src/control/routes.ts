import type { EndpointDefinition, JsonSchema } from "@parish/domain";
import type { FastifyInstance, FastifyReply, FastifyRequest } from "fastify";
import type { CreatorAuthenticator } from "../auth/creator-auth.js";
import {
  normalizedRuntimeError,
  parseInvocationInput,
  runtimeStatus,
} from "../invocation/routes.js";
import type { CreatorPrincipal } from "./contracts.js";
import type { PlaygroundService } from "./playground-service.js";
import type { ControlService } from "./service.js";
import { ControlError, normalizeControlError } from "./service.js";

function record(value: unknown, label: string): Record<string, unknown> {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    throw new ControlError("INVALID_DEFINITION", `${label} must be an object.`);
  }
  return value as Record<string, unknown>;
}

function text(value: unknown, label: string): string {
  if (typeof value !== "string")
    throw new ControlError("INVALID_DEFINITION", `${label} must be text.`);
  return value;
}

function integer(value: unknown, label: string): number {
  if (!Number.isSafeInteger(value))
    throw new ControlError("INVALID_DEFINITION", `${label} must be an integer.`);
  return value as number;
}

function definition(value: unknown): EndpointDefinition {
  const body = record(value, "definition");
  const provider = record(body.providerConfig, "providerConfig");
  const inference = record(body.inferenceConfig, "inferenceConfig");
  const providerId = text(provider.provider, "provider");
  if (providerId !== "fake" && providerId !== "openai" && providerId !== "google") {
    throw new ControlError("INVALID_DEFINITION", "provider is invalid.");
  }
  const retryCount = integer(inference.retryCount, "retryCount");
  if (retryCount !== 0 && retryCount !== 1) {
    throw new ControlError("INVALID_DEFINITION", "retryCount must be 0 or 1.");
  }
  const temperature = inference.temperature;
  if (temperature !== undefined && typeof temperature !== "number") {
    throw new ControlError("INVALID_DEFINITION", "temperature must be a number.");
  }
  const streaming = inference.streaming;
  let streamingConfig: { version: 1; textField: string } | undefined;
  if (streaming !== undefined) {
    const value = streaming as Record<string, unknown>;
    if (
      streaming === null ||
      typeof streaming !== "object" ||
      Array.isArray(streaming) ||
      value.version !== 1 ||
      typeof value.textField !== "string" ||
      value.textField.trim().length === 0 ||
      value.textField.length > 128
    ) {
      throw new ControlError(
        "INVALID_DEFINITION",
        "streaming must specify version 1 and a textField.",
      );
    }
    streamingConfig = { version: 1, textField: value.textField };
  }
  return {
    inputSchema: record(body.inputSchema, "inputSchema") as JsonSchema,
    outputSchema: record(body.outputSchema, "outputSchema") as JsonSchema,
    instructions: text(body.instructions, "instructions"),
    providerConfig: { provider: providerId, model: text(provider.model, "model") },
    inferenceConfig: {
      maxOutputTokens: integer(inference.maxOutputTokens, "maxOutputTokens"),
      retryCount,
      ...(temperature === undefined ? {} : { temperature }),
      ...(streamingConfig === undefined ? {} : { streaming: streamingConfig }),
    },
  };
}

function errorStatus(error: ControlError): number {
  if (error.code === "NOT_FOUND") return 404;
  if (error.code === "CONFLICT") return 409;
  if (error.code === "FORBIDDEN") return 403;
  return 400;
}

async function principal(
  request: FastifyRequest,
  reply: FastifyReply,
  authenticator: CreatorAuthenticator,
): Promise<CreatorPrincipal | null> {
  const authenticated = await authenticator.authenticate(request);
  if (authenticated !== null) return authenticated;
  await reply.status(401).send({
    error: {
      code: "AUTHENTICATION_FAILED",
      message: "A valid creator session is required.",
      request_id: request.id,
    },
  });
  return null;
}

async function respondWithControlError(
  request: FastifyRequest,
  reply: FastifyReply,
  error: unknown,
) {
  const normalized = normalizeControlError(error);
  return reply.status(errorStatus(normalized)).send({
    error: {
      code: normalized.code,
      message: normalized.message,
      request_id: request.id,
    },
  });
}

export async function registerControlRoutes(
  server: FastifyInstance,
  service: ControlService,
  authenticator: CreatorAuthenticator,
  playground?: PlaygroundService,
  maximumImagePixels = 40_000_000,
): Promise<void> {
  await server.register(
    async (control) => {
      control.get("/endpoints", async (request, reply) => {
        const actor = await principal(request, reply, authenticator);
        if (actor === null) return;
        return { data: await service.listEndpoints(actor) };
      });

      control.get("/models", async (request, reply) => {
        const actor = await principal(request, reply, authenticator);
        if (actor === null) return;
        return { data: service.listModels() };
      });

      control.post("/endpoints", async (request, reply) => {
        const actor = await principal(request, reply, authenticator);
        if (actor === null) return;
        try {
          const body = record(request.body, "body");
          const created = await service.createEndpoint(actor, {
            name: text(body.name, "name"),
            slug: text(body.slug, "slug"),
            ...(body.description === undefined
              ? {}
              : { description: text(body.description, "description") }),
            definition: definition(body.definition),
          });
          return reply.status(201).send({ data: created });
        } catch (error) {
          return respondWithControlError(request, reply, error);
        }
      });

      control.get<{ Params: { endpointId: string } }>(
        "/endpoints/:endpointId/draft",
        async (request, reply) => {
          const actor = await principal(request, reply, authenticator);
          if (actor === null) return;
          try {
            return { data: await service.getDraft(actor, request.params.endpointId) };
          } catch (error) {
            return respondWithControlError(request, reply, error);
          }
        },
      );

      control.put<{ Params: { endpointId: string } }>(
        "/endpoints/:endpointId/draft",
        async (request, reply) => {
          const actor = await principal(request, reply, authenticator);
          if (actor === null) return;
          try {
            const body = record(request.body, "body");
            return {
              data: await service.updateDraft(
                actor,
                request.params.endpointId,
                integer(body.expectedRevision, "expectedRevision"),
                definition(body.definition),
              ),
            };
          } catch (error) {
            return respondWithControlError(request, reply, error);
          }
        },
      );

      if (playground !== undefined) {
        control.post<{ Params: { endpointId: string } }>(
          "/endpoints/:endpointId/test",
          async (request, reply) => {
            const actor = await principal(request, reply, authenticator);
            if (actor === null) return;
            try {
              const draft = await service.getDraft(actor, request.params.endpointId);
              const parsed = await parseInvocationInput(
                request,
                draft.inputSchema,
                maximumImagePixels,
              );
              const output = await playground.invokeDraft(actor, {
                requestId: request.id,
                endpointId: request.params.endpointId,
                input: parsed.input,
                inputBytes: parsed.bytes,
              });
              return reply.status(200).send({ data: output });
            } catch (error) {
              if (error instanceof ControlError) {
                return respondWithControlError(request, reply, error);
              }
              const normalized = normalizedRuntimeError(error);
              return reply.status(runtimeStatus(normalized.code)).send({
                error: {
                  code: normalized.code,
                  message: normalized.message,
                  ...(normalized.details === undefined ? {} : { details: normalized.details }),
                  request_id: request.id,
                },
              });
            }
          },
        );
      }

      control.post<{ Params: { endpointId: string } }>(
        "/endpoints/:endpointId/publish",
        async (request, reply) => {
          const actor = await principal(request, reply, authenticator);
          if (actor === null) return;
          try {
            const body = record(request.body, "body");
            const version = await service.publish(
              actor,
              request.params.endpointId,
              integer(body.expectedRevision, "expectedRevision"),
            );
            return reply.status(201).send({ data: version });
          } catch (error) {
            return respondWithControlError(request, reply, error);
          }
        },
      );

      control.get<{ Params: { endpointId: string } }>(
        "/endpoints/:endpointId/versions",
        async (request, reply) => {
          const actor = await principal(request, reply, authenticator);
          if (actor === null) return;
          return { data: await service.listVersions(actor, request.params.endpointId) };
        },
      );

      control.put<{ Params: { endpointId: string } }>(
        "/endpoints/:endpointId/aliases/production",
        async (request, reply) => {
          const actor = await principal(request, reply, authenticator);
          if (actor === null) return;
          try {
            const body = record(request.body, "body");
            const expected = body.expectedRevision;
            return {
              data: await service.promoteProduction(
                actor,
                request.params.endpointId,
                integer(body.version, "version"),
                expected === null ? null : integer(expected, "expectedRevision"),
              ),
            };
          } catch (error) {
            return respondWithControlError(request, reply, error);
          }
        },
      );

      control.get("/api-keys", async (request, reply) => {
        const actor = await principal(request, reply, authenticator);
        if (actor === null) return;
        return { data: await service.listApiKeys(actor) };
      });

      control.post("/api-keys", async (request, reply) => {
        const actor = await principal(request, reply, authenticator);
        if (actor === null) return;
        try {
          const body = record(request.body, "body");
          if (
            !Array.isArray(body.scopes) ||
            body.scopes.some((scope) => typeof scope !== "string")
          ) {
            throw new ControlError("INVALID_DEFINITION", "scopes must be a string array.");
          }
          const created = await service.createApiKey(actor, text(body.name, "name"), body.scopes);
          return reply.status(201).send({ data: created });
        } catch (error) {
          return respondWithControlError(request, reply, error);
        }
      });

      control.delete<{ Params: { keyId: string } }>("/api-keys/:keyId", async (request, reply) => {
        const actor = await principal(request, reply, authenticator);
        if (actor === null) return;
        try {
          await service.revokeApiKey(actor, request.params.keyId);
          return reply.status(204).send();
        } catch (error) {
          return respondWithControlError(request, reply, error);
        }
      });

      control.get<{ Querystring: { limit?: string } }>("/invocations", async (request, reply) => {
        const actor = await principal(request, reply, authenticator);
        if (actor === null) return;
        const limit = request.query.limit === undefined ? 100 : Number(request.query.limit);
        return {
          data: await service.listInvocations(actor, Number.isSafeInteger(limit) ? limit : 100),
        };
      });

      control.get("/usage", async (request, reply) => {
        const actor = await principal(request, reply, authenticator);
        if (actor === null) return;
        return { data: await service.getUsage(actor) };
      });
    },
    { prefix: "/api/control/v1" },
  );
}
