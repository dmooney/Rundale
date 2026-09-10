import {
  boolean,
  check,
  foreignKey,
  index,
  integer,
  jsonb,
  numeric,
  pgEnum,
  pgTable,
  primaryKey,
  text,
  timestamp,
  unique,
  uuid,
} from "drizzle-orm/pg-core";
import { sql } from "drizzle-orm";
import type { InferenceConfig, JsonSchema, ProviderConfig } from "@parish/domain";

export const organizationStatus = pgEnum("organization_status", ["active", "suspended"]);
export const memberRole = pgEnum("member_role", ["owner", "admin", "developer", "viewer"]);
export const endpointStatus = pgEnum("endpoint_status", ["active", "disabled"]);
export const apiKeyStatus = pgEnum("api_key_status", ["active", "revoked"]);
export const invocationStatus = pgEnum("invocation_status", ["running", "succeeded", "failed"]);
export const validationStatus = pgEnum("validation_status", ["pending", "valid", "invalid"]);

const timestamps = {
  createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow(),
  updatedAt: timestamp("updated_at", { withTimezone: true }).notNull().defaultNow(),
};

export const users = pgTable("users", {
  id: uuid("id").primaryKey().defaultRandom(),
  externalAuthId: text("external_auth_id").notNull().unique(),
  email: text("email").notNull(),
  displayName: text("display_name").notNull(),
  ...timestamps,
});

export const organizations = pgTable("organizations", {
  id: uuid("id").primaryKey().defaultRandom(),
  name: text("name").notNull(),
  slug: text("slug").notNull().unique(),
  status: organizationStatus("status").notNull().default("active"),
  dailyInvocationQuota: integer("daily_invocation_quota").notNull().default(1000),
  inferenceEnabled: boolean("inference_enabled").notNull().default(true),
  ...timestamps,
});

export const organizationMembers = pgTable(
  "organization_members",
  {
    organizationId: uuid("organization_id")
      .notNull()
      .references(() => organizations.id),
    userId: uuid("user_id")
      .notNull()
      .references(() => users.id),
    role: memberRole("role").notNull(),
    createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow(),
  },
  (table) => [primaryKey({ columns: [table.organizationId, table.userId] })],
);

export const endpoints = pgTable(
  "endpoints",
  {
    id: uuid("id").primaryKey().defaultRandom(),
    organizationId: uuid("organization_id")
      .notNull()
      .references(() => organizations.id),
    name: text("name").notNull(),
    slug: text("slug").notNull(),
    description: text("description").notNull().default(""),
    status: endpointStatus("status").notNull().default("active"),
    inferenceEnabled: boolean("inference_enabled").notNull().default(true),
    ...timestamps,
  },
  (table) => [unique().on(table.organizationId, table.slug), index().on(table.organizationId)],
);

export const endpointDrafts = pgTable(
  "endpoint_drafts",
  {
    id: uuid("id").primaryKey().defaultRandom(),
    endpointId: uuid("endpoint_id")
      .notNull()
      .unique()
      .references(() => endpoints.id),
    revision: integer("revision").notNull().default(1),
    inputSchema: jsonb("input_schema").$type<JsonSchema>().notNull(),
    outputSchema: jsonb("output_schema").$type<JsonSchema>().notNull(),
    instructions: text("instructions").notNull(),
    providerConfig: jsonb("provider_config").$type<ProviderConfig>().notNull(),
    inferenceConfig: jsonb("inference_config").$type<InferenceConfig>().notNull(),
    updatedBy: uuid("updated_by")
      .notNull()
      .references(() => users.id),
    updatedAt: timestamp("updated_at", { withTimezone: true }).notNull().defaultNow(),
  },
  (table) => [unique("endpoint_drafts_endpoint_id_id_unique").on(table.endpointId, table.id)],
);

export const endpointVersions = pgTable(
  "endpoint_versions",
  {
    id: uuid("id").primaryKey().defaultRandom(),
    endpointId: uuid("endpoint_id")
      .notNull()
      .references(() => endpoints.id),
    version: integer("version").notNull(),
    contentHash: text("content_hash").notNull(),
    inputSchema: jsonb("input_schema").$type<JsonSchema>().notNull(),
    outputSchema: jsonb("output_schema").$type<JsonSchema>().notNull(),
    instructions: text("instructions").notNull(),
    providerConfig: jsonb("provider_config").$type<ProviderConfig>().notNull(),
    inferenceConfig: jsonb("inference_config").$type<InferenceConfig>().notNull(),
    publishedBy: uuid("published_by")
      .notNull()
      .references(() => users.id),
    publishedAt: timestamp("published_at", { withTimezone: true }).notNull().defaultNow(),
  },
  (table) => [
    unique().on(table.endpointId, table.version),
    unique("endpoint_versions_endpoint_id_id_unique").on(table.endpointId, table.id),
    index().on(table.endpointId),
  ],
);

export const deploymentAliases = pgTable(
  "deployment_aliases",
  {
    id: uuid("id").primaryKey().defaultRandom(),
    endpointId: uuid("endpoint_id")
      .notNull()
      .references(() => endpoints.id),
    alias: text("alias").notNull().default("production"),
    endpointVersionId: uuid("endpoint_version_id").notNull(),
    revision: integer("revision").notNull().default(1),
    updatedBy: uuid("updated_by")
      .notNull()
      .references(() => users.id),
    updatedAt: timestamp("updated_at", { withTimezone: true }).notNull().defaultNow(),
  },
  (table) => [
    unique().on(table.endpointId, table.alias),
    foreignKey({
      columns: [table.endpointId, table.endpointVersionId],
      foreignColumns: [endpointVersions.endpointId, endpointVersions.id],
      name: "deployment_aliases_endpoint_version_endpoint_fk",
    }),
  ],
);

export const apiKeys = pgTable(
  "api_keys",
  {
    id: uuid("id").primaryKey().defaultRandom(),
    organizationId: uuid("organization_id")
      .notNull()
      .references(() => organizations.id),
    name: text("name").notNull(),
    keyPrefix: text("key_prefix").notNull().unique(),
    keyDigest: text("key_digest").notNull(),
    scopes: jsonb("scopes").$type<string[]>().notNull(),
    status: apiKeyStatus("status").notNull().default("active"),
    lastUsedAt: timestamp("last_used_at", { withTimezone: true }),
    createdBy: uuid("created_by")
      .notNull()
      .references(() => users.id),
    createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow(),
    revokedAt: timestamp("revoked_at", { withTimezone: true }),
  },
  (table) => [index().on(table.organizationId)],
);

export const invocations = pgTable(
  "invocations",
  {
    id: uuid("id").primaryKey().defaultRandom(),
    requestId: text("request_id").notNull().unique(),
    callerOrganizationId: uuid("caller_organization_id")
      .notNull()
      .references(() => organizations.id),
    endpointId: uuid("endpoint_id")
      .notNull()
      .references(() => endpoints.id),
    endpointVersionId: uuid("endpoint_version_id"),
    endpointDraftId: uuid("endpoint_draft_id"),
    apiKeyId: uuid("api_key_id").references(() => apiKeys.id),
    isTest: boolean("is_test").notNull().default(false),
    status: invocationStatus("status").notNull().default("running"),
    startedAt: timestamp("started_at", { withTimezone: true }).notNull().defaultNow(),
    completedAt: timestamp("completed_at", { withTimezone: true }),
    durationMs: integer("duration_ms"),
    inputBytes: integer("input_bytes").notNull().default(0),
    outputBytes: integer("output_bytes"),
    provider: text("provider").notNull(),
    model: text("model").notNull(),
    providerRequestId: text("provider_request_id"),
    inputTokens: integer("input_tokens"),
    outputTokens: integer("output_tokens"),
    totalTokens: integer("total_tokens"),
    estimatedProviderCost: numeric("estimated_provider_cost", { precision: 14, scale: 6 }),
    validationStatus: validationStatus("validation_status").notNull().default("pending"),
    errorCode: text("error_code"),
  },
  (table) => [
    index().on(table.callerOrganizationId, table.startedAt),
    index().on(table.endpointId),
    check(
      "invocations_exactly_one_source_check",
      sql`(${table.endpointVersionId} IS NOT NULL) <> (${table.endpointDraftId} IS NOT NULL)`,
    ),
    foreignKey({
      columns: [table.endpointId, table.endpointVersionId],
      foreignColumns: [endpointVersions.endpointId, endpointVersions.id],
      name: "invocations_endpoint_version_endpoint_fk",
    }),
    foreignKey({
      columns: [table.endpointId, table.endpointDraftId],
      foreignColumns: [endpointDrafts.endpointId, endpointDrafts.id],
      name: "invocations_endpoint_draft_endpoint_fk",
    }),
  ],
);

export const invocationAttempts = pgTable(
  "invocation_attempts",
  {
    id: uuid("id").primaryKey().defaultRandom(),
    invocationId: uuid("invocation_id")
      .notNull()
      .references(() => invocations.id),
    attemptNumber: integer("attempt_number").notNull(),
    provider: text("provider").notNull(),
    model: text("model").notNull(),
    status: invocationStatus("status").notNull(),
    durationMs: integer("duration_ms").notNull(),
    usage: jsonb("usage").$type<Record<string, number>>().notNull(),
    estimatedCost: numeric("estimated_cost", { precision: 14, scale: 6 }).notNull(),
    errorCode: text("error_code"),
    createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow(),
  },
  (table) => [unique().on(table.invocationId, table.attemptNumber)],
);

export const auditEvents = pgTable(
  "audit_events",
  {
    id: uuid("id").primaryKey().defaultRandom(),
    organizationId: uuid("organization_id")
      .notNull()
      .references(() => organizations.id),
    actorUserId: uuid("actor_user_id").references(() => users.id),
    action: text("action").notNull(),
    resourceType: text("resource_type").notNull(),
    resourceId: uuid("resource_id").notNull(),
    metadata: jsonb("metadata").$type<Record<string, unknown>>().notNull().default({}),
    createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow(),
  },
  (table) => [index().on(table.organizationId, table.createdAt)],
);

export const operatorControls = pgTable(
  "operator_controls",
  {
    id: uuid("id").primaryKey().defaultRandom(),
    scope: text("scope").notNull(),
    scopeId: text("scope_id").notNull().default("*"),
    inferenceEnabled: boolean("inference_enabled").notNull().default(true),
    updatedAt: timestamp("updated_at", { withTimezone: true }).notNull().defaultNow(),
  },
  (table) => [unique().on(table.scope, table.scopeId)],
);
