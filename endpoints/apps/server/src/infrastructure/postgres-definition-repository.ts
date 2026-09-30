import { and, eq, inArray } from "drizzle-orm";
import {
  auditEvents,
  endpointDrafts,
  endpoints,
  endpointVersions,
  type Database,
} from "@limerick/database";
import type { EndpointVersionSnapshot } from "@limerick/domain";
import type { CreatorPrincipal } from "../control/contracts.js";
import type {
  DefinitionFile,
  DefinitionPublishRepository,
  PublishedDefinition,
} from "../control/definition-files.js";

export class PostgresDefinitionRepository implements DefinitionPublishRepository {
  constructor(private readonly database: Database) {}

  async listPublished(
    organizationId: string,
    slugs: readonly string[],
  ): Promise<PublishedDefinition[]> {
    if (slugs.length === 0) return [];
    const rows = await this.database
      .select({ slug: endpoints.slug, version: endpointVersions })
      .from(endpointVersions)
      .innerJoin(endpoints, eq(endpointVersions.endpointId, endpoints.id))
      .where(
        and(eq(endpoints.organizationId, organizationId), inArray(endpoints.slug, [...slugs])),
      );
    return rows.map(({ slug, version }) => ({
      slug,
      version: version.version,
      contentHash: version.contentHash,
      definition: {
        inputSchema: version.inputSchema,
        outputSchema: version.outputSchema,
        instructions: version.instructions,
        providerConfig: version.providerConfig,
        inferenceConfig: version.inferenceConfig,
      },
    }));
  }

  async publishFile(
    principal: CreatorPrincipal,
    file: DefinitionFile,
  ): Promise<EndpointVersionSnapshot> {
    return this.database.transaction(async (transaction) => {
      const [created] = await transaction
        .insert(endpoints)
        .values({
          organizationId: principal.organizationId,
          name: file.slug,
          slug: file.slug,
          description: `Published from ${file.fileName}.`,
        })
        .onConflictDoNothing({ target: [endpoints.organizationId, endpoints.slug] })
        .returning();
      if (created !== undefined) {
        await transaction.insert(endpointDrafts).values({
          endpointId: created.id,
          ...file.definition,
          updatedBy: principal.userId,
        });
        await transaction.insert(auditEvents).values({
          organizationId: principal.organizationId,
          actorUserId: principal.userId,
          action: "endpoint.created",
          resourceType: "endpoint",
          resourceId: created.id,
          metadata: { slug: file.slug, source: file.fileName },
        });
      }
      const [endpoint] = await transaction
        .select({ id: endpoints.id })
        .from(endpoints)
        .where(
          and(
            eq(endpoints.organizationId, principal.organizationId),
            eq(endpoints.slug, file.slug),
          ),
        )
        .for("update")
        .limit(1);
      if (endpoint === undefined) throw new Error("Endpoint did not resolve after insert.");
      const [inserted] = await transaction
        .insert(endpointVersions)
        .values({
          endpointId: endpoint.id,
          version: file.version,
          contentHash: file.contentHash,
          ...file.definition,
          publishedBy: principal.userId,
        })
        .onConflictDoNothing({ target: [endpointVersions.endpointId, endpointVersions.version] })
        .returning();
      if (inserted !== undefined) {
        await transaction.insert(auditEvents).values({
          organizationId: principal.organizationId,
          actorUserId: principal.userId,
          action: "endpoint.version.published",
          resourceType: "endpoint_version",
          resourceId: inserted.id,
          metadata: {
            endpointId: endpoint.id,
            version: file.version,
            contentHash: file.contentHash,
            source: file.fileName,
          },
        });
      }
      const [row] =
        inserted === undefined
          ? await transaction
              .select()
              .from(endpointVersions)
              .where(
                and(
                  eq(endpointVersions.endpointId, endpoint.id),
                  eq(endpointVersions.version, file.version),
                ),
              )
              .limit(1)
          : [inserted];
      if (row === undefined) throw new Error("Version did not resolve after insert.");
      return snapshot(row, principal.organizationId);
    });
  }

  async replaceFile(
    principal: CreatorPrincipal,
    file: DefinitionFile,
  ): Promise<EndpointVersionSnapshot> {
    return this.database.transaction(async (transaction) => {
      const [endpoint] = await transaction
        .select({ id: endpoints.id })
        .from(endpoints)
        .where(
          and(
            eq(endpoints.organizationId, principal.organizationId),
            eq(endpoints.slug, file.slug),
          ),
        )
        .limit(1);
      if (endpoint === undefined) throw new Error(`${file.slug} is not published.`);
      const [previous] = await transaction
        .select({ id: endpointVersions.id, contentHash: endpointVersions.contentHash })
        .from(endpointVersions)
        .where(
          and(
            eq(endpointVersions.endpointId, endpoint.id),
            eq(endpointVersions.version, file.version),
          ),
        )
        .for("update")
        .limit(1);
      if (previous === undefined) throw new Error(`${file.slug}@${file.version} is not published.`);
      const [row] = await transaction
        .update(endpointVersions)
        .set({
          contentHash: file.contentHash,
          ...file.definition,
          publishedBy: principal.userId,
          publishedAt: new Date(),
        })
        .where(eq(endpointVersions.id, previous.id))
        .returning();
      if (row === undefined) throw new Error("Version did not resolve after update.");
      await transaction.insert(auditEvents).values({
        organizationId: principal.organizationId,
        actorUserId: principal.userId,
        action: "endpoint.version.replaced",
        resourceType: "endpoint_version",
        resourceId: row.id,
        metadata: {
          endpointId: endpoint.id,
          version: file.version,
          previousContentHash: previous.contentHash,
          contentHash: file.contentHash,
          source: file.fileName,
        },
      });
      return snapshot(row, principal.organizationId);
    });
  }
}

function snapshot(
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
