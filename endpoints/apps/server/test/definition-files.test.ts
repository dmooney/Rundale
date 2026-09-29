import { mkdtemp, readFile, readdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { afterEach, describe, expect, it } from "vitest";
import {
  definitionContentHash,
  type EndpointDefinition,
  type EndpointVersionSnapshot,
} from "@limerick/domain";
import type { CreatorPrincipal } from "../src/control/contracts.js";
import {
  exportDefinitions,
  parseDefinitionFile,
  planDefinitions,
  publishDefinitions,
  readDefinitionFiles,
  type DefinitionFile,
  type DefinitionPublishRepository,
  type PublishedDefinition,
} from "../src/control/definition-files.js";

const rundaleEndpoints = fileURLToPath(
  new URL("../../../../mods/rundale/endpoints/", import.meta.url),
);
const principal: CreatorPrincipal = { userId: "user_1", organizationId: "org_1", role: "owner" };
const fake = new Set(["fake/fake-v1"]);

function definition(instructions = "Answer briefly."): EndpointDefinition {
  return {
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
    instructions,
    providerConfig: { provider: "fake", model: "fake-v1" },
    inferenceConfig: { maxOutputTokens: 64, retryCount: 0 },
  };
}

function file(slug: string, version: number, body = definition()): DefinitionFile {
  return parseDefinitionFile(`${slug}.v${version}.json`, JSON.stringify(body));
}

function published(slug: string, version: number, body = definition()): PublishedDefinition {
  return { slug, version, contentHash: definitionContentHash(body), definition: body };
}

class MemoryDefinitionRepository implements DefinitionPublishRepository {
  readonly rows: PublishedDefinition[] = [];
  readonly publishedFiles: string[] = [];

  constructor(rows: PublishedDefinition[] = []) {
    this.rows.push(...rows);
  }

  async listPublished(_organizationId: string, slugs: readonly string[]) {
    return this.rows.filter((row) => slugs.includes(row.slug));
  }

  async publishFile(_principal: CreatorPrincipal, input: DefinitionFile) {
    let row = this.rows.find((r) => r.slug === input.slug && r.version === input.version);
    if (row === undefined) {
      row = {
        slug: input.slug,
        version: input.version,
        contentHash: input.contentHash,
        definition: input.definition,
      };
      this.rows.push(row);
      this.publishedFiles.push(input.fileName);
    }
    return {
      id: `${row.slug}@${row.version}`,
      endpointId: row.slug,
      organizationId: principal.organizationId,
      version: row.version,
      contentHash: row.contentHash,
      ...row.definition,
      publishedBy: principal.userId,
      publishedAt: new Date(0),
    } satisfies EndpointVersionSnapshot;
  }
}

describe("Endpoint definition files", () => {
  let scratch: string | undefined;
  afterEach(async () => {
    if (scratch !== undefined) await rm(scratch, { recursive: true, force: true });
    scratch = undefined;
  });

  it("takes identity from the file name and hashes the canonical body", () => {
    const body = definition();
    const parsed = parseDefinitionFile("rundale-dialogue.v2.json", JSON.stringify(body, null, 4));
    expect(parsed).toMatchObject({ slug: "rundale-dialogue", version: 2 });
    expect(parsed.contentHash).toBe(definitionContentHash(body));
  });

  it("rejects badly named files and bodies that are not exactly a definition", () => {
    const body = JSON.stringify(definition());
    for (const name of [
      "dialogue.json",
      "Rundale.v1.json",
      "rundale.v0.json",
      "rundale.v01.json",
    ]) {
      expect(() => parseDefinitionFile(name, body)).toThrow(/<slug>\.v<version>\.json/);
    }
    expect(() => parseDefinitionFile("a.v1.json", "{")).toThrow(/not valid JSON/);
    expect(() =>
      parseDefinitionFile("a.v1.json", JSON.stringify({ ...definition(), slug: "a" })),
    ).toThrow(/exactly/);
    const partial: Partial<EndpointDefinition> = definition();
    delete partial.instructions;
    expect(() => parseDefinitionFile("a.v1.json", JSON.stringify(partial))).toThrow(/exactly/);
  });

  it("plans new files for publication and leaves matching copies unchanged", () => {
    const plan = planDefinitions(
      [file("a", 1), file("a", 2, definition("Second.")), file("b", 1)],
      [published("a", 1)],
      fake,
    );
    expect(plan.problems).toEqual([]);
    expect(plan.entries.map((entry) => [entry.slug, entry.version, entry.action])).toEqual([
      ["a", 1, "unchanged"],
      ["a", 2, "publish"],
      ["b", 1, "publish"],
    ]);
  });

  it("reports a published copy that differs from its file", () => {
    const plan = planDefinitions([file("a", 1, definition("Edited."))], [published("a", 1)], fake);
    expect(plan.entries.map((entry) => entry.action)).toEqual(["differs"]);
    expect(plan.problems).toHaveLength(1);
    expect(plan.problems[0]).toMatch(
      /a\.v1\.json: published sha256:.* differs .* new version file/,
    );
  });

  it("reports a stored row whose content no longer matches its recorded hash", () => {
    const row = { ...published("a", 1), definition: definition("Tampered.") };
    const plan = planDefinitions([], [row], fake);
    expect(plan.problems.some((problem) => /does not match its stored content/.test(problem))).toBe(
      true,
    );
  });

  it("reports published versions that have no file", () => {
    const plan = planDefinitions([file("a", 1)], [published("a", 1), published("a", 2)], fake);
    expect(plan.unfiled.map((row) => row.version)).toEqual([2]);
    expect(plan.problems).toEqual([
      "a@2: published with no a.v2.json; export it into the definitions directory.",
    ]);
  });

  it("validates new files against the deployment's rules before publishing", () => {
    const body = definition();
    body.providerConfig = { provider: "google", model: "not-allowed" };
    const plan = planDefinitions([file("a", 1, body)], [], fake);
    expect(plan.problems).toEqual(["a.v1.json: Model 'google/not-allowed' is not allowed."]);
  });

  it("publishes only missing versions, and nothing when anything disagrees", async () => {
    const repository = new MemoryDefinitionRepository([published("a", 1)]);
    const plan = await publishDefinitions(
      repository,
      principal,
      [file("a", 1), file("a", 2, definition("Second."))],
      fake,
    );
    expect(plan.problems).toEqual([]);
    expect(repository.publishedFiles).toEqual(["a.v2.json"]);

    const blocked = new MemoryDefinitionRepository([published("a", 1)]);
    const rejected = await publishDefinitions(
      blocked,
      principal,
      [file("a", 1, definition("Edited.")), file("b", 1)],
      fake,
    );
    expect(rejected.problems).toHaveLength(1);
    expect(blocked.publishedFiles).toEqual([]);
  });

  it("fails when a version was published concurrently with other content", async () => {
    const repository = new MemoryDefinitionRepository();
    repository.listPublished = async () => [];
    repository.rows.push(published("a", 1, definition("Raced.")));
    await expect(publishDefinitions(repository, principal, [file("a", 1)], fake)).rejects.toThrow(
      /published concurrently/,
    );
  });

  it("exports unfiled versions as files that hash to the published copy", async () => {
    scratch = await mkdtemp(join(tmpdir(), "definitions-"));
    const row = published("a", 3, definition("Exported."));
    expect(await exportDefinitions(scratch, [row])).toEqual(["a.v3.json"]);
    const [read] = await readDefinitionFiles(scratch);
    expect(read).toMatchObject({ slug: "a", version: 3, contentHash: row.contentHash });
    await expect(exportDefinitions(scratch, [row])).rejects.toThrow(/EEXIST/);
    await writeFile(join(scratch, "notes.txt"), "ignored");
    expect((await readDefinitionFiles(scratch)).map((f) => f.fileName)).toEqual(["a.v3.json"]);
  });

  it("reads the Rundale mod's definitions as publishable Google versions", async () => {
    const files = await readDefinitionFiles(rundaleEndpoints);
    expect(files.map((f) => f.fileName)).toEqual(
      (await readdir(rundaleEndpoints)).filter((name) => name.endsWith(".json")).sort(),
    );
    const plan = planDefinitions(files, [], new Set(["google/gemini-3.5-flash-lite"]));
    expect(plan.problems).toEqual([]);
    const dialogue = files.find((f) => f.slug === "rundale-dialogue" && f.version === 1)!;
    expect(dialogue.contentHash).toBe(
      "sha256:d2a58dc263543789c19a3bc5d3d934db7fee7e8fba81d5d01716bcf03315cae1",
    );
    expect(dialogue.definition).toEqual(
      JSON.parse(await readFile(join(rundaleEndpoints, "rundale-dialogue.v1.json"), "utf8")),
    );
  });
});
