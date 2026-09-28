import {
  snapshotDraft,
  type EndpointDefinition,
  type EndpointDraft,
  type EndpointVersionSnapshot,
} from "@parish/domain";
import type {
  ApiKeySummary,
  ControlRepository,
  CreatorPrincipal,
  EndpointRecord,
  InvocationSummary,
  PublishInput,
  VersionSummary,
  UsageSummary,
} from "../src/control/contracts.js";

export class MemoryControlRepository implements ControlRepository {
  private sequence = 0;
  private readonly endpoints = new Map<string, EndpointRecord>();
  private readonly drafts = new Map<string, EndpointDraft>();
  private readonly versions = new Map<string, EndpointVersionSnapshot[]>();
  private readonly aliases = new Map<string, { version: number; revision: number }>();
  private readonly keys: ApiKeySummary[] = [];
  private readonly keyOrganizations = new Map<string, string>();
  private readonly invocationRows: Array<{
    organizationId: string;
    summary: InvocationSummary;
  }> = [];

  private id(prefix: string): string {
    this.sequence += 1;
    return `${prefix}_${this.sequence}`;
  }

  async listEndpoints(organizationId: string): Promise<EndpointRecord[]> {
    return [...this.endpoints.values()].filter(
      (endpoint) => endpoint.organizationId === organizationId,
    );
  }

  async getEndpoint(organizationId: string, endpointId: string): Promise<EndpointRecord | null> {
    const endpoint = this.endpoints.get(endpointId);
    return endpoint?.organizationId === organizationId ? endpoint : null;
  }

  async createEndpoint(
    principal: CreatorPrincipal,
    input: { name: string; slug: string; description: string; definition: EndpointDefinition },
  ): Promise<{ endpoint: EndpointRecord; draft: EndpointDraft }> {
    const now = new Date();
    const endpoint: EndpointRecord = {
      id: this.id("endpoint"),
      organizationId: principal.organizationId,
      name: input.name,
      slug: input.slug,
      description: input.description,
      status: "active",
      inferenceEnabled: true,
      organizationStatus: "active",
      organizationInferenceEnabled: true,
      createdAt: now,
      updatedAt: now,
    };
    const draft: EndpointDraft = {
      id: this.id("draft"),
      endpointId: endpoint.id,
      revision: 1,
      ...structuredClone(input.definition),
      updatedBy: principal.userId,
      updatedAt: now,
    };
    this.endpoints.set(endpoint.id, endpoint);
    this.drafts.set(endpoint.id, draft);
    return { endpoint, draft };
  }

  async getDraft(organizationId: string, endpointId: string): Promise<EndpointDraft | null> {
    return (await this.getEndpoint(organizationId, endpointId)) === null
      ? null
      : (this.drafts.get(endpointId) ?? null);
  }

  async updateDraft(
    principal: CreatorPrincipal,
    endpointId: string,
    expectedRevision: number,
    definition: EndpointDefinition,
  ): Promise<EndpointDraft | null> {
    const draft = await this.getDraft(principal.organizationId, endpointId);
    if (draft === null || draft.revision !== expectedRevision) return null;
    const updated: EndpointDraft = {
      ...draft,
      ...structuredClone(definition),
      revision: draft.revision + 1,
      updatedBy: principal.userId,
      updatedAt: new Date(),
    };
    this.drafts.set(endpointId, updated);
    return updated;
  }

  async publish(
    principal: CreatorPrincipal,
    input: PublishInput,
  ): Promise<EndpointVersionSnapshot | null> {
    const draft = await this.getDraft(principal.organizationId, input.endpointId);
    if (draft === null || draft.revision !== input.expectedRevision) return null;
    const versions = this.versions.get(input.endpointId) ?? [];
    const snapshot = snapshotDraft(
      draft,
      principal.organizationId,
      versions.length + 1,
      principal.userId,
      this.id("version"),
    );
    versions.push(snapshot);
    this.versions.set(input.endpointId, versions);
    return snapshot;
  }

  async listVersions(organizationId: string, endpointId: string): Promise<VersionSummary[]> {
    if ((await this.getEndpoint(organizationId, endpointId)) === null) return [];
    const alias = this.aliases.get(endpointId);
    return (this.versions.get(endpointId) ?? []).map((version) => ({
      id: version.id,
      version: version.version,
      contentHash: version.contentHash,
      publishedAt: version.publishedAt,
      isProduction: alias?.version === version.version,
      productionAliasRevision: alias?.revision ?? null,
    }));
  }

  async getVersion(
    organizationId: string,
    endpointId: string,
    version: number,
  ): Promise<EndpointVersionSnapshot | null> {
    if ((await this.getEndpoint(organizationId, endpointId)) === null) return null;
    return this.versions.get(endpointId)?.find((item) => item.version === version) ?? null;
  }

  async promoteProduction(
    principal: CreatorPrincipal,
    endpointId: string,
    version: number,
    expectedRevision: number | null,
  ): Promise<{ version: number; revision: number } | null> {
    if ((await this.getVersion(principal.organizationId, endpointId, version)) === null)
      return null;
    const current = this.aliases.get(endpointId);
    if ((current?.revision ?? null) !== expectedRevision) return null;
    const alias = { version, revision: (current?.revision ?? 0) + 1 };
    this.aliases.set(endpointId, alias);
    return alias;
  }

  async createApiKey(
    principal: CreatorPrincipal,
    input: { name: string; keyPrefix: string; keyDigest: string; scopes: string[] },
  ): Promise<ApiKeySummary> {
    const key: ApiKeySummary = {
      id: this.id("key"),
      name: input.name,
      keyPrefix: input.keyPrefix,
      scopes: input.scopes,
      status: "active",
      lastUsedAt: null,
      createdAt: new Date(),
      revokedAt: null,
    };
    this.keys.push(key);
    this.keyOrganizations.set(key.id, principal.organizationId);
    return key;
  }

  async listApiKeys(organizationId: string): Promise<ApiKeySummary[]> {
    return this.keys.filter((key) => this.keyOrganizations.get(key.id) === organizationId);
  }

  async revokeApiKey(principal: CreatorPrincipal, keyId: string): Promise<boolean> {
    const key = this.keys.find((item) => item.id === keyId);
    if (key === undefined || this.keyOrganizations.get(keyId) !== principal.organizationId) {
      return false;
    }
    key.status = "revoked";
    key.revokedAt = new Date();
    return true;
  }

  addInvocation(organizationId: string, summary: InvocationSummary): void {
    this.invocationRows.push({ organizationId, summary });
  }

  async listInvocations(organizationId: string, limit: number): Promise<InvocationSummary[]> {
    return this.invocationRows
      .filter((row) => row.organizationId === organizationId)
      .slice(0, limit)
      .map((row) => row.summary);
  }

  async getUsage(_organizationId: string): Promise<UsageSummary> {
    void _organizationId;
    return {
      invocations: 0,
      succeeded: 0,
      failed: 0,
      totalTokens: 0,
      estimatedProviderCost: "0.000000",
    };
  }
}
