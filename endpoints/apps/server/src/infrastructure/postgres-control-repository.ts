import { and, desc, eq, max, sql } from "drizzle-orm";
import {
  apiKeys,
  auditEvents,
  deploymentAliases,
  endpointDrafts,
  endpoints,
  endpointVersions,
  invocations,
  organizations,
  type Database,
} from "@parish/database";
import type { EndpointDraft, EndpointVersionSnapshot } from "@parish/domain";
import type {
  ApiKeySummary,
  ControlRepository,
  CreatorPrincipal,
  EndpointRecord,
  InvocationSummary,
  PublishInput,
  VersionSummary,
  UsageSummary,
} from "../control/contracts.js";

type EndpointRow = typeof endpoints.$inferSelect;
type OrganizationRow = typeof organizations.$inferSelect;
type DraftRow = typeof endpointDrafts.$inferSelect;
type VersionRow = typeof endpointVersions.$inferSelect;
type ApiKeyRow = typeof apiKeys.$inferSelect;
type InvocationRow = typeof invocations.$inferSelect;

function endpointRecord(
  row: EndpointRow,
  organization: Pick<OrganizationRow, "status" | "inferenceEnabled">,
): EndpointRecord {
  return {
    id: row.id,
    organizationId: row.organizationId,
    name: row.name,
    slug: row.slug,
    description: row.description,
    status: row.status,
    inferenceEnabled: row.inferenceEnabled,
    organizationStatus: organization.status,
    organizationInferenceEnabled: organization.inferenceEnabled,
    createdAt: row.createdAt,
    updatedAt: row.updatedAt,
  };
}

function draftRecord(row: DraftRow): EndpointDraft {
  return {
    id: row.id,
    endpointId: row.endpointId,
    revision: row.revision,
    inputSchema: row.inputSchema,
    outputSchema: row.outputSchema,
    instructions: row.instructions,
    providerConfig: row.providerConfig,
    inferenceConfig: row.inferenceConfig,
    updatedBy: row.updatedBy,
    updatedAt: row.updatedAt,
  };
}

function versionRecord(row: VersionRow, organizationId: string): EndpointVersionSnapshot {
  return {
    id: row.id,
    endpointId: row.endpointId,
    organizationId,
    version: row.version,
    contentHash: row.contentHash,
    inputSchema: row.inputSchema,
    outputSchema: row.outputSchema,
    instructions: row.instructions,
    providerConfig: row.providerConfig,
    inferenceConfig: row.inferenceConfig,
    publishedBy: row.publishedBy,
    publishedAt: row.publishedAt,
  };
}

function keySummary(row: ApiKeyRow): ApiKeySummary {
  return {
    id: row.id,
    name: row.name,
    keyPrefix: row.keyPrefix,
    scopes: row.scopes,
    status: row.status,
    lastUsedAt: row.lastUsedAt,
    createdAt: row.createdAt,
    revokedAt: row.revokedAt,
  };
}

function invocationSummary(
  row: InvocationRow,
  endpointVersionNumber: number | null,
): InvocationSummary {
  return {
    id: row.id,
    requestId: row.requestId,
    endpointId: row.endpointId,
    endpointVersionId: row.endpointVersionId,
    endpointVersionNumber,
    endpointDraftId: row.endpointDraftId,
    status: row.status,
    isTest: row.isTest,
    provider: row.provider,
    model: row.model,
    durationMs: row.durationMs,
    estimatedProviderCost: row.estimatedProviderCost,
    validationStatus: row.validationStatus,
    errorCode: row.errorCode,
    startedAt: row.startedAt,
  };
}

export class PostgresControlRepository implements ControlRepository {
  constructor(private readonly database: Database) {}

  async listEndpoints(organizationId: string): Promise<EndpointRecord[]> {
    const rows = await this.database
      .select({ endpoint: endpoints, organization: organizations })
      .from(endpoints)
      .innerJoin(organizations, eq(endpoints.organizationId, organizations.id))
      .where(eq(endpoints.organizationId, organizationId))
      .orderBy(desc(endpoints.updatedAt));
    return rows.map(({ endpoint, organization }) => endpointRecord(endpoint, organization));
  }

  async getEndpoint(organizationId: string, endpointId: string): Promise<EndpointRecord | null> {
    const [row] = await this.database
      .select({ endpoint: endpoints, organization: organizations })
      .from(endpoints)
      .innerJoin(organizations, eq(endpoints.organizationId, organizations.id))
      .where(and(eq(endpoints.id, endpointId), eq(endpoints.organizationId, organizationId)))
      .limit(1);
    return row === undefined ? null : endpointRecord(row.endpoint, row.organization);
  }

  async createEndpoint(
    principal: CreatorPrincipal,
    input: Parameters<ControlRepository["createEndpoint"]>[1],
  ): Promise<{ endpoint: EndpointRecord; draft: EndpointDraft }> {
    return this.database.transaction(async (transaction) => {
      const [endpoint] = await transaction
        .insert(endpoints)
        .values({
          organizationId: principal.organizationId,
          name: input.name,
          slug: input.slug,
          description: input.description,
        })
        .returning();
      if (endpoint === undefined) throw new Error("Endpoint insert did not return a row.");
      const [draft] = await transaction
        .insert(endpointDrafts)
        .values({
          endpointId: endpoint.id,
          ...input.definition,
          updatedBy: principal.userId,
        })
        .returning();
      if (draft === undefined) throw new Error("Draft insert did not return a row.");
      const [organization] = await transaction
        .select({ status: organizations.status, inferenceEnabled: organizations.inferenceEnabled })
        .from(organizations)
        .where(eq(organizations.id, principal.organizationId))
        .limit(1);
      if (organization === undefined) throw new Error("Organization did not return a row.");
      await transaction.insert(auditEvents).values({
        organizationId: principal.organizationId,
        actorUserId: principal.userId,
        action: "endpoint.created",
        resourceType: "endpoint",
        resourceId: endpoint.id,
        metadata: { slug: endpoint.slug },
      });
      return { endpoint: endpointRecord(endpoint, organization), draft: draftRecord(draft) };
    });
  }

  async getDraft(organizationId: string, endpointId: string): Promise<EndpointDraft | null> {
    const [result] = await this.database
      .select({ draft: endpointDrafts })
      .from(endpointDrafts)
      .innerJoin(endpoints, eq(endpointDrafts.endpointId, endpoints.id))
      .where(
        and(
          eq(endpointDrafts.endpointId, endpointId),
          eq(endpoints.organizationId, organizationId),
        ),
      )
      .limit(1);
    return result === undefined ? null : draftRecord(result.draft);
  }

  async updateDraft(
    principal: CreatorPrincipal,
    endpointId: string,
    expectedRevision: number,
    definition: Parameters<ControlRepository["updateDraft"]>[3],
  ): Promise<EndpointDraft | null> {
    const tenantEndpoint = await this.getEndpoint(principal.organizationId, endpointId);
    if (tenantEndpoint === null) return null;
    return this.database.transaction(async (transaction) => {
      const [draft] = await transaction
        .update(endpointDrafts)
        .set({
          ...definition,
          revision: sql`${endpointDrafts.revision} + 1`,
          updatedBy: principal.userId,
          updatedAt: new Date(),
        })
        .where(
          and(
            eq(endpointDrafts.endpointId, endpointId),
            eq(endpointDrafts.revision, expectedRevision),
          ),
        )
        .returning();
      if (draft === undefined) return null;
      await transaction.insert(auditEvents).values({
        organizationId: principal.organizationId,
        actorUserId: principal.userId,
        action: "draft.updated",
        resourceType: "endpoint",
        resourceId: endpointId,
        metadata: { revision: draft.revision },
      });
      return draftRecord(draft);
    });
  }

  async publish(
    principal: CreatorPrincipal,
    input: PublishInput,
  ): Promise<EndpointVersionSnapshot | null> {
    return this.database.transaction(async (transaction) => {
      const [current] = await transaction
        .select({ revision: endpointDrafts.revision })
        .from(endpointDrafts)
        .innerJoin(endpoints, eq(endpointDrafts.endpointId, endpoints.id))
        .where(
          and(
            eq(endpointDrafts.endpointId, input.endpointId),
            eq(endpoints.organizationId, principal.organizationId),
          ),
        )
        .for("update")
        .limit(1);
      if (current?.revision !== input.expectedRevision) return null;
      const [aggregate] = await transaction
        .select({ maximum: max(endpointVersions.version) })
        .from(endpointVersions)
        .where(eq(endpointVersions.endpointId, input.endpointId));
      const nextVersion = (aggregate?.maximum ?? 0) + 1;
      const [version] = await transaction
        .insert(endpointVersions)
        .values({
          endpointId: input.endpointId,
          version: nextVersion,
          contentHash: input.contentHash,
          ...input.definition,
          publishedBy: principal.userId,
        })
        .returning();
      if (version === undefined) throw new Error("Version insert did not return a row.");
      await transaction.insert(auditEvents).values({
        organizationId: principal.organizationId,
        actorUserId: principal.userId,
        action: "endpoint.version.published",
        resourceType: "endpoint_version",
        resourceId: version.id,
        metadata: {
          endpointId: input.endpointId,
          version: nextVersion,
          contentHash: input.contentHash,
        },
      });
      return versionRecord(version, principal.organizationId);
    });
  }

  async listVersions(organizationId: string, endpointId: string): Promise<VersionSummary[]> {
    const rows = await this.database
      .select({
        version: endpointVersions,
        aliasVersionId: deploymentAliases.endpointVersionId,
        aliasRevision: deploymentAliases.revision,
      })
      .from(endpointVersions)
      .innerJoin(endpoints, eq(endpointVersions.endpointId, endpoints.id))
      .leftJoin(
        deploymentAliases,
        and(
          eq(deploymentAliases.endpointId, endpointVersions.endpointId),
          eq(deploymentAliases.alias, "production"),
        ),
      )
      .where(
        and(
          eq(endpointVersions.endpointId, endpointId),
          eq(endpoints.organizationId, organizationId),
        ),
      )
      .orderBy(desc(endpointVersions.version));
    return rows.map(({ version, aliasVersionId, aliasRevision }) => ({
      id: version.id,
      version: version.version,
      contentHash: version.contentHash,
      publishedAt: version.publishedAt,
      isProduction: aliasVersionId === version.id,
      productionAliasRevision: aliasRevision,
    }));
  }

  async getVersion(
    organizationId: string,
    endpointId: string,
    versionNumber: number,
  ): Promise<EndpointVersionSnapshot | null> {
    const [result] = await this.database
      .select({ version: endpointVersions })
      .from(endpointVersions)
      .innerJoin(endpoints, eq(endpointVersions.endpointId, endpoints.id))
      .where(
        and(
          eq(endpointVersions.endpointId, endpointId),
          eq(endpointVersions.version, versionNumber),
          eq(endpoints.organizationId, organizationId),
        ),
      )
      .limit(1);
    return result === undefined ? null : versionRecord(result.version, organizationId);
  }

  async promoteProduction(
    principal: CreatorPrincipal,
    endpointId: string,
    versionNumber: number,
    expectedRevision: number | null,
  ): Promise<{ version: number; revision: number } | null> {
    return this.database.transaction(async (transaction) => {
      const [endpoint] = await transaction
        .select({ id: endpoints.id })
        .from(endpoints)
        .where(
          and(eq(endpoints.id, endpointId), eq(endpoints.organizationId, principal.organizationId)),
        )
        .for("update")
        .limit(1);
      if (endpoint === undefined) return null;
      const [target] = await transaction
        .select({ id: endpointVersions.id })
        .from(endpointVersions)
        .innerJoin(endpoints, eq(endpointVersions.endpointId, endpoints.id))
        .where(
          and(
            eq(endpointVersions.endpointId, endpointId),
            eq(endpointVersions.version, versionNumber),
            eq(endpoints.organizationId, principal.organizationId),
          ),
        )
        .limit(1);
      if (target === undefined) return null;
      const [existing] = await transaction
        .select()
        .from(deploymentAliases)
        .where(
          and(
            eq(deploymentAliases.endpointId, endpointId),
            eq(deploymentAliases.alias, "production"),
          ),
        )
        .for("update")
        .limit(1);
      let revision: number;
      if (existing === undefined) {
        if (expectedRevision !== null) return null;
        const [created] = await transaction
          .insert(deploymentAliases)
          .values({
            endpointId,
            endpointVersionId: target.id,
            updatedBy: principal.userId,
          })
          .returning();
        if (created === undefined) throw new Error("Alias insert did not return a row.");
        revision = created.revision;
      } else {
        if (expectedRevision !== existing.revision) return null;
        const [updated] = await transaction
          .update(deploymentAliases)
          .set({
            endpointVersionId: target.id,
            revision: existing.revision + 1,
            updatedBy: principal.userId,
            updatedAt: new Date(),
          })
          .where(
            and(
              eq(deploymentAliases.id, existing.id),
              eq(deploymentAliases.revision, existing.revision),
            ),
          )
          .returning();
        if (updated === undefined) return null;
        revision = updated.revision;
      }
      await transaction.insert(auditEvents).values({
        organizationId: principal.organizationId,
        actorUserId: principal.userId,
        action: "endpoint.alias.promoted",
        resourceType: "endpoint",
        resourceId: endpointId,
        metadata: { alias: "production", version: versionNumber, revision },
      });
      return { version: versionNumber, revision };
    });
  }

  async createApiKey(
    principal: CreatorPrincipal,
    input: Parameters<ControlRepository["createApiKey"]>[1],
  ): Promise<ApiKeySummary> {
    return this.database.transaction(async (transaction) => {
      const [key] = await transaction
        .insert(apiKeys)
        .values({
          organizationId: principal.organizationId,
          name: input.name,
          keyPrefix: input.keyPrefix,
          keyDigest: input.keyDigest,
          scopes: input.scopes,
          createdBy: principal.userId,
        })
        .returning();
      if (key === undefined) throw new Error("API key insert did not return a row.");
      await transaction.insert(auditEvents).values({
        organizationId: principal.organizationId,
        actorUserId: principal.userId,
        action: "api_key.created",
        resourceType: "api_key",
        resourceId: key.id,
        metadata: { prefix: key.keyPrefix, scopes: key.scopes },
      });
      return keySummary(key);
    });
  }

  async listApiKeys(organizationId: string): Promise<ApiKeySummary[]> {
    const rows = await this.database
      .select()
      .from(apiKeys)
      .where(eq(apiKeys.organizationId, organizationId))
      .orderBy(desc(apiKeys.createdAt));
    return rows.map(keySummary);
  }

  async revokeApiKey(principal: CreatorPrincipal, keyId: string): Promise<boolean> {
    return this.database.transaction(async (transaction) => {
      const [key] = await transaction
        .update(apiKeys)
        .set({ status: "revoked", revokedAt: new Date() })
        .where(and(eq(apiKeys.id, keyId), eq(apiKeys.organizationId, principal.organizationId)))
        .returning();
      if (key === undefined) return false;
      await transaction.insert(auditEvents).values({
        organizationId: principal.organizationId,
        actorUserId: principal.userId,
        action: "api_key.revoked",
        resourceType: "api_key",
        resourceId: key.id,
        metadata: { prefix: key.keyPrefix },
      });
      return true;
    });
  }

  async listInvocations(organizationId: string, limit: number): Promise<InvocationSummary[]> {
    const rows = await this.database
      .select({
        invocation: invocations,
        endpointVersionNumber: endpointVersions.version,
      })
      .from(invocations)
      .leftJoin(
        endpointVersions,
        and(
          eq(invocations.endpointId, endpointVersions.endpointId),
          eq(invocations.endpointVersionId, endpointVersions.id),
        ),
      )
      .where(eq(invocations.callerOrganizationId, organizationId))
      .orderBy(desc(invocations.startedAt))
      .limit(limit);
    return rows.map(({ invocation, endpointVersionNumber }) =>
      invocationSummary(invocation, endpointVersionNumber),
    );
  }

  async getUsage(organizationId: string): Promise<UsageSummary> {
    const [row] = await this.database
      .select({
        invocations: sql<number>`count(*)::int`,
        succeeded: sql<number>`count(*) filter (where ${invocations.status} = 'succeeded')::int`,
        failed: sql<number>`count(*) filter (where ${invocations.status} = 'failed')::int`,
        totalTokens: sql<number>`coalesce(sum(${invocations.totalTokens}), 0)::int`,
        estimatedProviderCost: sql<string>`coalesce(sum(${invocations.estimatedProviderCost}), 0)::numeric(14,6)`,
      })
      .from(invocations)
      .where(eq(invocations.callerOrganizationId, organizationId));
    return (
      row ?? {
        invocations: 0,
        succeeded: 0,
        failed: 0,
        totalTokens: 0,
        estimatedProviderCost: "0.000000",
      }
    );
  }
}
