import type { FastifyInstance, FastifyReply, FastifyRequest } from "fastify";
import { RuntimeError, type InvocationAttachment, type InvocationInput } from "@parish/runtime";
import { assertImageDimensions, detectImageMediaType, imageFields } from "./images.js";
import type { InvocationService } from "./service.js";

interface InvocationParams {
  organizationSlug: string;
  endpointSlug: string;
  version?: string;
}

const multipartLimitErrorCodes = new Set([
  "FST_REQ_FILE_TOO_LARGE",
  "FST_FILES_LIMIT",
  "FST_PARTS_LIMIT",
  "FST_FIELDS_LIMIT",
]);
const multipartMalformedErrorCodes = new Set([
  "FST_MP_PREMATURE_CLOSE",
  "ERR_STREAM_PREMATURE_CLOSE",
]);

export function runtimeStatus(code: RuntimeError["code"]): number {
  if (code === "AUTHENTICATION_FAILED") return 401;
  if (code === "AUTHORIZATION_FAILED") return 403;
  if (code === "ENDPOINT_NOT_FOUND" || code === "VERSION_NOT_FOUND") return 404;
  if (code === "ENDPOINT_DISABLED") return 503;
  if (code === "INVALID_INPUT" || code === "UNSUPPORTED_MEDIA_TYPE") return 400;
  if (code === "REQUEST_TOO_LARGE") return 413;
  if (code === "RATE_LIMITED") return 429;
  if (code === "QUOTA_EXCEEDED") return 429;
  if (code === "REQUEST_TIMEOUT") return 504;
  if (code === "REQUEST_CANCELLED") return 499;
  if (code === "PROVIDER_UNAVAILABLE" || code === "PROVIDER_RATE_LIMITED") return 503;
  if (code === "OUTPUT_VALIDATION_FAILED" || code === "MODEL_ERROR") return 502;
  return 500;
}

export function normalizedRuntimeError(error: unknown): RuntimeError {
  if (error instanceof RuntimeError) return error;
  if (error !== null && typeof error === "object" && "code" in error) {
    const code = (error as { code?: unknown }).code;
    if (typeof code === "string" && multipartLimitErrorCodes.has(code)) {
      return new RuntimeError(
        "REQUEST_TOO_LARGE",
        code === "FST_REQ_FILE_TOO_LARGE"
          ? "The uploaded image is too large."
          : "The multipart request exceeds the configured limit.",
      );
    }
    if (typeof code === "string" && multipartMalformedErrorCodes.has(code)) {
      return new RuntimeError("INVALID_INPUT", "The multipart request could not be parsed.");
    }
  }
  return new RuntimeError("INTERNAL_ERROR", "The request could not be completed.");
}

function validCorrelationId(value: string): boolean {
  return value.length >= 1 && value.length <= 128 && /^[A-Za-z0-9._:-]+$/.test(value);
}

async function writeSse(reply: FastifyReply, payload: string): Promise<void> {
  const response = reply.raw;
  if (response.destroyed || response.writableEnded)
    throw new RuntimeError("REQUEST_CANCELLED", "The caller cancelled the request.");
  if (response.write(payload)) return;
  await new Promise<void>((resolve, reject) => {
    const cleanup = () => {
      response.off("drain", drained);
      response.off("close", closed);
      response.off("error", failed);
    };
    const drained = () => {
      cleanup();
      resolve();
    };
    const closed = () => {
      cleanup();
      reject(new RuntimeError("REQUEST_CANCELLED", "The caller cancelled the request."));
    };
    const failed = () => {
      cleanup();
      reject(new RuntimeError("REQUEST_CANCELLED", "The caller cancelled the request."));
    };
    response.once("drain", drained);
    response.once("close", closed);
    response.once("error", failed);
  });
}

async function parseJsonInput(
  request: FastifyRequest,
  schema: Record<string, unknown>,
): Promise<{ input: InvocationInput; bytes: number }> {
  if (imageFields(schema).length > 0) {
    throw new RuntimeError(
      "INVALID_INPUT",
      "Image inputs require multipart form data with an image attachment.",
    );
  }
  if (request.body === null || typeof request.body !== "object" || Array.isArray(request.body)) {
    throw new RuntimeError("INVALID_INPUT", "The request body must be a JSON object.");
  }
  const body = request.body as Record<string, unknown>;
  if (Object.keys(body).length !== 1 || !Object.hasOwn(body, "input")) {
    throw new RuntimeError("INVALID_INPUT", "The request must contain only an input object.");
  }
  if (body.input === null || typeof body.input !== "object" || Array.isArray(body.input)) {
    throw new RuntimeError("INVALID_INPUT", "The request must contain an input object.");
  }
  return {
    input: { values: body.input as Record<string, unknown>, attachments: [] },
    bytes: Buffer.byteLength(JSON.stringify(body)),
  };
}

async function parseMultipartInput(
  request: FastifyRequest,
  schema: Record<string, unknown>,
  maximumImagePixels: number,
): Promise<{ input: InvocationInput; bytes: number }> {
  const expectedImageFields = imageFields(schema);
  const requiredFields = Array.isArray(schema.required) ? schema.required : [];
  const requiredImageFields = expectedImageFields.filter((field) => requiredFields.includes(field));
  const attachments: InvocationAttachment[] = [];
  let values: Record<string, unknown> = {};
  let bytes = 0;
  let sawInput = false;
  let malformedFields = false;
  for await (const part of request.parts()) {
    if (part.type === "field") {
      if (part.fieldname !== "input" || typeof part.value !== "string" || sawInput) {
        malformedFields = true;
        continue;
      }
      sawInput = true;
      try {
        const parsed: unknown = JSON.parse(part.value);
        if (parsed === null || typeof parsed !== "object" || Array.isArray(parsed))
          throw new Error();
        values = parsed as Record<string, unknown>;
        bytes += Buffer.byteLength(part.value);
      } catch {
        throw new RuntimeError(
          "INVALID_INPUT",
          "The multipart input field must contain a JSON object.",
        );
      }
      continue;
    }
    if (attachments.length > 0) {
      throw new RuntimeError("INVALID_INPUT", "Only one image is supported per invocation.");
    }
    const data = await part.toBuffer();
    const detected = detectImageMediaType(data);
    assertImageDimensions(data, detected, maximumImagePixels);
    if (part.mimetype !== detected) {
      throw new RuntimeError(
        "UNSUPPORTED_MEDIA_TYPE",
        "The declared image type does not match its content.",
      );
    }
    const field = expectedImageFields.includes(part.fieldname)
      ? part.fieldname
      : expectedImageFields.length === 1 && part.fieldname === "image"
        ? expectedImageFields[0]!
        : undefined;
    if (field === undefined) {
      throw new RuntimeError(
        "INVALID_INPUT",
        "The uploaded image does not match an image field in the contract.",
      );
    }
    attachments.push({ field, mediaType: detected, bytes: data });
    bytes += data.byteLength;
  }
  if (malformedFields) {
    throw new RuntimeError("INVALID_INPUT", "Multipart fields must use one JSON 'input' part.");
  }
  if (!sawInput) {
    throw new RuntimeError("INVALID_INPUT", "The request must contain an input object.");
  }
  if (expectedImageFields.some((field) => Object.hasOwn(values, field))) {
    throw new RuntimeError(
      "INVALID_INPUT",
      "Image fields must be supplied as multipart attachments.",
    );
  }
  const attachedFields = new Set(attachments.map((attachment) => attachment.field));
  if (requiredImageFields.some((field) => !attachedFields.has(field))) {
    throw new RuntimeError("INVALID_INPUT", "The request must contain an image attachment.");
  }
  return { input: { values, attachments }, bytes };
}

export async function parseInvocationInput(
  request: FastifyRequest,
  schema: Record<string, unknown>,
  maximumImagePixels: number,
): Promise<{ input: InvocationInput; bytes: number }> {
  return request.isMultipart()
    ? parseMultipartInput(request, schema, maximumImagePixels)
    : parseJsonInput(request, schema);
}

async function handler(
  request: FastifyRequest<{ Params: InvocationParams }>,
  reply: FastifyReply,
  service: InvocationService,
  maximumImagePixels: number,
) {
  try {
    const versionText = request.params.version;
    const version = versionText === undefined ? undefined : Number(versionText);
    if (version !== undefined && (!Number.isSafeInteger(version) || version < 1)) {
      throw new RuntimeError("VERSION_NOT_FOUND", "The requested Endpoint version was not found.");
    }
    const authorization = request.headers.authorization;
    const base = {
      authorization,
      appCheck:
        typeof request.headers["x-firebase-appcheck"] === "string"
          ? request.headers["x-firebase-appcheck"]
          : undefined,
      organizationSlug: request.params.organizationSlug,
      endpointSlug: request.params.endpointSlug,
      ...(version === undefined ? {} : { version }),
      requestId: request.id,
    };
    const authorized = await service.resolveAuthorized(base);
    const parsed = await parseInvocationInput(
      request,
      authorized.endpoint.version.inputSchema,
      maximumImagePixels,
    );
    const output = await service.invoke(
      { ...base, input: parsed.input, inputBytes: parsed.bytes },
      authorized,
    );
    return reply.status(200).send(output);
  } catch (error) {
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
}

async function streamHandler(
  request: FastifyRequest<{ Params: InvocationParams }>,
  reply: FastifyReply,
  service: InvocationService,
  maximumImagePixels: number,
) {
  let started = false;
  let streamHandle: Awaited<ReturnType<InvocationService["openStream"]>> | undefined;
  let endpointVersion = 0;
  let nextSequence = 1;
  let terminalSent = false;
  const callerAbort = new AbortController();
  const onResponseClose = () => {
    if (!reply.raw.writableEnded) callerAbort.abort();
  };
  reply.raw.once("close", onResponseClose);
  try {
    const attemptId = request.headers["x-attempt-id"];
    if (typeof attemptId !== "string" || !validCorrelationId(attemptId)) {
      throw new RuntimeError("INVALID_INPUT", "X-Attempt-Id is required for streaming requests.");
    }
    if (!validCorrelationId(request.id))
      throw new RuntimeError("INVALID_INPUT", "X-Request-Id is invalid for streaming requests.");
    const versionText = request.params.version;
    const version = versionText === undefined ? undefined : Number(versionText);
    if (version !== undefined && (!Number.isSafeInteger(version) || version < 1))
      throw new RuntimeError("VERSION_NOT_FOUND", "The requested Endpoint version was not found.");
    const base = {
      authorization: request.headers.authorization,
      ...(typeof request.headers["x-firebase-appcheck"] === "string"
        ? { appCheck: request.headers["x-firebase-appcheck"] }
        : {}),
      organizationSlug: request.params.organizationSlug,
      endpointSlug: request.params.endpointSlug,
      ...(version === undefined ? {} : { version }),
      requestId: request.id,
      attemptId,
    };
    const authorized = await service.resolveAuthorized(base);
    endpointVersion = authorized.endpoint.version.version;
    if (!service.supportsStreaming(authorized.endpoint))
      throw new RuntimeError("MODEL_ERROR", "Streaming is not configured for this Endpoint.");
    const parsed = await parseInvocationInput(
      request,
      authorized.endpoint.version.inputSchema,
      maximumImagePixels,
    );
    const handle = await service.openStream(
      { ...base, input: parsed.input, inputBytes: parsed.bytes },
      authorized,
      callerAbort.signal,
    );
    streamHandle = handle;
    const stream = handle.events;
    reply.raw.statusCode = 200;
    reply.raw.setHeader("content-type", "text/event-stream; charset=utf-8");
    reply.raw.setHeader("cache-control", "no-cache, no-store, no-transform");
    reply.raw.setHeader("connection", "keep-alive");
    reply.raw.setHeader("x-accel-buffering", "no");
    reply.raw.setHeader("x-request-id", request.id);
    reply.raw.setHeader("x-attempt-id", attemptId);
    reply.hijack();
    started = true;
    const write = async (
      event: string,
      type: string,
      data: Record<string, unknown>,
      terminal: boolean,
    ) => {
      if (nextSequence > 4096)
        throw new RuntimeError("MODEL_ERROR", "The stream exceeded its frame limit.");
      const invocationId = handle.invocationId;
      const sequence = nextSequence;
      const eventId = `${invocationId}:${sequence}`;
      await writeSse(
        reply,
        `event: ${event}\nid: ${eventId}\ndata: ${JSON.stringify({ contract_version: 1, endpoint_version: authorized.endpoint.version.version, request_id: request.id, attempt_id: attemptId, invocation_id: invocationId, event_id: eventId, sequence, type, terminal, ...data })}\n\n`,
      );
      nextSequence += 1;
      if (terminal) terminalSent = true;
    };
    await write("progress", "progress", {}, false);
    for await (const event of stream) {
      if (event.type === "delta")
        await write("text_delta", "text_delta", { text: event.content }, false);
      else await write("final", "final", { output: event.output }, true);
    }
    if (!reply.raw.destroyed && !reply.raw.writableEnded) reply.raw.end();
  } catch (error) {
    const normalized = normalizedRuntimeError(error);
    if (started) {
      if (!terminalSent) streamHandle?.abort();
      if (
        normalized.code === "REQUEST_CANCELLED" &&
        !reply.raw.destroyed &&
        !reply.raw.writableEnded
      )
        reply.raw.end();
      if (
        !terminalSent &&
        normalized.code !== "REQUEST_CANCELLED" &&
        !reply.raw.destroyed &&
        !reply.raw.writableEnded
      ) {
        const invocationId = streamHandle?.invocationId ?? null;
        const eventId = `${invocationId ?? "error"}:${nextSequence}`;
        try {
          await writeSse(
            reply,
            `event: error\nid: ${eventId}\ndata: ${JSON.stringify({ contract_version: 1, endpoint_version: endpointVersion, request_id: request.id, attempt_id: request.headers["x-attempt-id"] ?? null, invocation_id: invocationId, event_id: eventId, sequence: nextSequence, type: "error", terminal: true, error: { code: normalized.code, message: normalized.message } })}\n\n`,
          );
          terminalSent = true;
        } finally {
          if (!reply.raw.destroyed && !reply.raw.writableEnded) reply.raw.end();
        }
      }
      return;
    }
    return reply.status(runtimeStatus(normalized.code)).send({
      error: { code: normalized.code, message: normalized.message, request_id: request.id },
    });
  } finally {
    reply.raw.off("close", onResponseClose);
  }
}

async function cancelStreamHandler(
  request: FastifyRequest<{ Params: InvocationParams }>,
  reply: FastifyReply,
  service: InvocationService,
) {
  try {
    const attemptId = request.headers["x-attempt-id"];
    if (typeof attemptId !== "string" || !validCorrelationId(attemptId))
      throw new RuntimeError("INVALID_INPUT", "X-Attempt-Id is required for cancellation.");
    if (!validCorrelationId(request.id))
      throw new RuntimeError("INVALID_INPUT", "X-Request-Id is invalid for cancellation.");
    const versionText = request.params.version;
    const version = versionText === undefined ? undefined : Number(versionText);
    if (version !== undefined && (!Number.isSafeInteger(version) || version < 1))
      throw new RuntimeError("VERSION_NOT_FOUND", "The requested Endpoint version was not found.");
    await service.cancelStream({
      authorization: request.headers.authorization,
      ...(typeof request.headers["x-firebase-appcheck"] === "string"
        ? { appCheck: request.headers["x-firebase-appcheck"] }
        : {}),
      organizationSlug: request.params.organizationSlug,
      endpointSlug: request.params.endpointSlug,
      ...(version === undefined ? {} : { version }),
      requestId: request.id,
      attemptId,
    });
    return reply.status(202).send({
      status: "cancellation_requested",
      request_id: request.id,
      attempt_id: attemptId,
    });
  } catch (error) {
    const normalized = normalizedRuntimeError(error);
    return reply.status(runtimeStatus(normalized.code)).send({
      error: { code: normalized.code, message: normalized.message, request_id: request.id },
    });
  }
}

export async function registerInvocationRoutes(
  server: FastifyInstance,
  service: InvocationService,
  requestsPerMinute: number,
  maximumImagePixels: number,
): Promise<void> {
  const routeOptions = {
    config: { rateLimit: { max: requestsPerMinute, timeWindow: "1 minute" } },
  };
  server.post<{ Params: InvocationParams }>(
    "/v1/endpoints/:organizationSlug/:endpointSlug",
    routeOptions,
    (request, reply) => handler(request, reply, service, maximumImagePixels),
  );
  server.post<{ Params: InvocationParams }>(
    "/v1/endpoints/:organizationSlug/:endpointSlug/versions/:version",
    routeOptions,
    (request, reply) => handler(request, reply, service, maximumImagePixels),
  );
  server.post<{ Params: InvocationParams }>(
    "/v1/endpoints/:organizationSlug/:endpointSlug/stream",
    routeOptions,
    (request, reply) => streamHandler(request, reply, service, maximumImagePixels),
  );
  server.post<{ Params: InvocationParams }>(
    "/v1/endpoints/:organizationSlug/:endpointSlug/versions/:version/stream",
    routeOptions,
    (request, reply) => streamHandler(request, reply, service, maximumImagePixels),
  );
  server.delete<{ Params: InvocationParams }>(
    "/v1/endpoints/:organizationSlug/:endpointSlug/stream",
    routeOptions,
    (request, reply) => cancelStreamHandler(request, reply, service),
  );
  server.delete<{ Params: InvocationParams }>(
    "/v1/endpoints/:organizationSlug/:endpointSlug/versions/:version/stream",
    routeOptions,
    (request, reply) => cancelStreamHandler(request, reply, service),
  );
}
