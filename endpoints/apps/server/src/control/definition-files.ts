import { readdir, readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";
import {
  definitionContentHash,
  type EndpointDefinition,
  type EndpointVersionSnapshot,
  type Id,
} from "@limerick/domain";
import type { CreatorPrincipal } from "./contracts.js";
import { ControlError, validateDefinition } from "./service.js";

// Endpoint definitions authored as files: `<slug>.v<version>.json`, whose body
// is exactly an EndpointDefinition. The file name is the version's identity,
// the file is the source of truth, and the database holds published copies.

const fileNamePattern = /^([a-z0-9]+(?:-[a-z0-9]+)*)\.v([1-9][0-9]*)\.json$/;
const definitionKeys = [
  "inputSchema",
  "outputSchema",
  "instructions",
  "providerConfig",
  "inferenceConfig",
] as const;

export interface DefinitionFile {
  slug: string;
  version: number;
  fileName: string;
  definition: EndpointDefinition;
  contentHash: string;
}

export interface PublishedDefinition {
  slug: string;
  version: number;
  contentHash: string;
  definition: EndpointDefinition;
}

export interface DefinitionPublishRepository {
  /** Every published version of the organization's Endpoints with these slugs. */
  listPublished(organizationId: Id, slugs: readonly string[]): Promise<PublishedDefinition[]>;
  /**
   * Inserts the file as the exact published version, creating the Endpoint
   * (with the file as its Draft) when the slug is new. Returns the stored
   * version; if the version already exists, returns it unchanged.
   */
  publishFile(principal: CreatorPrincipal, file: DefinitionFile): Promise<EndpointVersionSnapshot>;
  /** Overwrites an existing version's content with the file (pre-release only). */
  replaceFile(principal: CreatorPrincipal, file: DefinitionFile): Promise<EndpointVersionSnapshot>;
}

export function definitionFileName(slug: string, version: number): string {
  return `${slug}.v${version}.json`;
}

export function parseDefinitionFile(fileName: string, text: string): DefinitionFile {
  const match = fileNamePattern.exec(fileName);
  if (match === null) {
    throw new ControlError(
      "INVALID_DEFINITION",
      `${fileName}: definition files are named <slug>.v<version>.json.`,
    );
  }
  const version = Number(match[2]);
  if (!Number.isSafeInteger(version)) {
    throw new ControlError("INVALID_DEFINITION", `${fileName}: version is out of range.`);
  }
  let parsed: unknown;
  try {
    parsed = JSON.parse(text);
  } catch {
    throw new ControlError("INVALID_DEFINITION", `${fileName}: not valid JSON.`);
  }
  if (parsed === null || typeof parsed !== "object" || Array.isArray(parsed)) {
    throw new ControlError("INVALID_DEFINITION", `${fileName}: must contain a JSON object.`);
  }
  const keys = Object.keys(parsed).sort();
  if (keys.join() !== [...definitionKeys].sort().join()) {
    throw new ControlError(
      "INVALID_DEFINITION",
      `${fileName}: must contain exactly ${definitionKeys.join(", ")}.`,
    );
  }
  const definition = parsed as EndpointDefinition;
  return {
    slug: match[1]!,
    version,
    fileName,
    definition,
    contentHash: definitionContentHash(definition),
  };
}

/** Reads every definition file in a directory, in slug and version order. */
export async function readDefinitionFiles(directory: string): Promise<DefinitionFile[]> {
  const names = (await readdir(directory)).filter((name) => name.endsWith(".json"));
  const files = await Promise.all(
    names.map(async (name) =>
      parseDefinitionFile(name, await readFile(join(directory, name), "utf8")),
    ),
  );
  return files.sort(compareVersions);
}

export type DefinitionAction = "unchanged" | "publish" | "replace" | "differs";

export interface DefinitionPlanEntry {
  slug: string;
  version: number;
  contentHash: string;
  action: DefinitionAction;
}

export interface DefinitionPlan {
  entries: DefinitionPlanEntry[];
  /** Published versions of these slugs that have no file. */
  unfiled: PublishedDefinition[];
  /** Differences that make the files and the database disagree. */
  problems: string[];
}

function compareVersions(
  left: { slug: string; version: number },
  right: { slug: string; version: number },
): number {
  return left.slug.localeCompare(right.slug) || left.version - right.version;
}

export interface DefinitionPlanOptions {
  /**
   * Before release, a changed file replaces its published version in place
   * instead of being refused, so definitions stay at v1 until the game ships.
   */
  replace?: boolean;
}

/**
 * Compares the files with the published copies. A published version must
 * equal its file in canonical form, unless `replace` is set; a file with no
 * published version is to be published; a published version with no file is
 * reported, since the database may only hold copies of files.
 */
export function planDefinitions(
  files: readonly DefinitionFile[],
  published: readonly PublishedDefinition[],
  allowedModels: ReadonlySet<string>,
  options: DefinitionPlanOptions = {},
): DefinitionPlan {
  const problems: string[] = [];
  const entries: DefinitionPlanEntry[] = [];
  const key = (slug: string, version: number) => `${slug}@${version}`;
  const stored = new Map(published.map((row) => [key(row.slug, row.version), row]));
  for (const row of published) {
    const recomputed = definitionContentHash(row.definition);
    if (recomputed !== row.contentHash) {
      problems.push(
        `${row.slug}@${row.version}: stored content hash ${row.contentHash} does not match its stored content (${recomputed}).`,
      );
    }
  }
  for (const file of files) {
    const row = stored.get(key(file.slug, file.version));
    const validate = () => {
      try {
        validateDefinition(file.definition, allowedModels);
      } catch (error) {
        const reason = error instanceof Error ? error.message : String(error);
        problems.push(`${file.fileName}: ${reason}`);
      }
    };
    if (row === undefined) {
      validate();
      entries.push({ ...pick(file), action: "publish" });
    } else if (row.contentHash === file.contentHash) {
      entries.push({ ...pick(file), action: "unchanged" });
    } else if (options.replace === true) {
      validate();
      entries.push({ ...pick(file), action: "replace" });
    } else {
      problems.push(
        `${file.fileName}: published ${row.contentHash} differs from the file's ${file.contentHash}; replace it in place before release, or ship the change as a new version file after.`,
      );
      entries.push({ ...pick(file), action: "differs" });
    }
  }
  const filed = new Set(files.map((file) => key(file.slug, file.version)));
  const unfiled = published
    .filter((row) => !filed.has(key(row.slug, row.version)))
    .sort(compareVersions);
  for (const row of unfiled) {
    problems.push(
      `${row.slug}@${row.version}: published with no ${definitionFileName(row.slug, row.version)}; export it into the definitions directory.`,
    );
  }
  return { entries, unfiled, problems };
}

function pick(file: DefinitionFile): Omit<DefinitionPlanEntry, "action"> {
  return { slug: file.slug, version: file.version, contentHash: file.contentHash };
}

/**
 * Publishes the files that have no published version, and with `replace`
 * overwrites changed ones, after the whole plan checks out. Nothing is
 * written when any file or published copy disagrees.
 */
export async function publishDefinitions(
  repository: DefinitionPublishRepository,
  principal: CreatorPrincipal,
  files: readonly DefinitionFile[],
  allowedModels: ReadonlySet<string>,
  options: DefinitionPlanOptions = {},
): Promise<DefinitionPlan> {
  const slugs = [...new Set(files.map((file) => file.slug))];
  const plan = planDefinitions(
    files,
    await repository.listPublished(principal.organizationId, slugs),
    allowedModels,
    options,
  );
  if (plan.problems.length > 0) return plan;
  for (const entry of plan.entries) {
    if (entry.action !== "publish" && entry.action !== "replace") continue;
    const file = files.find(
      (candidate) => candidate.slug === entry.slug && candidate.version === entry.version,
    )!;
    const version =
      entry.action === "publish"
        ? await repository.publishFile(principal, file)
        : await repository.replaceFile(principal, file);
    if (version.contentHash !== file.contentHash) {
      throw new ControlError(
        "CONFLICT",
        `${file.fileName}: published concurrently as ${version.contentHash}.`,
      );
    }
  }
  return plan;
}

/** Writes each published version that has no file into the directory. */
export async function exportDefinitions(
  directory: string,
  unfiled: readonly PublishedDefinition[],
): Promise<string[]> {
  const written: string[] = [];
  for (const row of unfiled) {
    const fileName = definitionFileName(row.slug, row.version);
    const body = Object.fromEntries(definitionKeys.map((name) => [name, row.definition[name]]));
    await writeFile(join(directory, fileName), `${JSON.stringify(body, null, 2)}\n`, {
      flag: "wx",
    });
    written.push(fileName);
  }
  return written;
}
