import { createHash } from "node:crypto";

export type Id = string;
export type JsonSchema = Record<string, unknown>;
export type ProviderId = "fake" | "openai" | "google";
export type OrganizationStatus = "active" | "suspended";
export type EndpointStatus = "active" | "disabled";
export type ApiKeyStatus = "active" | "revoked";
export type MemberRole = "owner" | "admin" | "developer" | "viewer";

export interface Organization {
  id: Id;
  name: string;
  slug: string;
  status: OrganizationStatus;
  dailyInvocationQuota: number;
}

export interface Endpoint {
  id: Id;
  organizationId: Id;
  name: string;
  slug: string;
  description: string;
  status: EndpointStatus;
}

export interface ProviderConfig {
  provider: ProviderId;
  model: string;
}

export interface InferenceConfig {
  temperature?: number;
  maxOutputTokens: number;
  retryCount: 0 | 1;
}

export interface EndpointDefinition {
  inputSchema: JsonSchema;
  outputSchema: JsonSchema;
  instructions: string;
  providerConfig: ProviderConfig;
  inferenceConfig: InferenceConfig;
}

export interface EndpointDraft extends EndpointDefinition {
  id: Id;
  endpointId: Id;
  revision: number;
  updatedBy: Id;
  updatedAt: Date;
}

export interface EndpointVersionSnapshot extends EndpointDefinition {
  id: Id;
  endpointId: Id;
  organizationId: Id;
  version: number;
  contentHash: string;
  publishedBy: Id;
  publishedAt: Date;
}

export interface DeploymentAlias {
  id: Id;
  endpointId: Id;
  alias: "production";
  endpointVersionId: Id;
  revision: number;
}

export class DomainError extends Error {
  constructor(
    public readonly code: "CONFLICT" | "INVALID_DEFINITION" | "IMMUTABLE_VERSION",
    message: string,
  ) {
    super(message);
    this.name = "DomainError";
  }
}

export function assertDraftRevision(current: number, expected: number): void {
  if (current !== expected) {
    throw new DomainError(
      "CONFLICT",
      `Draft revision ${expected} is stale; current revision is ${current}.`,
    );
  }
}

export function assertAliasRevision(current: number, expected: number): void {
  if (current !== expected) {
    throw new DomainError(
      "CONFLICT",
      `Alias revision ${expected} is stale; current revision is ${current}.`,
    );
  }
}

function canonicalize(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(canonicalize);
  if (value !== null && typeof value === "object") {
    return Object.fromEntries(
      Object.entries(value)
        .sort(([left], [right]) => left.localeCompare(right))
        .map(([key, child]) => [key, canonicalize(child)]),
    );
  }
  return value;
}

export function canonicalDefinition(definition: EndpointDefinition): string {
  return JSON.stringify(
    canonicalize({
      inputSchema: definition.inputSchema,
      outputSchema: definition.outputSchema,
      instructions: definition.instructions,
      providerConfig: definition.providerConfig,
      inferenceConfig: definition.inferenceConfig,
    }),
  );
}

export function definitionContentHash(definition: EndpointDefinition): string {
  return `sha256:${createHash("sha256").update(canonicalDefinition(definition)).digest("hex")}`;
}

export function snapshotDraft(
  draft: EndpointDraft,
  organizationId: Id,
  version: number,
  publishedBy: Id,
  id: Id,
  now = new Date(),
): EndpointVersionSnapshot {
  if (version < 1 || !Number.isInteger(version)) {
    throw new DomainError("INVALID_DEFINITION", "Published version numbers start at 1.");
  }
  const definition: EndpointDefinition = {
    inputSchema: structuredClone(draft.inputSchema),
    outputSchema: structuredClone(draft.outputSchema),
    instructions: draft.instructions,
    providerConfig: structuredClone(draft.providerConfig),
    inferenceConfig: structuredClone(draft.inferenceConfig),
  };
  return Object.freeze({
    id,
    endpointId: draft.endpointId,
    organizationId,
    version,
    ...definition,
    contentHash: definitionContentHash(definition),
    publishedBy,
    publishedAt: now,
  });
}
