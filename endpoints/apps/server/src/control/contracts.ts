import type {
  EndpointDefinition,
  EndpointDraft,
  EndpointVersionSnapshot,
  Id,
  ProviderId,
} from "@parish/domain";

export interface CreatorPrincipal {
  userId: Id;
  organizationId: Id;
  role: "owner";
}

export interface EndpointRecord {
  id: Id;
  organizationId: Id;
  name: string;
  slug: string;
  description: string;
  status: "active" | "disabled";
  inferenceEnabled: boolean;
  organizationStatus: "active" | "suspended";
  organizationInferenceEnabled: boolean;
  createdAt: Date;
  updatedAt: Date;
}

export interface ModelSummary {
  provider: ProviderId;
  model: string;
}

export interface VersionSummary {
  id: Id;
  version: number;
  contentHash: string;
  publishedAt: Date;
  isProduction: boolean;
  productionAliasRevision: number | null;
}

export interface ApiKeySummary {
  id: Id;
  name: string;
  keyPrefix: string;
  scopes: string[];
  status: "active" | "revoked";
  lastUsedAt: Date | null;
  createdAt: Date;
  revokedAt: Date | null;
}

export interface InvocationSummary {
  id: Id;
  requestId: string;
  endpointId: Id;
  endpointVersionId: Id | null;
  endpointVersionNumber: number | null;
  endpointDraftId: Id | null;
  status: "running" | "succeeded" | "failed";
  isTest: boolean;
  provider: string;
  model: string;
  durationMs: number | null;
  estimatedProviderCost: string | null;
  validationStatus: "pending" | "valid" | "invalid";
  errorCode: string | null;
  startedAt: Date;
}

export interface UsageSummary {
  invocations: number;
  succeeded: number;
  failed: number;
  totalTokens: number;
  estimatedProviderCost: string;
}

export interface PublishInput {
  endpointId: Id;
  expectedRevision: number;
  definition: EndpointDefinition;
  contentHash: string;
}

export interface ControlRepository {
  listEndpoints(organizationId: Id): Promise<EndpointRecord[]>;
  getEndpoint(organizationId: Id, endpointId: Id): Promise<EndpointRecord | null>;
  createEndpoint(
    principal: CreatorPrincipal,
    input: { name: string; slug: string; description: string; definition: EndpointDefinition },
  ): Promise<{ endpoint: EndpointRecord; draft: EndpointDraft }>;
  getDraft(organizationId: Id, endpointId: Id): Promise<EndpointDraft | null>;
  updateDraft(
    principal: CreatorPrincipal,
    endpointId: Id,
    expectedRevision: number,
    definition: EndpointDefinition,
  ): Promise<EndpointDraft | null>;
  publish(
    principal: CreatorPrincipal,
    input: PublishInput,
  ): Promise<EndpointVersionSnapshot | null>;
  listVersions(organizationId: Id, endpointId: Id): Promise<VersionSummary[]>;
  getVersion(
    organizationId: Id,
    endpointId: Id,
    version: number,
  ): Promise<EndpointVersionSnapshot | null>;
  promoteProduction(
    principal: CreatorPrincipal,
    endpointId: Id,
    version: number,
    expectedRevision: number | null,
  ): Promise<{ version: number; revision: number } | null>;
  createApiKey(
    principal: CreatorPrincipal,
    input: { name: string; keyPrefix: string; keyDigest: string; scopes: string[] },
  ): Promise<ApiKeySummary>;
  listApiKeys(organizationId: Id): Promise<ApiKeySummary[]>;
  revokeApiKey(principal: CreatorPrincipal, keyId: Id): Promise<boolean>;
  listInvocations(organizationId: Id, limit: number): Promise<InvocationSummary[]>;
  getUsage(organizationId: Id): Promise<UsageSummary>;
}
