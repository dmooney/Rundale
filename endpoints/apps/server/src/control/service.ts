import { issueApiKey } from "@parish/auth";
import {
  definitionContentHash,
  DomainError,
  type EndpointDefinition,
  type EndpointDraft,
  type EndpointVersionSnapshot,
  type Id,
} from "@parish/domain";
import { validateSchemaDefinition } from "@parish/schemas";
import type {
  ApiKeySummary,
  ControlRepository,
  CreatorPrincipal,
  EndpointRecord,
  InvocationSummary,
  ModelSummary,
  VersionSummary,
  UsageSummary,
} from "./contracts.js";

export class ControlError extends Error {
  constructor(
    public readonly code: "NOT_FOUND" | "CONFLICT" | "INVALID_DEFINITION" | "FORBIDDEN",
    message: string,
  ) {
    super(message);
    this.name = "ControlError";
  }
}

const slugPattern = /^[a-z0-9]+(?:-[a-z0-9]+)*$/;

export interface ControlServiceOptions {
  allowedModels: ReadonlySet<string>;
}

function validateDefinition(
  definition: EndpointDefinition,
  allowedModels: ReadonlySet<string>,
): void {
  validateSchemaDefinition(definition.inputSchema);
  validateSchemaDefinition(definition.outputSchema);
  if (definition.instructions.trim().length === 0 || definition.instructions.length > 32_000) {
    throw new ControlError(
      "INVALID_DEFINITION",
      "Instructions must contain 1 to 32,000 characters.",
    );
  }
  const modelKey = `${definition.providerConfig.provider}/${definition.providerConfig.model}`;
  if (!allowedModels.has(modelKey)) {
    throw new ControlError("INVALID_DEFINITION", `Model '${modelKey}' is not allowed.`);
  }
  if (
    !Number.isSafeInteger(definition.inferenceConfig.maxOutputTokens) ||
    definition.inferenceConfig.maxOutputTokens < 1 ||
    definition.inferenceConfig.maxOutputTokens > 4096
  ) {
    throw new ControlError("INVALID_DEFINITION", "maxOutputTokens must be between 1 and 4096.");
  }
  if (![0, 1].includes(definition.inferenceConfig.retryCount)) {
    throw new ControlError("INVALID_DEFINITION", "retryCount must be 0 or 1.");
  }
  const streaming = definition.inferenceConfig.streaming;
  if (
    streaming !== undefined &&
    (streaming.version !== 1 ||
      streaming.textField.trim().length === 0 ||
      streaming.textField.length > 128)
  ) {
    throw new ControlError(
      "INVALID_DEFINITION",
      "streaming must specify version 1 and a textField.",
    );
  }
  if (
    definition.inferenceConfig.temperature !== undefined &&
    (definition.inferenceConfig.temperature < 0 || definition.inferenceConfig.temperature > 2)
  ) {
    throw new ControlError("INVALID_DEFINITION", "temperature must be between 0 and 2.");
  }
  if (
    definition.providerConfig.provider === "google" &&
    definition.inferenceConfig.temperature !== undefined
  ) {
    throw new ControlError(
      "INVALID_DEFINITION",
      "temperature is not supported by the Google Interactions adapter.",
    );
  }
}

export class ControlService {
  constructor(
    private readonly repository: ControlRepository,
    private readonly options: ControlServiceOptions,
  ) {}

  listEndpoints(principal: CreatorPrincipal): Promise<EndpointRecord[]> {
    return this.repository.listEndpoints(principal.organizationId);
  }

  listModels(): ModelSummary[] {
    return [...this.options.allowedModels]
      .flatMap((modelKey): ModelSummary[] => {
        const separator = modelKey.indexOf("/");
        if (separator <= 0 || separator === modelKey.length - 1) return [];
        const provider = modelKey.slice(0, separator);
        if (provider !== "fake" && provider !== "openai" && provider !== "google") return [];
        return [{ provider, model: modelKey.slice(separator + 1) }];
      })
      .sort((left, right) =>
        `${left.provider}/${left.model}`.localeCompare(`${right.provider}/${right.model}`),
      );
  }

  async createEndpoint(
    principal: CreatorPrincipal,
    input: { name: string; slug: string; description?: string; definition: EndpointDefinition },
  ): Promise<{ endpoint: EndpointRecord; draft: EndpointDraft }> {
    if (input.name.trim().length === 0 || input.name.length > 120) {
      throw new ControlError(
        "INVALID_DEFINITION",
        "Endpoint name must contain 1 to 120 characters.",
      );
    }
    if (!slugPattern.test(input.slug) || input.slug.length > 80) {
      throw new ControlError("INVALID_DEFINITION", "Endpoint slug is invalid.");
    }
    validateDefinition(input.definition, this.options.allowedModels);
    return this.repository.createEndpoint(principal, {
      name: input.name.trim(),
      slug: input.slug,
      description: input.description?.trim() ?? "",
      definition: input.definition,
    });
  }

  async getDraft(principal: CreatorPrincipal, endpointId: Id): Promise<EndpointDraft> {
    const draft = await this.repository.getDraft(principal.organizationId, endpointId);
    if (draft === null) throw new ControlError("NOT_FOUND", "Endpoint draft was not found.");
    return draft;
  }

  async updateDraft(
    principal: CreatorPrincipal,
    endpointId: Id,
    expectedRevision: number,
    definition: EndpointDefinition,
  ): Promise<EndpointDraft> {
    validateDefinition(definition, this.options.allowedModels);
    const draft = await this.repository.updateDraft(
      principal,
      endpointId,
      expectedRevision,
      definition,
    );
    if (draft === null) throw new ControlError("CONFLICT", "Draft revision is stale or missing.");
    return draft;
  }

  async publish(
    principal: CreatorPrincipal,
    endpointId: Id,
    expectedRevision: number,
  ): Promise<EndpointVersionSnapshot> {
    const draft = await this.getDraft(principal, endpointId);
    if (draft.revision !== expectedRevision) {
      throw new ControlError("CONFLICT", "Draft revision is stale.");
    }
    validateDefinition(draft, this.options.allowedModels);
    const definition: EndpointDefinition = {
      inputSchema: draft.inputSchema,
      outputSchema: draft.outputSchema,
      instructions: draft.instructions,
      providerConfig: draft.providerConfig,
      inferenceConfig: draft.inferenceConfig,
    };
    const version = await this.repository.publish(principal, {
      endpointId,
      expectedRevision,
      definition,
      contentHash: definitionContentHash(definition),
    });
    if (version === null) throw new ControlError("CONFLICT", "Draft changed while publishing.");
    return version;
  }

  listVersions(principal: CreatorPrincipal, endpointId: Id): Promise<VersionSummary[]> {
    return this.repository.listVersions(principal.organizationId, endpointId);
  }

  async promoteProduction(
    principal: CreatorPrincipal,
    endpointId: Id,
    version: number,
    expectedRevision: number | null,
  ): Promise<{ version: number; revision: number }> {
    const alias = await this.repository.promoteProduction(
      principal,
      endpointId,
      version,
      expectedRevision,
    );
    if (alias === null) throw new ControlError("CONFLICT", "Version or alias revision is invalid.");
    return alias;
  }

  async createApiKey(
    principal: CreatorPrincipal,
    name: string,
    scopes: string[],
  ): Promise<{ secret: string; key: ApiKeySummary }> {
    if (name.trim().length === 0 || name.length > 120) {
      throw new ControlError(
        "INVALID_DEFINITION",
        "API key name must contain 1 to 120 characters.",
      );
    }
    if (
      scopes.length === 0 ||
      scopes.some((scope) => !/^invoke:endpoint:(?:\*|[a-z0-9]+(?:-[a-z0-9]+)*)$/.test(scope))
    ) {
      throw new ControlError("INVALID_DEFINITION", "API key scopes are invalid.");
    }
    const issued = issueApiKey();
    const key = await this.repository.createApiKey(principal, {
      name: name.trim(),
      keyPrefix: issued.prefix,
      keyDigest: issued.digest,
      scopes: [...new Set(scopes)],
    });
    return { secret: issued.secret, key };
  }

  listApiKeys(principal: CreatorPrincipal): Promise<ApiKeySummary[]> {
    return this.repository.listApiKeys(principal.organizationId);
  }

  async revokeApiKey(principal: CreatorPrincipal, keyId: Id): Promise<void> {
    if (!(await this.repository.revokeApiKey(principal, keyId))) {
      throw new ControlError("NOT_FOUND", "API key was not found.");
    }
  }

  listInvocations(principal: CreatorPrincipal, limit = 100): Promise<InvocationSummary[]> {
    return this.repository.listInvocations(
      principal.organizationId,
      Math.min(Math.max(limit, 1), 200),
    );
  }

  getUsage(principal: CreatorPrincipal): Promise<UsageSummary> {
    return this.repository.getUsage(principal.organizationId);
  }
}

export function normalizeControlError(error: unknown): ControlError {
  if (error instanceof ControlError) return error;
  if (error instanceof DomainError) {
    return new ControlError(
      error.code === "CONFLICT" ? "CONFLICT" : "INVALID_DEFINITION",
      error.message,
    );
  }
  return new ControlError("INVALID_DEFINITION", "The Endpoint definition is invalid.");
}
