import { and, eq } from "drizzle-orm";
import { issueApiKey } from "@parish/auth";
import { definitionContentHash, type EndpointDefinition } from "@parish/domain";
import {
  apiKeys,
  deploymentAliases,
  endpointDrafts,
  endpoints,
  endpointVersions,
  organizationMembers,
  organizations,
  users,
} from "./schema.js";
import { createDatabase } from "./index.js";

const databaseUrl = process.env.DATABASE_URL;
if (databaseUrl === undefined) throw new Error("DATABASE_URL is required.");
const externalAuthId = process.env.PARISH_OWNER_FIREBASE_UID ?? "user_synthetic_owner";
const email = process.env.PARISH_OWNER_EMAIL ?? "owner@example.invalid";
const definition: EndpointDefinition = {
  inputSchema: {
    type: "object",
    properties: {
      image: {
        type: "string",
        contentMediaType: "image/*",
        "x-semantic-type": "image",
      },
    },
    required: ["image"],
    additionalProperties: false,
  },
  outputSchema: {
    type: "object",
    properties: { result: { type: "string" } },
    required: ["result"],
    additionalProperties: false,
  },
  instructions: "Describe the supplied image as a concise structured result.",
  providerConfig: { provider: "fake", model: "fake-v1" },
  inferenceConfig: { maxOutputTokens: 256, retryCount: 0 },
};

const database = createDatabase(databaseUrl);
try {
  await database.db.transaction(async (transaction) => {
    await transaction
      .insert(users)
      .values({ externalAuthId, email, displayName: "Parish Owner" })
      .onConflictDoNothing({ target: users.externalAuthId });
    const [user] = await transaction
      .select()
      .from(users)
      .where(eq(users.externalAuthId, externalAuthId));
    if (user === undefined) throw new Error("Could not resolve seeded owner.");

    await transaction
      .insert(organizations)
      .values({ name: "Parish Demo", slug: "parish-demo" })
      .onConflictDoNothing({ target: organizations.slug });
    const [organization] = await transaction
      .select()
      .from(organizations)
      .where(eq(organizations.slug, "parish-demo"));
    if (organization === undefined) throw new Error("Could not resolve seeded organization.");
    await transaction
      .insert(organizationMembers)
      .values({ organizationId: organization.id, userId: user.id, role: "owner" })
      .onConflictDoNothing();

    await transaction
      .insert(endpoints)
      .values({
        organizationId: organization.id,
        name: "Generic Image Extractor",
        slug: "generic-image-extractor",
        description: "Synthetic local-development Endpoint with no application-specific semantics.",
      })
      .onConflictDoNothing();
    const [endpoint] = await transaction
      .select()
      .from(endpoints)
      .where(
        and(
          eq(endpoints.organizationId, organization.id),
          eq(endpoints.slug, "generic-image-extractor"),
        ),
      );
    if (endpoint === undefined) throw new Error("Could not resolve seeded Endpoint.");

    await transaction
      .insert(endpointDrafts)
      .values({ endpointId: endpoint.id, ...definition, updatedBy: user.id })
      .onConflictDoNothing({ target: endpointDrafts.endpointId });
    await transaction
      .insert(endpointVersions)
      .values({
        endpointId: endpoint.id,
        version: 1,
        contentHash: definitionContentHash(definition),
        ...definition,
        publishedBy: user.id,
      })
      .onConflictDoNothing();
    const [version] = await transaction
      .select()
      .from(endpointVersions)
      .where(and(eq(endpointVersions.endpointId, endpoint.id), eq(endpointVersions.version, 1)));
    if (version === undefined) throw new Error("Could not resolve seeded version.");
    await transaction
      .insert(deploymentAliases)
      .values({ endpointId: endpoint.id, endpointVersionId: version.id, updatedBy: user.id })
      .onConflictDoNothing();

    if (process.env.SEED_CREATE_API_KEY === "true") {
      const [existing] = await transaction
        .select({ id: apiKeys.id })
        .from(apiKeys)
        .where(and(eq(apiKeys.organizationId, organization.id), eq(apiKeys.name, "Local CLI")));
      if (existing === undefined) {
        const issued = issueApiKey();
        await transaction.insert(apiKeys).values({
          organizationId: organization.id,
          name: "Local CLI",
          keyPrefix: issued.prefix,
          keyDigest: issued.digest,
          scopes: ["invoke:endpoint:generic-image-extractor"],
          createdBy: user.id,
        });
        process.stdout.write(`Seed API key (shown once): ${issued.secret}\n`);
      }
    }
  });
} finally {
  await database.close();
}
