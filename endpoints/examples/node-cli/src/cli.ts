#!/usr/bin/env node
import { readFile } from "node:fs/promises";
import { basename } from "node:path";
import { pathToFileURL } from "node:url";
import { compileSchema, formatValidationErrors } from "@parish/schemas";
import type { JsonSchema } from "@parish/domain";

interface CliOptions {
  endpointUrl: string;
  imagePath: string;
  apiKey: string;
  schemaPath?: string;
}

export class CliError extends Error {}

export function parseOptions(
  arguments_: string[],
  environment: NodeJS.ProcessEnv = process.env,
): CliOptions {
  const argumentsWithoutSeparator = arguments_[0] === "--" ? arguments_.slice(1) : arguments_;
  const values = new Map<string, string>();
  for (let index = 0; index < argumentsWithoutSeparator.length; index += 2) {
    const flag = argumentsWithoutSeparator[index];
    const value = argumentsWithoutSeparator[index + 1];
    if (flag === undefined || value === undefined || !flag.startsWith("--")) {
      throw new CliError("Usage: parish-invoke-image --image PATH --endpoint URL [--schema PATH]");
    }
    values.set(flag, value);
  }
  const imagePath = values.get("--image");
  const endpointUrl = values.get("--endpoint") ?? environment.PARISH_ENDPOINT_URL;
  const apiKey = environment.PARISH_API_KEY;
  if (imagePath === undefined || endpointUrl === undefined || apiKey === undefined) {
    throw new CliError(
      "Image and Endpoint URL are required; set PARISH_API_KEY in the environment (never pass it as an argument).",
    );
  }
  let parsedUrl: URL;
  try {
    parsedUrl = new URL(endpointUrl);
  } catch {
    throw new CliError("The Endpoint URL is invalid.");
  }
  if (!["https:", "http:"].includes(parsedUrl.protocol)) {
    throw new CliError("The Endpoint URL must use HTTP or HTTPS.");
  }
  return {
    imagePath,
    endpointUrl: parsedUrl.toString(),
    apiKey,
    ...(values.get("--schema") === undefined ? {} : { schemaPath: values.get("--schema")! }),
  };
}

async function loadOutputValidator(schemaPath: string) {
  const schema: unknown = JSON.parse(await readFile(schemaPath, "utf8"));
  if (schema === null || typeof schema !== "object" || Array.isArray(schema)) {
    throw new CliError("The output schema file must contain a JSON Schema object.");
  }
  return compileSchema(schema as JsonSchema);
}

function mediaType(bytes: Uint8Array): "image/jpeg" | "image/png" | "image/webp" {
  if (bytes[0] === 0xff && bytes[1] === 0xd8 && bytes[2] === 0xff) return "image/jpeg";
  if (
    bytes[0] === 0x89 &&
    bytes[1] === 0x50 &&
    bytes[2] === 0x4e &&
    bytes[3] === 0x47 &&
    bytes[4] === 0x0d &&
    bytes[5] === 0x0a &&
    bytes[6] === 0x1a &&
    bytes[7] === 0x0a
  ) {
    return "image/png";
  }
  if (
    String.fromCharCode(...bytes.slice(0, 4)) === "RIFF" &&
    String.fromCharCode(...bytes.slice(8, 12)) === "WEBP"
  ) {
    return "image/webp";
  }
  throw new CliError("The image must be JPEG, PNG, or WEBP.");
}

export async function invokeImage(
  options: CliOptions,
  fetcher: typeof fetch = fetch,
): Promise<unknown> {
  const validate =
    options.schemaPath === undefined ? undefined : await loadOutputValidator(options.schemaPath);
  const bytes = await readFile(options.imagePath);
  const type = mediaType(bytes);
  const form = new FormData();
  form.set("input", "{}");
  form.set("image", new File([bytes], basename(options.imagePath), { type }));
  const response = await fetcher(options.endpointUrl, {
    method: "POST",
    headers: { authorization: `Bearer ${options.apiKey}` },
    body: form,
  });
  let payload: unknown;
  try {
    payload = await response.json();
  } catch {
    throw new CliError(`Parish returned a non-JSON response (HTTP ${response.status}).`);
  }
  if (!response.ok) {
    const publicError =
      payload !== null && typeof payload === "object" && "error" in payload
        ? JSON.stringify((payload as { error: unknown }).error)
        : `HTTP ${response.status}`;
    throw new CliError(`Endpoint invocation failed: ${publicError}`);
  }
  if (validate !== undefined && !validate(payload)) {
    throw new CliError(
      `Response did not match the supplied output schema: ${JSON.stringify(formatValidationErrors(validate.errors))}`,
    );
  }
  return payload;
}

export async function main(arguments_ = process.argv.slice(2)): Promise<void> {
  try {
    const result = await invokeImage(parseOptions(arguments_));
    process.stdout.write(`${JSON.stringify(result, null, 2)}\n`);
  } catch (error) {
    const message = error instanceof Error ? error.message : "Invocation failed.";
    process.stderr.write(`${message}\n`);
    process.exitCode = 1;
  }
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? "").href) await main();
