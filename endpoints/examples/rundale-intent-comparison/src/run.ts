// Runs the System 1 input comparison of Rundale issue #2220 (design
// docs/design/dynamic-npc-context.md §4.6 and §5): each decision model on
// every labelled case through the real decision runtime and Decisions
// adapter, answering `rundale-intent`, and the engine path before Jev as the baseline
// through the mobile FFI harness, with `rundale-intent` answered live by Gemini.
//
//   TYPESAFE_API_KEY=… GOOGLE_API_KEY=… pnpm --filter \
//     @limerick/rundale-intent-comparison compare --out <dir> [--runs 5]
//
// Since #2221 the engine path the baseline measured is gone, so a new run
// reuses a recorded baseline with `--baseline-from <earlier raw.json>`
// (no GOOGLE_API_KEY needed). Outcomes follow the recipient policy of
// design §3.1 as amended on 2026-10-17.
//
// Writes <dir>/requests.json, <dir>/raw.json (every call) and
// <dir>/summary.json. Keep <dir> outside the repository: run data is archived,
// not committed.

import { spawnSync } from "node:child_process";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import {
  definitionContentHash,
  type EndpointDefinition,
  type EndpointVersionSnapshot,
} from "@limerick/domain";
import {
  DecisionsProvider,
  FixedPriceCostCalculator,
  GoogleProvider,
  StaticProviderRegistry,
  type ModelPrice,
} from "@limerick/providers";
import { buildDecisionRequest, DeterministicRuntime, RuntimeError } from "@limerick/runtime";
import { cases as allCases, type LabelledCase } from "./cases.js";
import {
  baselineOutcome,
  decisionRuns,
  resolveInput,
  score,
  startingThresholds,
  engineThresholds,
  type BaselineObservation,
  type DecisionOutput,
  type Outcome,
  type Thresholds,
  localOutcomes,
  type LocalOutcomes,
} from "./policy.js";
import { buildInput, loadPeople, loadPlaces } from "./world.js";

const repo = resolve(fileURLToPath(new URL("../../../..", import.meta.url)));
const definitions = join(repo, "mods/rundale/endpoints");
const npcsPath = join(repo, "mods/rundale/npcs.json");
const places = loadPlaces(join(repo, "mods/rundale/world.json"));
/** `--definition <file>`: the decision definition to run, `rundale-intent` v1 by default. */
const definitionPath = process.argv.includes("--definition")
  ? resolve(
      process.env.INIT_CWD ?? process.cwd(),
      process.argv[process.argv.indexOf("--definition") + 1] ?? "",
    )
  : join(definitions, "rundale-intent.v1.json");

/** `--appendix A` or `--appendix B` runs one appendix; both by default. */
const appendix = process.argv.includes("--appendix")
  ? process.argv[process.argv.indexOf("--appendix") + 1]
  : undefined;
export const cases = allCases.filter(
  (item) => appendix === undefined || item.id.startsWith(appendix),
);

/**
 * TypeSafe model ids to run: `--models a,b`. Jev 1.13 alone by default (§4.6),
 * pinned to a versioned id because the thresholds were tuned against it and
 * TypeSafe's `jev-latest` alias moves with each release.
 */
export const candidates = (
  process.argv.includes("--models")
    ? (process.argv[process.argv.indexOf("--models") + 1] ?? "")
    : "jev-1.13.0"
)
  .split(",")
  .filter(Boolean);

const inputFor = (item: LabelledCase) =>
  buildInput(item, loadPeople(npcsPath, item.fixture), places);
const intentModel = "gemini-3.5-flash-lite";
/** limerick-prod's MODEL_PRICES_JSON entry for the intent model, read 2026-10-10. */
const intentPrice: ModelPrice = { inputPerMillionUsd: 0.3, outputPerMillionUsd: 2.5 };
/** Design §4.4: a decision call that takes longer than this is uncertain. */
const decisionTimeoutMs = 2_000;

function argument(name: string, fallback?: string): string {
  const index = process.argv.indexOf(`--${name}`);
  const value = index >= 0 ? process.argv[index + 1] : fallback;
  if (value === undefined) throw new Error(`--${name} is required`);
  return value;
}

function snapshot(path: string, model?: string): EndpointVersionSnapshot {
  const definition = JSON.parse(readFileSync(path, "utf8")) as EndpointDefinition;
  return {
    ...definition,
    providerConfig:
      model === undefined
        ? definition.providerConfig
        : { ...definition.providerConfig, provider: "typesafe", model },
    id: path,
    endpointId: path,
    organizationId: "comparison",
    version: 1,
    contentHash: definitionContentHash(definition),
    publishedBy: "comparison",
    publishedAt: new Date(),
  };
}

/**
 * The input values for a case, fitted to a definition's input schema: fields
 * it does not declare are left out, its role is used, and an older reference
 * shape that lists option ids (`fits`) gets them from the people's names.
 */
function valuesFor(item: LabelledCase, version: EndpointVersionSnapshot) {
  const built = inputFor(item);
  const properties = (version.inputSchema.properties ?? {}) as Record<
    string,
    { enum?: string[]; items?: { properties?: Record<string, unknown> } }
  >;
  const values: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(built.values)) {
    if (Object.hasOwn(properties, key)) values[key] = value;
  }
  const role = properties.role?.enum?.[0];
  if (role !== undefined) values.role = role;
  if (Object.hasOwn(properties.references?.items?.properties ?? {}, "fits")) {
    const optionOf = new Map(built.values.present.map((option) => [option.name, option.optionId]));
    values.references = built.values.references.map((reference) => ({
      optionId: reference.optionId,
      phrase: reference.phrase,
      fits: reference.people.map((name) => optionOf.get(name)!),
    }));
  }
  return { built, values };
}

/** The exact state and questions each case sends, for the record. */
function writeRequests(out: string, version: EndpointVersionSnapshot): void {
  const decision = version.inferenceConfig.decision!;
  const requests = cases.filter(decisionRuns).map((item) => {
    const { values } = valuesFor(item, version);
    const built = buildDecisionRequest(decision, "<model>", values, version.instructions);
    return { id: item.id, state: built.invocation.state, questions: built.invocation.questions };
  });
  writeFileSync(
    join(out, "requests.json"),
    `${JSON.stringify({ definition: version.contentHash, requests }, null, 2)}\n`,
  );
}

function envelope(caseId: string, run: number) {
  const id = `${caseId}-${run}`;
  return {
    contractVersion: { major: 1, minor: 0 },
    sessionID: "comparison",
    logicalRequestID: id,
    attemptID: id,
    baseRevision: { rawValue: 0 },
    idempotencyKey: id,
  };
}

function signal(ms: number) {
  return {
    requestId: "comparison",
    deadline: new Date(Date.now() + ms),
    signal: AbortSignal.timeout(ms),
  };
}

/**
 * TypeSafe's published price for Jev 1.13 and 1.14: $0.042 per million input
 * tokens, output free (https://docs.typesafe.ai/models, read 2026-10-16).
 */
function decisionPrices(): Record<string, ModelPrice> {
  return Object.fromEntries(
    candidates.map((model) => [
      `typesafe/${model}`,
      { inputPerMillionUsd: 0.042, outputPerMillionUsd: 0 },
    ]),
  );
}

interface DecisionCall {
  model: string;
  id: string;
  run: number;
  ms: number;
  costUsd: number;
  output?: DecisionOutput;
  error?: string;
}

async function runCandidates(runs: number, prices: Record<string, ModelPrice>) {
  const key = process.env.TYPESAFE_API_KEY;
  if (!key) throw new Error("TYPESAFE_API_KEY is required");
  const runtime = new DeterministicRuntime(
    new StaticProviderRegistry([new DecisionsProvider("typesafe", key)]),
    new FixedPriceCostCalculator(prices),
  );
  const calls: DecisionCall[] = [];
  for (const model of candidates) {
    const version = snapshot(definitionPath, model);
    for (const item of cases.filter(decisionRuns)) {
      const input = { values: valuesFor(item, version).values };
      for (let run = 0; run < runs; run += 1) {
        const started = performance.now();
        try {
          const result = await runtime.invoke(
            version,
            { values: { ...envelope(item.id, run), ...input.values }, attachments: [] },
            signal(decisionTimeoutMs),
          );
          calls.push({
            model,
            id: item.id,
            run,
            ms: performance.now() - started,
            costUsd: Number(result.estimatedCostUsd),
            output: result.output as DecisionOutput,
          });
        } catch (error) {
          calls.push({
            model,
            id: item.id,
            run,
            ms: performance.now() - started,
            costUsd: 0,
            error: error instanceof RuntimeError ? error.code : String(error),
          });
        }
      }
      process.stdout.write(".");
    }
    process.stdout.write(` ${model}\n`);
  }
  return calls;
}

interface IntentCall {
  id: string;
  run: number;
  ms: number;
  costUsd: number;
  output: unknown;
  error?: string;
}

const unknownIntent = { intent: "unknown", target: null, dialogue: null, atmosphere: null };

async function recordIntents(runs: number): Promise<IntentCall[]> {
  const key = process.env.GOOGLE_API_KEY;
  if (!key) throw new Error("GOOGLE_API_KEY is required");
  const runtime = new DeterministicRuntime(
    new StaticProviderRegistry([GoogleProvider.fromApiKey(key)]),
    new FixedPriceCostCalculator({ [`google/${intentModel}`]: intentPrice }),
  );
  // The Gemini intent Endpoint Jev replaced, kept with the comparison's results.
  const version = snapshot(
    join(repo, "docs/test-plans/results/2026-10-10-input-comparison/rundale-intent-gemini.v1.json"),
  );
  const calls: IntentCall[] = [];
  for (const item of cases) {
    for (let run = 0; run < runs; run += 1) {
      const started = performance.now();
      try {
        const result = await runtime.invoke(
          version,
          {
            values: { ...envelope(item.id, run), role: "player_intent", playerInput: item.line },
            attachments: [],
          },
          signal(30_000),
        );
        calls.push({
          id: item.id,
          run,
          ms: performance.now() - started,
          costUsd: Number(result.estimatedCostUsd),
          output: result.output,
        });
      } catch (error) {
        calls.push({
          id: item.id,
          run,
          ms: performance.now() - started,
          costUsd: 0,
          output: unknownIntent,
          error: error instanceof RuntimeError ? error.code : String(error),
        });
      }
    }
    process.stdout.write(".");
  }
  process.stdout.write(" baseline intents\n");
  return calls;
}

function runBaseline(out: string, intents: IntentCall[]): BaselineObservation[] {
  const input = join(out, "baseline-in.json");
  const output = join(out, "baseline-out.json");
  writeFileSync(
    input,
    JSON.stringify({
      cases: cases.map((item) => ({
        id: item.id,
        place: item.place,
        fixture: item.fixture ?? null,
        present: item.present,
        introduced: item.introduced,
        line: item.line,
        intents: intents.filter((call) => call.id === item.id).map((call) => call.output),
      })),
    }),
  );
  const cargo = spawnSync(
    "cargo",
    [
      "test",
      "-p",
      "limerick-mobile-ffi",
      "--lib",
      "tests::addressee_baseline",
      "--",
      "--ignored",
      "--exact",
    ],
    {
      cwd: join(repo, "limerick"),
      env: { ...process.env, ADDRESSEE_BASELINE_IN: input, ADDRESSEE_BASELINE_OUT: output },
      stdio: "inherit",
    },
  );
  if (cargo.status !== 0) throw new Error("the baseline harness failed");
  return (JSON.parse(readFileSync(output, "utf8")) as { observations: BaselineObservation[] })
    .observations;
}

function percentile(values: number[], p: number): number | null {
  if (values.length === 0) return null;
  const sorted = [...values].sort((a, b) => a - b);
  return sorted[Math.min(sorted.length - 1, Math.ceil((p / 100) * sorted.length) - 1)]!;
}

interface Tally {
  runs: number;
  correct: number;
  confidentWrongRoutes: number;
  unnecessaryQuestions: number;
  wrong: { id: string; run: number; outcome: Outcome }[];
}

function tally(scored: { item: LabelledCase; run: number; outcome: Outcome }[]): Tally {
  const result: Tally = {
    runs: scored.length,
    correct: 0,
    confidentWrongRoutes: 0,
    unnecessaryQuestions: 0,
    wrong: [],
  };
  for (const { item, run, outcome } of scored) {
    const s = score(item.expected, outcome);
    if (s.correct) result.correct += 1;
    else result.wrong.push({ id: item.id, run, outcome });
    if (s.confidentWrongRoute) result.confidentWrongRoutes += 1;
    if (s.unnecessaryQuestion) result.unnecessaryQuestions += 1;
  }
  return result;
}

/** Set from the baseline before scoring. */
let settledLocally: LocalOutcomes = new Map();

export function scoreCandidate(
  calls: DecisionCall[],
  model: string,
  runs: number,
  thresholds: Thresholds,
  local: LocalOutcomes = settledLocally,
): Tally {
  const scored = cases.flatMap((item) => {
    const input = inputFor(item);
    return Array.from({ length: runs }, (_, run) => {
      const settled = local.get(`${item.id}#${run}`);
      if (settled !== undefined) return { item, run, outcome: settled };
      const call = calls.find((c) => c.model === model && c.id === item.id && c.run === run);
      return { item, run, outcome: resolveInput(item, input, call?.output, thresholds) };
    });
  });
  return tally(scored);
}

/**
 * The thresholds that give the most correct outcomes with no confident wrong
 * route, then the fewest unnecessary questions, then the values nearest the
 * starting ones.
 */
function bestThresholds(calls: DecisionCall[], model: string, runs: number) {
  let best: { thresholds: Thresholds; tally: Tally } | undefined;
  for (let choice = 0.3; choice <= 0.951; choice += 0.05) {
    for (let mention = 0.3; mention <= 0.951; mention += 0.05) {
      const thresholds = { choice: round(choice), mention: round(mention) };
      const result = scoreCandidate(calls, model, runs, thresholds);
      if (result.confidentWrongRoutes > 0) continue;
      const distance = (t: Thresholds) =>
        Math.abs(t.choice - startingThresholds.choice) +
        Math.abs(t.mention - startingThresholds.mention);
      if (
        best === undefined ||
        result.correct > best.tally.correct ||
        (result.correct === best.tally.correct &&
          (result.unnecessaryQuestions < best.tally.unnecessaryQuestions ||
            (result.unnecessaryQuestions === best.tally.unnecessaryQuestions &&
              distance(thresholds) < distance(best.thresholds))))
      ) {
        best = { thresholds, tally: result };
      }
    }
  }
  return best;
}

const round = (value: number) => Math.round(value * 100) / 100;

/** `correct/total` over the runs of cases that label an atmosphere subject. */
function atmosphereScore(answer: (id: string, run: number) => string, runs: number): string {
  const labelled = cases.filter((item) => item.atmosphere !== undefined);
  let correct = 0;
  for (const item of labelled) {
    for (let run = 0; run < runs; run += 1)
      if (answer(item.id, run) === item.atmosphere) correct += 1;
  }
  return `${correct}/${labelled.length * runs}`;
}

async function main() {
  // pnpm runs the script in this package; resolve --out from where it was invoked.
  const out = resolve(process.env.INIT_CWD ?? process.cwd(), argument("out"));
  const runs = Number(argument("runs", "5"));
  let scored = candidates;
  mkdirSync(out, { recursive: true });
  writeRequests(out, snapshot(definitionPath));
  if (process.argv.includes("--requests-only")) return;
  let raw: {
    prices: Record<string, ModelPrice>;
    decisions: DecisionCall[];
    intents: IntentCall[];
    observations: BaselineObservation[];
  };
  if (process.argv.includes("--rescore")) {
    // Re-scores a recorded run without calling any model.
    raw = JSON.parse(readFileSync(join(out, "raw.json"), "utf8")) as typeof raw;
    // Archived runs name models as they were called (OpenRouter ids before
    // 2026-10-16), so score what the run recorded unless --models says otherwise.
    if (!process.argv.includes("--models")) {
      scored = [...new Set(raw.decisions.map((call) => call.model))];
    }
  } else {
    const prices = decisionPrices();
    const decisions = await runCandidates(runs, prices);
    // `--baseline-from <raw.json>` reuses a recorded run's baseline (its
    // Gemini intents and the engine before Jev), which no longer exists to
    // run again; the candidates are still called live.
    const baselineFrom = process.argv.includes("--baseline-from")
      ? resolve(
          process.env.INIT_CWD ?? process.cwd(),
          process.argv[process.argv.indexOf("--baseline-from") + 1] ?? "",
        )
      : undefined;
    let intents: IntentCall[];
    let observations: BaselineObservation[];
    if (baselineFrom === undefined) {
      intents = await recordIntents(runs);
      observations = runBaseline(out, intents);
    } else {
      const recorded = JSON.parse(readFileSync(baselineFrom, "utf8")) as typeof raw;
      intents = recorded.intents;
      observations = recorded.observations;
    }
    raw = { prices, decisions, intents, observations };
    writeFileSync(join(out, "raw.json"), `${JSON.stringify(raw, null, 2)}\n`);
  }
  const { decisions, intents, observations } = raw;

  const people = loadPeople(npcsPath, "mary_elsewhere");
  const nameToId = new Map([...people.values()].map((person) => [person.name, person.id]));
  const caseOf = (id: string) => cases.find((c) => c.id === id)!;
  settledLocally = localOutcomes(observations, (observation) =>
    baselineOutcome(caseOf(observation.id), observation, nameToId, places),
  );
  const baseline = tally(
    observations.map((observation) => {
      const item = cases.find((c) => c.id === observation.id)!;
      return {
        item,
        run: observation.run,
        outcome: baselineOutcome(item, observation, nameToId, places),
      };
    }),
  );
  const intentMs = intents
    .filter((call) =>
      observations.some((o) => o.id === call.id && o.run === call.run && o.intentCalls > 0),
    )
    .map((call) => call.ms);
  const intentCostPerLine =
    intents
      .filter((call) =>
        observations.some((o) => o.id === call.id && o.run === call.run && o.intentCalls > 0),
      )
      .reduce((sum, call) => sum + call.costUsd, 0) / observations.length;

  const summary = {
    date: new Date().toISOString(),
    runsPerCase: runs,
    cases: cases.length,
    baseline: {
      ...baseline,
      intentCallShare: intentMs.length / observations.length,
      intentP50Ms: percentile(intentMs, 50),
      intentP95Ms: percentile(intentMs, 95),
      costPer1000LinesUsd: intentCostPerLine * 1000,
      atmosphere: atmosphereScore((id, run) => {
        const output = intents.find((call) => call.id === id && call.run === run)?.output as
          { atmosphere?: string | null } | undefined;
        return output?.atmosphere ?? "none";
      }, runs),
    },
    candidates: scored.map((model) => {
      const own = decisions.filter((call) => call.model === model);
      // Lines the local parser settles never reach the model in the game, so
      // latency and cost count only the calls the game would make, as the
      // baseline counts only its intent calls.
      const made = own.filter((call) => !settledLocally.has(`${call.id}#${call.run}`));
      const ms = made.map((call) => call.ms);
      return {
        model,
        errors: own
          .filter((call) => call.error !== undefined)
          .map((c) => `${c.id}#${c.run}:${c.error}`),
        p50Ms: percentile(ms, 50),
        p95Ms: percentile(ms, 95),
        // Lines where System 1 does not run (nobody present or met) cost nothing.
        costPer1000LinesUsd:
          (made.reduce((sum, call) => sum + call.costUsd, 0) / (cases.length * runs)) * 1000,
        // Runs where the atmosphere answer matches a labelled case's subject.
        atmosphere: atmosphereScore((id, run) => {
          const output = own.find((call) => call.id === id && call.run === run)?.output;
          const answer = output?.atmosphere;
          return answer !== undefined && (answer.probabilities[answer.choice] ?? 0) >= 0.7
            ? answer.choice
            : "none";
        }, runs),
        atStartingThresholds: scoreCandidate(decisions, model, runs, startingThresholds),
        // The engine's thresholds (design §4.3): 0.70 for both questions.
        atEngineThresholds: scoreCandidate(decisions, model, runs, engineThresholds),
        best: bestThresholds(decisions, model, runs) ?? null,
        // correct/confident wrong routes/unnecessary questions per threshold pair.
        sensitivity: Object.fromEntries(
          [0.6, 0.65, 0.7, 0.75, 0.8, 0.85, 0.9, 0.95].map((choice) => [
            choice.toFixed(2),
            Object.fromEntries(
              [0.5, 0.6, 0.7, 0.8, 0.9].map((mention) => {
                const t = scoreCandidate(decisions, model, runs, { choice, mention });
                return [
                  mention.toFixed(2),
                  `${t.correct}/${t.confidentWrongRoutes}/${t.unnecessaryQuestions}`,
                ];
              }),
            ),
          ]),
        ),
      };
    }),
  };
  writeFileSync(join(out, "summary.json"), `${JSON.stringify(summary, null, 2)}\n`);
  for (const candidate of summary.candidates) {
    const start = candidate.atEngineThresholds;
    console.log(
      `${candidate.model}: ${start.correct}/${start.runs} correct, ${start.confidentWrongRoutes} wrong routes, ${start.unnecessaryQuestions} unnecessary questions at 0.70/0.70; p50 ${candidate.p50Ms?.toFixed(0)} ms, p95 ${candidate.p95Ms?.toFixed(0)} ms`,
    );
  }
  console.log(
    `baseline: ${baseline.correct}/${baseline.runs} correct, ${baseline.confidentWrongRoutes} wrong routes, ${baseline.unnecessaryQuestions} unnecessary questions; intent p95 ${summary.baseline.intentP95Ms?.toFixed(0)} ms`,
  );
}

if (process.argv[1] !== undefined && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  await main();
}
