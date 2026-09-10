import { randomUUID } from "node:crypto";
import { afterAll, describe, expect, it } from "vitest";
import { and, eq, inArray } from "drizzle-orm";
import {
  apiKeys,
  auditEvents,
  createDatabase,
  deploymentAliases,
  endpointDrafts,
  endpoints,
  endpointVersions,
  invocationAttempts,
  invocations,
  organizationMembers,
  organizations,
  users,
} from "@parish/database";
import { FakeProvider, FixedPriceCostCalculator, StaticProviderRegistry } from "@parish/providers";
import { DeterministicRuntime } from "@parish/runtime";
import { ControlService } from "../src/control/service.js";
import { PlaygroundService } from "../src/control/playground-service.js";
import { PostgresControlRepository } from "../src/infrastructure/postgres-control-repository.js";
import { PostgresInvocationRepository } from "../src/infrastructure/postgres-invocation-repository.js";
import { InvocationQuotaExceededError } from "../src/invocation/contracts.js";
import { InvocationService } from "../src/invocation/service.js";

const databaseUrl = process.env.DATABASE_URL_TEST;
const suite = databaseUrl === undefined ? describe.skip : describe;

suite("PostgreSQL workflow integration", () => {
  const database = createDatabase(databaseUrl!);
  const suffix = randomUUID().slice(0, 8);
  let userId = "";
  let organizationId = "";
  let endpointId = "";

  afterAll(async () => {
    if (organizationId !== "") {
      const invocationRows = await database.db
        .select({ id: invocations.id })
        .from(invocations)
        .where(eq(invocations.callerOrganizationId, organizationId));
      const ids = invocationRows.map((row) => row.id);
      if (ids.length > 0) {
        await database.db
          .delete(invocationAttempts)
          .where(inArray(invocationAttempts.invocationId, ids));
        await database.db.delete(invocations).where(inArray(invocations.id, ids));
      }
      await database.db.delete(apiKeys).where(eq(apiKeys.organizationId, organizationId));
      await database.db.delete(auditEvents).where(eq(auditEvents.organizationId, organizationId));
    }
    if (endpointId !== "") {
      await database.db
        .delete(deploymentAliases)
        .where(eq(deploymentAliases.endpointId, endpointId));
      await database.db.delete(endpointVersions).where(eq(endpointVersions.endpointId, endpointId));
      await database.db.delete(endpointDrafts).where(eq(endpointDrafts.endpointId, endpointId));
      await database.db.delete(endpoints).where(eq(endpoints.id, endpointId));
    }
    if (organizationId !== "") {
      await database.db
        .delete(organizationMembers)
        .where(eq(organizationMembers.organizationId, organizationId));
      await database.db.delete(organizations).where(eq(organizations.id, organizationId));
    }
    if (userId !== "") await database.db.delete(users).where(eq(users.id, userId));
    await database.close();
  });

  it("persists draft tests and versioned API invocation without raw content", async () => {
    const [user] = await database.db
      .insert(users)
      .values({
        externalAuthId: `test_${suffix}`,
        email: `${suffix}@example.invalid`,
        displayName: "Integration Owner",
      })
      .returning();
    const [organization] = await database.db
      .insert(organizations)
      .values({ name: "Integration", slug: `integration-${suffix}` })
      .returning();
    expect(user).toBeDefined();
    expect(organization).toBeDefined();
    userId = user!.id;
    organizationId = organization!.id;
    await database.db.insert(organizationMembers).values({
      userId,
      organizationId,
      role: "owner",
    });

    const principal = { userId, organizationId, role: "owner" as const };
    const controlRepository = new PostgresControlRepository(database.db);
    const invocationRepository = new PostgresInvocationRepository(database.db);
    const runtime = new DeterministicRuntime(
      new StaticProviderRegistry([new FakeProvider()]),
      new FixedPriceCostCalculator({}),
    );
    const control = new ControlService(controlRepository, {
      allowedModels: new Set(["fake/fake-v1"]),
    });
    const created = await control.createEndpoint(principal, {
      name: "Typed transform",
      slug: `typed-${suffix}`,
      definition: {
        inputSchema: {
          type: "object",
          properties: { text: { type: "string" } },
          required: ["text"],
          additionalProperties: false,
        },
        outputSchema: {
          type: "object",
          properties: { result: { type: "string" } },
          required: ["result"],
          additionalProperties: false,
        },
        instructions: "Return a typed result.",
        providerConfig: { provider: "fake", model: "fake-v1" },
        inferenceConfig: { maxOutputTokens: 128, retryCount: 0 },
      },
    });
    endpointId = created.endpoint.id;

    const playground = new PlaygroundService(controlRepository, invocationRepository, runtime, {
      globalInferenceEnabled: true,
      timeoutMs: 2_000,
      requestsPerDay: 100,
    });
    await playground.invokeDraft(principal, {
      endpointId,
      requestId: `test_${randomUUID()}`,
      input: { values: { text: "draft" }, attachments: [] },
      inputBytes: 16,
    });
    const version = await control.publish(principal, endpointId, 1);
    await control.promoteProduction(principal, endpointId, version.version, null);
    const issued = await control.createApiKey(principal, "Integration CLI", [
      `invoke:endpoint:${created.endpoint.slug}`,
    ]);
    const invocation = new InvocationService(invocationRepository, runtime, {
      globalInferenceEnabled: true,
      requestsPerMinute: 100,
      requestsPerDay: 100,
      timeoutMs: 2_000,
    });
    await invocation.invoke({
      authorization: `Bearer ${issued.secret}`,
      organizationSlug: organization!.slug,
      endpointSlug: created.endpoint.slug,
      requestId: `api_${randomUUID()}`,
      input: { values: { text: "published" }, attachments: [] },
      inputBytes: 20,
    });

    const updated = await control.updateDraft(principal, endpointId, 1, {
      inputSchema: created.draft.inputSchema,
      outputSchema: created.draft.outputSchema,
      instructions: "Return a second typed result.",
      providerConfig: created.draft.providerConfig,
      inferenceConfig: created.draft.inferenceConfig,
    });
    const versionTwo = await control.publish(principal, endpointId, updated.revision);
    const beforePromotion = await control.listVersions(principal, endpointId);
    expect(beforePromotion).toHaveLength(2);
    expect(beforePromotion.every((summary) => summary.productionAliasRevision === 1)).toBe(true);
    expect(beforePromotion.find((summary) => summary.version === versionTwo.version)).toMatchObject(
      {
        isProduction: false,
        productionAliasRevision: 1,
      },
    );
    const currentAliasRevision = beforePromotion[0]?.productionAliasRevision;
    expect(currentAliasRevision).toBe(1);
    await control.promoteProduction(
      principal,
      endpointId,
      versionTwo.version,
      currentAliasRevision,
    );
    await invocation.invoke({
      authorization: `Bearer ${issued.secret}`,
      organizationSlug: organization!.slug,
      endpointSlug: created.endpoint.slug,
      requestId: `api_${randomUUID()}`,
      input: { values: { text: "v2" }, attachments: [] },
      inputBytes: 13,
    });
    await control.promoteProduction(principal, endpointId, version.version, 2);
    await invocation.invoke({
      authorization: `Bearer ${issued.secret}`,
      organizationSlug: organization!.slug,
      endpointSlug: created.endpoint.slug,
      requestId: `api_${randomUUID()}`,
      input: { values: { text: "rollback" }, attachments: [] },
      inputBytes: 19,
    });

    const rows = await database.db
      .select()
      .from(invocations)
      .where(and(eq(invocations.endpointId, endpointId), eq(invocations.status, "succeeded")));
    expect(rows).toHaveLength(4);
    expect(rows.find((row) => row.isTest)).toMatchObject({
      endpointVersionId: null,
      endpointDraftId: created.draft.id,
      apiKeyId: null,
      validationStatus: "valid",
    });
    expect(rows.filter((row) => row.endpointVersionId === version.id)).toHaveLength(2);
    expect(rows.filter((row) => row.endpointVersionId === versionTwo.id)).toHaveLength(1);
    expect(rows.filter((row) => !row.isTest).every((row) => row.validationStatus === "valid")).toBe(
      true,
    );
    const usage = await control.getUsage(principal);
    expect(usage).toMatchObject({ invocations: 4, succeeded: 4, failed: 0, totalTokens: 60 });
    expect(Object.keys(rows[0]!)).not.toContain("input");
    expect(Object.keys(rows[0]!)).not.toContain("output");
    const summaries = await control.listInvocations(principal, 100);
    expect(summaries.find((summary) => summary.isTest)).toMatchObject({
      endpointVersionNumber: null,
    });
    expect(summaries.find((summary) => summary.endpointVersionId === versionTwo.id)).toMatchObject({
      endpointVersionNumber: 2,
    });
  });

  it("reserves daily quota atomically for concurrent invocation creation", async () => {
    const [version] = await database.db
      .select()
      .from(endpointVersions)
      .where(and(eq(endpointVersions.endpointId, endpointId), eq(endpointVersions.version, 1)))
      .limit(1);
    expect(version).toBeDefined();
    const repository = new PostgresInvocationRepository(database.db);
    const results = await Promise.allSettled(
      [1, 2].map((attempt) =>
        repository.createInvocation(
          {
            requestId: `quota_${suffix}_${attempt}`,
            callerOrganizationId: organizationId,
            endpointId,
            endpointVersionId: version!.id,
            endpointDraftId: null,
            apiKeyId: null,
            isTest: false,
            inputBytes: 0,
            provider: "fake",
            model: "fake-v1",
          },
          5,
        ),
      ),
    );

    expect(results.filter((result) => result.status === "fulfilled")).toHaveLength(1);
    const rejected = results.find((result) => result.status === "rejected");
    expect(rejected?.status === "rejected" ? rejected.reason : undefined).toBeInstanceOf(
      InvocationQuotaExceededError,
    );
    const fulfilled = results.find((result) => result.status === "fulfilled");
    if (fulfilled?.status !== "fulfilled") throw new Error("Quota reservation did not succeed.");
    expect(
      await repository.finalizeSuccess(fulfilled.value, {
        durationMs: 1,
        outputBytes: 2,
        estimatedProviderCost: "0.000000",
      }),
    ).toBe(true);
    expect(
      await repository.finalizeFailure(fulfilled.value, {
        durationMs: 2,
        errorCode: "INTERNAL_ERROR",
        validationStatus: "pending",
      }),
    ).toBe(false);
    const [finalized] = await database.db
      .select({ status: invocations.status })
      .from(invocations)
      .where(eq(invocations.id, fulfilled.value));
    expect(finalized?.status).toBe("succeeded");
  });

  it("serializes first production alias creation", async () => {
    await database.db.delete(deploymentAliases).where(eq(deploymentAliases.endpointId, endpointId));
    const repository = new PostgresControlRepository(database.db);
    const principal = { userId, organizationId, role: "owner" as const };
    const results = await Promise.all([
      repository.promoteProduction(principal, endpointId, 1, null),
      repository.promoteProduction(principal, endpointId, 1, null),
    ]);

    expect(results.filter((result) => result !== null)).toHaveLength(1);
    expect(results.filter((result) => result === null)).toHaveLength(1);
    expect(results.find((result) => result !== null)).toEqual({ version: 1, revision: 1 });
  });

  it("enforces Endpoint ownership and invocation source integrity", async () => {
    const [otherEndpoint] = await database.db
      .insert(endpoints)
      .values({
        organizationId,
        name: "Other Endpoint",
        slug: `other-${suffix}`,
      })
      .returning();
    expect(otherEndpoint).toBeDefined();
    const [otherVersion] = await database.db
      .insert(endpointVersions)
      .values({
        endpointId: otherEndpoint!.id,
        version: 1,
        contentHash: "sha256:other",
        inputSchema: { type: "object" },
        outputSchema: { type: "object" },
        instructions: "Other Endpoint test.",
        providerConfig: { provider: "fake", model: "fake-v1" },
        inferenceConfig: { maxOutputTokens: 1, retryCount: 0 },
        publishedBy: userId,
      })
      .returning();
    expect(otherVersion).toBeDefined();
    try {
      await expect(
        database.db.insert(deploymentAliases).values({
          endpointId,
          endpointVersionId: otherVersion!.id,
          updatedBy: userId,
        }),
      ).rejects.toThrow();
      await expect(
        database.db.insert(invocations).values({
          requestId: `invalid-source_${suffix}`,
          callerOrganizationId: organizationId,
          endpointId,
          endpointVersionId: null,
          endpointDraftId: null,
          apiKeyId: null,
          isTest: false,
          inputBytes: 0,
          provider: "fake",
          model: "fake-v1",
        }),
      ).rejects.toThrow();
    } finally {
      await database.db.delete(endpointVersions).where(eq(endpointVersions.id, otherVersion!.id));
      await database.db.delete(endpoints).where(eq(endpoints.id, otherEndpoint!.id));
    }
  });
});
