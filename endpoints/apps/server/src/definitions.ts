import { resolve } from "node:path";
import { eq } from "drizzle-orm";
import { createDatabase, organizations } from "@limerick/database";
import { readAllowedModels } from "./config.js";
import {
  exportDefinitions,
  planDefinitions,
  publishDefinitions,
  readDefinitionFiles,
  type DefinitionPlan,
} from "./control/definition-files.js";
import { PostgresDefinitionRepository } from "./infrastructure/postgres-definition-repository.js";
import { PostgresCreatorIdentityRepository } from "./infrastructure/postgres-identity-repository.js";

// Publishes Endpoint definition files as versions of the owner's organization
// and verifies the published copies against the files.
//
//   verify   report the plan; exit non-zero if anything disagrees or is unpublished
//   publish  publish the files that have no version yet; write nothing on disagreement
//   replace  publish, and overwrite published versions whose file changed (pre-release:
//            definitions stay at v1 until the game ships, then change as new versions)
//   export   write published versions that have no file into the directory

const usage =
  "Usage: pnpm definitions <verify|publish|replace|export> <organization-slug> <definitions-directory>";
const [action, organizationSlug, suppliedDirectory] = process.argv.slice(2);
if (
  (action !== "verify" && action !== "publish" && action !== "replace" && action !== "export") ||
  organizationSlug === undefined ||
  suppliedDirectory === undefined
) {
  throw new Error(usage);
}
// pnpm runs the script from this package; resolve paths from where it was invoked.
const directory = resolve(process.env.INIT_CWD ?? process.cwd(), suppliedDirectory);
const databaseUrl = process.env.DATABASE_URL;
if (databaseUrl === undefined) throw new Error("DATABASE_URL is required.");
const ownerFirebaseUid = process.env.LIMERICK_OWNER_FIREBASE_UID ?? "user_synthetic_owner";
const providerMode = process.env.PROVIDER_MODE ?? "fake";
if (providerMode !== "fake" && providerMode !== "live") {
  throw new Error("PROVIDER_MODE must be 'fake' or 'live'.");
}
const allowedModels = readAllowedModels(process.env, providerMode);

function report(plan: DefinitionPlan): void {
  for (const entry of plan.entries) {
    process.stdout.write(`${entry.action}\t${entry.slug}@${entry.version}\t${entry.contentHash}\n`);
  }
  for (const row of plan.unfiled) {
    process.stdout.write(`unfiled\t${row.slug}@${row.version}\t${row.contentHash}\n`);
  }
  for (const problem of plan.problems) process.stderr.write(`error: ${problem}\n`);
}

const database = createDatabase(databaseUrl);
try {
  const principal = await new PostgresCreatorIdentityRepository(database.db).findOwnerByExternalId(
    ownerFirebaseUid,
  );
  if (principal === null) throw new Error(`No organization owner for '${ownerFirebaseUid}'.`);
  const [organization] = await database.db
    .select({ slug: organizations.slug })
    .from(organizations)
    .where(eq(organizations.id, principal.organizationId))
    .limit(1);
  if (organization?.slug !== organizationSlug) {
    throw new Error(`The owner's organization is not '${organizationSlug}'.`);
  }
  const repository = new PostgresDefinitionRepository(database.db);
  const files = await readDefinitionFiles(directory);
  const slugs = [...new Set(files.map((file) => file.slug))];

  if (action === "publish" || action === "replace") {
    const plan = await publishDefinitions(repository, principal, files, allowedModels, {
      replace: action === "replace",
    });
    report(plan);
    if (plan.problems.length > 0) {
      process.exitCode = 1;
    } else {
      const count = (kind: string) => plan.entries.filter((entry) => entry.action === kind).length;
      process.stdout.write(
        `published ${count("publish")} version(s), replaced ${count("replace")}\n`,
      );
    }
  } else {
    const plan = planDefinitions(
      files,
      await repository.listPublished(principal.organizationId, slugs),
      allowedModels,
    );
    if (action === "export") {
      for (const name of await exportDefinitions(directory, plan.unfiled)) {
        process.stdout.write(`wrote ${name}\n`);
      }
    } else {
      report(plan);
      const pending = plan.entries.some((entry) => entry.action === "publish");
      if (plan.problems.length > 0 || pending) process.exitCode = 1;
      else process.stdout.write("published copies match the files\n");
    }
  }
} finally {
  await database.close();
}
