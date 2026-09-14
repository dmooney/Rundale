import { and, eq } from "drizzle-orm";
import { createDatabase } from "./index.js";
import { endpoints, operatorControls, organizations } from "./schema.js";

const [action, scope, suppliedScopeId] = process.argv.slice(2);
if (
  (action !== "enable" && action !== "disable") ||
  (scope !== "global" &&
    scope !== "provider" &&
    scope !== "model" &&
    scope !== "organization" &&
    scope !== "endpoint")
) {
  throw new Error(
    "Usage: pnpm operator <enable|disable> <global|provider|model|organization|endpoint> [scope-id]",
  );
}
const scopeId = scope === "global" ? "*" : suppliedScopeId;
if (scopeId === undefined || scopeId.trim().length === 0) {
  throw new Error("A provider or provider/model scope identifier is required.");
}
if (scope === "model" && !scopeId.includes("/")) {
  throw new Error("Model scope must use the provider/model form.");
}
const databaseUrl = process.env.DATABASE_URL;
if (databaseUrl === undefined) throw new Error("DATABASE_URL is required.");
const database = createDatabase(databaseUrl);
try {
  const inferenceEnabled = action === "enable";
  if (scope === "organization" || scope === "endpoint") {
    const updated =
      scope === "organization"
        ? await database.db
            .update(organizations)
            .set({ inferenceEnabled, updatedAt: new Date() })
            .where(eq(organizations.id, scopeId))
            .returning({ id: organizations.id })
        : await database.db
            .update(endpoints)
            .set({ inferenceEnabled, updatedAt: new Date() })
            .where(eq(endpoints.id, scopeId))
            .returning({ id: endpoints.id });
    if (updated.length === 0) throw new Error(`${scope} '${scopeId}' was not found.`);
  } else {
    const where = and(eq(operatorControls.scope, scope), eq(operatorControls.scopeId, scopeId));
    const [existing] = await database.db
      .select({ id: operatorControls.id })
      .from(operatorControls)
      .where(where)
      .limit(1);
    if (existing === undefined) {
      await database.db.insert(operatorControls).values({ scope, scopeId, inferenceEnabled });
    } else {
      await database.db
        .update(operatorControls)
        .set({ inferenceEnabled, updatedAt: new Date() })
        .where(eq(operatorControls.id, existing.id));
    }
  }
  process.stdout.write(
    `${scope}:${scopeId} inference ${inferenceEnabled ? "enabled" : "disabled"}\n`,
  );
} finally {
  await database.close();
}
