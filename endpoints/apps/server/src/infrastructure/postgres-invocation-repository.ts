import { and, count, eq, gte, or } from "drizzle-orm";
import {
  apiKeys,
  deploymentAliases,
  endpoints,
  endpointVersions,
  invocations,
  invocationAttempts,
  operatorControls,
  organizations,
  type Database,
} from "@parish/database";
import type { EndpointVersionSnapshot } from "@parish/domain";
import { InvocationQuotaExceededError } from "../invocation/contracts.js";
import type {
  InvocationApiKey,
  InvocationRepository,
  InvocationStart,
  ResolvedEndpoint,
} from "../invocation/contracts.js";

function versionSnapshot(
  row: typeof endpointVersions.$inferSelect,
  organizationId: string,
): EndpointVersionSnapshot {
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

export class PostgresInvocationRepository implements InvocationRepository {
  constructor(private readonly database: Database) {}

  async findApiKeyByPrefix(prefix: string): Promise<InvocationApiKey | null> {
    const [result] = await this.database
      .select({ key: apiKeys, organization: organizations })
      .from(apiKeys)
      .innerJoin(organizations, eq(apiKeys.organizationId, organizations.id))
      .where(eq(apiKeys.keyPrefix, prefix))
      .limit(1);
    if (result === undefined) return null;
    return {
      id: result.key.id,
      organizationId: result.key.organizationId,
      keyDigest: result.key.keyDigest,
      scopes: result.key.scopes,
      status: result.key.status,
      organizationStatus: result.organization.status,
      dailyInvocationQuota: result.organization.dailyInvocationQuota,
      organizationInferenceEnabled: result.organization.inferenceEnabled,
    };
  }

  async resolveProduction(
    organizationSlug: string,
    endpointSlug: string,
  ): Promise<ResolvedEndpoint | null> {
    const [result] = await this.database
      .select({ endpoint: endpoints, version: endpointVersions, organizationId: organizations.id })
      .from(organizations)
      .innerJoin(endpoints, eq(endpoints.organizationId, organizations.id))
      .innerJoin(
        deploymentAliases,
        and(
          eq(deploymentAliases.endpointId, endpoints.id),
          eq(deploymentAliases.alias, "production"),
        ),
      )
      .innerJoin(
        endpointVersions,
        and(
          eq(deploymentAliases.endpointId, endpointVersions.endpointId),
          eq(deploymentAliases.endpointVersionId, endpointVersions.id),
        ),
      )
      .where(and(eq(organizations.slug, organizationSlug), eq(endpoints.slug, endpointSlug)))
      .limit(1);
    return result === undefined
      ? null
      : {
          endpointId: result.endpoint.id,
          endpointSlug: result.endpoint.slug,
          endpointStatus: result.endpoint.status,
          endpointInferenceEnabled: result.endpoint.inferenceEnabled,
          version: versionSnapshot(result.version, result.organizationId),
        };
  }

  async resolveVersion(
    organizationSlug: string,
    endpointSlug: string,
    versionNumber: number,
  ): Promise<ResolvedEndpoint | null> {
    const [result] = await this.database
      .select({ endpoint: endpoints, version: endpointVersions, organizationId: organizations.id })
      .from(organizations)
      .innerJoin(endpoints, eq(endpoints.organizationId, organizations.id))
      .innerJoin(endpointVersions, eq(endpointVersions.endpointId, endpoints.id))
      .where(
        and(
          eq(organizations.slug, organizationSlug),
          eq(endpoints.slug, endpointSlug),
          eq(endpointVersions.version, versionNumber),
        ),
      )
      .limit(1);
    return result === undefined
      ? null
      : {
          endpointId: result.endpoint.id,
          endpointSlug: result.endpoint.slug,
          endpointStatus: result.endpoint.status,
          endpointInferenceEnabled: result.endpoint.inferenceEnabled,
          version: versionSnapshot(result.version, result.organizationId),
        };
  }

  async isInferenceEnabled(provider: string, model: string): Promise<boolean> {
    const [disabled] = await this.database
      .select({ id: operatorControls.id })
      .from(operatorControls)
      .where(
        and(
          eq(operatorControls.inferenceEnabled, false),
          or(
            and(eq(operatorControls.scope, "global"), eq(operatorControls.scopeId, "*")),
            and(eq(operatorControls.scope, "provider"), eq(operatorControls.scopeId, provider)),
            and(
              eq(operatorControls.scope, "model"),
              eq(operatorControls.scopeId, `${provider}/${model}`),
            ),
          ),
        ),
      )
      .limit(1);
    return disabled === undefined;
  }

  async createInvocation(start: InvocationStart, dailyInvocationQuota: number): Promise<string> {
    if (!Number.isSafeInteger(dailyInvocationQuota) || dailyInvocationQuota < 1) {
      throw new InvocationQuotaExceededError();
    }
    return this.database.transaction(async (transaction) => {
      const [organization] = await transaction
        .select({ id: organizations.id, dailyInvocationQuota: organizations.dailyInvocationQuota })
        .from(organizations)
        .where(eq(organizations.id, start.callerOrganizationId))
        .for("update")
        .limit(1);
      if (organization === undefined) {
        throw new Error("Invocation organization does not exist.");
      }
      const dayStart = new Date();
      dayStart.setUTCHours(0, 0, 0, 0);
      const [usage] = await transaction
        .select({ value: count() })
        .from(invocations)
        .where(
          and(
            eq(invocations.callerOrganizationId, start.callerOrganizationId),
            gte(invocations.startedAt, dayStart),
          ),
        );
      const quota = Math.min(organization.dailyInvocationQuota, dailyInvocationQuota);
      if (quota < 1 || (usage?.value ?? 0) >= quota) {
        throw new InvocationQuotaExceededError();
      }
      const [row] = await transaction
        .insert(invocations)
        .values(start)
        .returning({ id: invocations.id });
      if (row === undefined) throw new Error("Invocation insert did not return a row.");
      return row.id;
    });
  }

  async recordAttempts(
    invocationId: string,
    attempts: Parameters<InvocationRepository["recordAttempts"]>[1],
  ): Promise<void> {
    if (attempts.length === 0) return;
    await this.database.insert(invocationAttempts).values(
      attempts.map((attempt) => ({
        invocationId,
        attemptNumber: attempt.attempt,
        provider: attempt.provider,
        model: attempt.model,
        status: attempt.status,
        durationMs: attempt.durationMs,
        usage: Object.fromEntries(
          Object.entries(attempt.usage).filter(
            (entry): entry is [string, number] => entry[1] !== undefined,
          ),
        ),
        estimatedCost: attempt.estimatedCostUsd,
        ...(attempt.errorCode === undefined ? {} : { errorCode: attempt.errorCode }),
      })),
    );
  }

  async finalizeSuccess(
    invocationId: string,
    result: Parameters<InvocationRepository["finalizeSuccess"]>[1],
  ): Promise<boolean> {
    const updated = await this.database
      .update(invocations)
      .set({
        status: "succeeded",
        completedAt: new Date(),
        durationMs: result.durationMs,
        outputBytes: result.outputBytes,
        validationStatus: "valid",
        estimatedProviderCost: result.estimatedProviderCost,
        ...(result.providerRequestId === undefined
          ? {}
          : { providerRequestId: result.providerRequestId }),
        ...(result.inputTokens === undefined ? {} : { inputTokens: result.inputTokens }),
        ...(result.outputTokens === undefined ? {} : { outputTokens: result.outputTokens }),
        ...(result.totalTokens === undefined ? {} : { totalTokens: result.totalTokens }),
      })
      .where(and(eq(invocations.id, invocationId), eq(invocations.status, "running")))
      .returning({ id: invocations.id });
    return updated.length > 0;
  }

  async finalizeFailure(
    invocationId: string,
    result: Parameters<InvocationRepository["finalizeFailure"]>[1],
  ): Promise<boolean> {
    const updated = await this.database
      .update(invocations)
      .set({
        status: "failed",
        completedAt: new Date(),
        durationMs: result.durationMs,
        errorCode: result.errorCode,
        validationStatus: result.validationStatus,
        ...(result.providerRequestId === undefined
          ? {}
          : { providerRequestId: result.providerRequestId }),
        ...(result.inputTokens === undefined ? {} : { inputTokens: result.inputTokens }),
        ...(result.outputTokens === undefined ? {} : { outputTokens: result.outputTokens }),
        ...(result.totalTokens === undefined ? {} : { totalTokens: result.totalTokens }),
        ...(result.estimatedProviderCost === undefined
          ? {}
          : { estimatedProviderCost: result.estimatedProviderCost }),
      })
      .where(and(eq(invocations.id, invocationId), eq(invocations.status, "running")))
      .returning({ id: invocations.id });
    return updated.length > 0;
  }

  async touchApiKey(keyId: string, usedAt: Date): Promise<void> {
    await this.database.update(apiKeys).set({ lastUsedAt: usedAt }).where(eq(apiKeys.id, keyId));
  }
}
