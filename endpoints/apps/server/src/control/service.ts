import { issueApiKey } from "@limerick/auth";
import {
  definitionContentHash,
  DomainError,
  isProviderId,
  type EndpointDefinition,
  type EndpointDraft,
  type EndpointVersionSnapshot,
  type Id,
} from "@limerick/domain";
import { validateSchemaDefinition } from "@limerick/schemas";
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

export function validateDefinition(
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
  validateDecision(definition);
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

const decisionProviders = new Set(["fake", "openrouter", "typesafe"]);
const decisionNamePattern = /^[a-z][a-z0-9_]{0,63}$/;

function invalid(message: string): never {
  throw new ControlError("INVALID_DEFINITION", message);
}

function boundedText(value: unknown, label: string, max = 4_000): string {
  if (typeof value !== "string" || value.trim().length === 0 || value.length > max)
    invalid(`${label} must contain 1 to ${max} characters.`);
  return value;
}

/** Text or JSON structure (ADR 014), non-empty and at most `max` characters as JSON. */
function boundedStructure(value: unknown, label: string, max = 4_000): void {
  if (typeof value === "string") {
    boundedText(value, label, max);
    return;
  }
  // The Decisions schema takes text, an object or an array here.
  const json = typeof value === "object" && value !== null ? JSON.stringify(value) : undefined;
  if (json === undefined || json.length > max || json === "{}" || json === "[]")
    invalid(`${label} must be text, or a JSON object or array of 1 to ${max} characters.`);
}

function fieldList(value: unknown, label: string): void {
  if (
    !Array.isArray(value) ||
    value.length === 0 ||
    value.length > 16 ||
    value.some((field) => typeof field !== "string" || field.length === 0)
  )
    invalid(`${label} must list 1 to 16 field names.`);
}

/**
 * A decision Endpoint (ADR 014) runs on a decision model, names input fields
 * its schema declares, and asks well-formed choice or noul questions.
 */
function validateDecision(definition: EndpointDefinition): void {
  const spec: unknown = definition.inferenceConfig.decision;
  const provider = definition.providerConfig.provider;
  if (spec === undefined) {
    if (provider === "openrouter" || provider === "typesafe")
      invalid(`Provider '${provider}' serves decision Endpoints only.`);
    return;
  }
  if (!decisionProviders.has(provider))
    invalid(`Provider '${provider}' cannot serve a decision Endpoint.`);
  if (definition.inferenceConfig.streaming !== undefined)
    invalid("A decision Endpoint cannot stream.");
  if (definition.inferenceConfig.temperature !== undefined)
    invalid("A decision Endpoint has no temperature.");
  if (spec === null || typeof spec !== "object" || Array.isArray(spec))
    invalid("decision must be an object.");
  const value = spec as Record<string, unknown>;
  const allowedKeys = ["version", "stateFields", "idField", "questions"];
  if (Object.keys(value).some((key) => !allowedKeys.includes(key)))
    invalid(`decision may contain only ${allowedKeys.join(", ")}.`);
  if (value.version !== 1) invalid("decision.version must be 1.");
  const properties = (definition.inputSchema.properties ?? {}) as Record<string, unknown>;
  const field = (name: unknown, label: string, array: boolean): void => {
    if (typeof name !== "string" || !Object.hasOwn(properties, name))
      invalid(`${label} must name an input field.`);
    const schema = properties[name] as Record<string, unknown> | undefined;
    if (array && schema?.type !== "array") invalid(`${label} must name an array input field.`);
  };
  if (
    !Array.isArray(value.stateFields) ||
    value.stateFields.length === 0 ||
    value.stateFields.length > 32
  )
    invalid("decision.stateFields must list 1 to 32 input fields.");
  for (const name of value.stateFields) field(name, "decision.stateFields", false);
  boundedText(value.idField, "decision.idField", 64);
  const questions = value.questions;
  if (questions === null || typeof questions !== "object" || Array.isArray(questions))
    invalid("decision.questions must be an object.");
  const entries = Object.entries(questions as Record<string, unknown>);
  if (entries.length === 0 || entries.length > 32)
    invalid("decision.questions must hold 1 to 32 questions.");
  for (const [name, raw] of entries) {
    const label = `decision.questions.${name}`;
    if (!decisionNamePattern.test(name) || name.includes("__"))
      invalid(`${label}: question names are lowercase letters, digits and single '_'.`);
    if (raw === null || typeof raw !== "object" || Array.isArray(raw))
      invalid(`${label} must be an object.`);
    const question = raw as Record<string, unknown>;
    boundedStructure(question.instructions, `${label}.instructions`);
    if (question.type === "choice") {
      const keys = ["type", "instructions", "criteria", "criteriaFrom"];
      if (Object.keys(question).some((key) => !keys.includes(key)))
        invalid(`${label} may contain only ${keys.join(", ")}.`);
      const criteria = question.criteria ?? {};
      if (criteria === null || typeof criteria !== "object" || Array.isArray(criteria))
        invalid(`${label}.criteria must be an object.`);
      for (const [id, text] of Object.entries(criteria)) {
        if (!/^[A-Za-z0-9_-]{1,64}$/.test(id)) invalid(`${label}.criteria has an invalid id.`);
        boundedStructure(text, `${label}.criteria.${id}`);
      }
      const sources = question.criteriaFrom ?? [];
      if (!Array.isArray(sources)) invalid(`${label}.criteriaFrom must be an array.`);
      for (const source of sources as unknown[]) {
        const entry = source as Record<string, unknown> | null;
        if (entry === null || typeof entry !== "object")
          invalid(`${label}.criteriaFrom entries must be objects.`);
        field(entry.field, `${label}.criteriaFrom.field`, true);
        if (entry.criterion === undefined && entry.include === undefined)
          invalid(`${label}.criteriaFrom entries need a criterion or include.`);
        if (entry.criterion !== undefined)
          boundedStructure(entry.criterion, `${label}.criteriaFrom.criterion`);
        if (entry.include !== undefined) fieldList(entry.include, `${label}.criteriaFrom.include`);
      }
      if (Object.keys(criteria).length + (sources as unknown[]).length === 0)
        invalid(`${label} offers no options.`);
    } else if (question.type === "noul") {
      const keys = ["type", "instructions", "criteria", "forEach", "include"];
      if (Object.keys(question).some((key) => !keys.includes(key)))
        invalid(`${label} may contain only ${keys.join(", ")}.`);
      const criteria = question.criteria as Record<string, unknown> | null | undefined;
      if (criteria === null || typeof criteria !== "object" || Object.keys(criteria).length !== 2)
        invalid(`${label}.criteria must describe true and false.`);
      boundedStructure(criteria.true, `${label}.criteria.true`);
      boundedStructure(criteria.false, `${label}.criteria.false`);
      if (question.forEach !== undefined) field(question.forEach, `${label}.forEach`, true);
      if (question.include !== undefined) {
        if (question.forEach === undefined) invalid(`${label}.include needs forEach.`);
        fieldList(question.include, `${label}.include`);
      }
    } else {
      invalid(`${label}.type must be choice or noul.`);
    }
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
        if (!isProviderId(provider)) return [];
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
