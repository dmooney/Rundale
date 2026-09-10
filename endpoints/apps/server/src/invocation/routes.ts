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
  for await (const part of request.parts()) {
    if (part.type === "field") {
      if (part.fieldname !== "input" || typeof part.value !== "string") {
        throw new RuntimeError("INVALID_INPUT", "Multipart fields must use one JSON 'input' part.");
      }
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
}
