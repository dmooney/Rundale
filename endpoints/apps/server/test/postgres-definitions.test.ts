import { randomUUID } from "node:crypto";
import { fileURLToPath } from "node:url";
import { afterAll, describe, expect, it } from "vitest";
import { and, eq, inArray } from "drizzle-orm";
import {
  auditEvents,
  createDatabase,
  endpointDrafts,
  endpoints,
  endpointVersions,
  organizationMembers,
  organizations,
  users,
} from "@limerick/database";
import {
  planDefinitions,
  publishDefinitions,
  readDefinitionFiles,
} from "../src/control/definition-files.js";
import { PostgresControlRepository } from "../src/infrastructure/postgres-control-repository.js";
import { PostgresDefinitionRepository } from "../src/infrastructure/postgres-definition-repository.js";

const databaseUrl = process.env.DATABASE_URL_TEST;
const suite = databaseUrl === undefined ? describe.skip : describe;
const rundaleEndpoints = fileURLToPath(
  new URL("../../../../mods/rundale/endpoints/", import.meta.url),
);
const allowedModels = new Set(["google/gemini-3.5-flash-lite"]);

suite("PostgreSQL Endpoint definition publishing", () => {
  const database = createDatabase(databaseUrl!);
  const suffix = randomUUID().slice(0, 8);
  let userId = "";
  let organizationId = "";

  afterAll(async () => {
    if (organizationId !== "") {
      const ids = (
        await database.db
          .select({ id: endpoints.id })
          .from(endpoints)
          .where(eq(endpoints.organizationId, organizationId))
      ).map((row) => row.id);
      await database.db.delete(auditEvents).where(eq(auditEvents.organizationId, organizationId));
      if (ids.length > 0) {
        await database.db.delete(endpointVersions).where(inArray(endpointVersions.endpointId, ids));
        await database.db.delete(endpointDrafts).where(inArray(endpointDrafts.endpointId, ids));
        await database.db.delete(endpoints).where(inArray(endpoints.id, ids));
      }
      await database.db
        .delete(organizationMembers)
        .where(eq(organizationMembers.organizationId, organizationId));
      await database.db.delete(organizations).where(eq(organizations.id, organizationId));
    }
    if (userId !== "") await database.db.delete(users).where(eq(users.id, userId));
    await database.close();
  });

  it("publishes the mod's files as exact versions and verifies them idempotently", async () => {
    const [user] = await database.db
      .insert(users)
      .values({
        externalAuthId: `definitions_${suffix}`,
        email: `${suffix}@example.invalid`,
        displayName: "Definitions Owner",
      })
      .returning();
    const [organization] = await database.db
      .insert(organizations)
      .values({ name: "Definitions", slug: `definitions-${suffix}` })
      .returning();
    userId = user!.id;
    organizationId = organization!.id;
    await database.db.insert(organizationMembers).values({ userId, organizationId, role: "owner" });
    const principal = { userId, organizationId, role: "owner" as const };
    const repository = new PostgresDefinitionRepository(database.db);
    const files = await readDefinitionFiles(rundaleEndpoints);
    const slugs = [...new Set(files.map((file) => file.slug))];

    const first = await publishDefinitions(repository, principal, files, allowedModels);
    expect(first.problems).toEqual([]);
    expect(first.entries.every((entry) => entry.action === "publish")).toBe(true);

    const control = new PostgresControlRepository(database.db);
    for (const file of files) {
      const [endpoint] = await database.db
        .select()
        .from(endpoints)
        .where(and(eq(endpoints.organizationId, organizationId), eq(endpoints.slug, file.slug)));
      const version = await control.getVersion(organizationId, endpoint!.id, file.version);
      expect(version).toMatchObject({ version: file.version, contentHash: file.contentHash });
      expect(version).toMatchObject(file.definition);
      expect(await control.getDraft(organizationId, endpoint!.id)).not.toBeNull();
    }

    const audits = await database.db
      .select()
      .from(auditEvents)
      .where(eq(auditEvents.organizationId, organizationId));
    const second = await publishDefinitions(repository, principal, files, allowedModels);
    expect(second.problems).toEqual([]);
    expect(second.entries.every((entry) => entry.action === "unchanged")).toBe(true);
    expect(
      await database.db
        .select()
        .from(auditEvents)
        .where(eq(auditEvents.organizationId, organizationId)),
    ).toHaveLength(audits.length);

    const stored = await repository.listPublished(organizationId, slugs);
    expect(planDefinitions(files, stored, allowedModels).problems).toEqual([]);
    const edited = files.map((file) =>
      file.version === 1 && file.slug === "rundale-intent"
        ? { ...file, contentHash: "sha256:edited" }
        : file,
    );
    expect(planDefinitions(edited, stored, allowedModels).problems).toHaveLength(1);
  });
});
