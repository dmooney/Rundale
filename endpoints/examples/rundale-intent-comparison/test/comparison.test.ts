import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { cases, MICHEAL, OFFICE, PEIG, ROISIN } from "../src/cases.js";
import {
  baselineOutcome,
  localOutcomes,
  resolveDecision,
  resolveInput,
  score,
  startingThresholds,
  type BaselineObservation,
  type DecisionOutput,
} from "../src/policy.js";
import { buildInput, candidatePhrases, fold, loadPeople, loadPlaces } from "../src/world.js";

const npcs = fileURLToPath(new URL("../../../../mods/rundale/npcs.json", import.meta.url));
const worldPath = fileURLToPath(new URL("../../../../mods/rundale/world.json", import.meta.url));
const byId = (id: string) => cases.find((item) => item.id === id)!;

function answer(choice: string, p = 0.95, mentioned: Record<string, number> = {}): DecisionOutput {
  return { recipient: { choice, confidence: p, probabilities: { [choice]: p } }, mentioned };
}

describe("comparison input", () => {
  it("covers Appendices A and B", () => {
    expect(cases.map((item) => item.id)).toEqual([
      ...Array.from({ length: 35 }, (_, index) => `A${index + 1}`),
      ...Array.from({ length: 20 }, (_, index) => `B${index + 1}`),
    ]);
  });

  it("cuts candidate phrases the player typed, for the model to select", () => {
    expect(candidatePhrases("latch the gate behind me")).toEqual(["the gate"]);
    expect(candidatePhrases("examine Peig's satchel")).toContain("Peig's satchel");
    expect(candidatePhrases("go to the fairy fort")).toContain("the fairy fort");
  });

  it("folds accents typed precomposed or as combining marks", () => {
    expect(fold("Róisín")).toBe("roisin");
    expect(fold("Róisín")).toBe("roisin");
  });

  it("offers people present, then people met elsewhere, in roster order", () => {
    const built = buildInput(byId("A21"), loadPeople(npcs));
    expect(built.values.present.map((p) => [p.optionId, p.name])).toEqual([
      ["p1", "Mícheál Connolly"],
      ["p2", "Róisín Connolly"],
    ]);
    expect(built.values.elsewhere).toEqual([{ optionId: "e1", name: "Peig Hannigan" }]);
    expect(built.values.present.every((p) => p.introduced)).toBe(true);
  });

  it("leaves out absent people the player has not met", () => {
    expect(buildInput(byId("A22"), loadPeople(npcs)).values.elsewhere).toEqual([]);
  });

  it("finds a family name and a shared role as references", () => {
    expect(buildInput(byId("A9"), loadPeople(npcs)).values.references).toEqual([
      { optionId: "r1", phrase: "Connolly", people: ["Mícheál Connolly", "Róisín Connolly"] },
    ]);
    const drovers = byId("A34");
    expect(
      buildInput(drovers, loadPeople(npcs, drovers.fixture)).values.references.map((r) => r.phrase),
    ).toEqual(["Drover"]);
    expect(buildInput(byId("A15"), loadPeople(npcs)).values.references).toEqual([]);
  });
});

describe("resolution policy", () => {
  const people = loadPeople(npcs);
  const resolve = (id: string, output?: DecisionOutput) =>
    resolveDecision(byId(id), buildInput(byId(id), people), output, startingThresholds);

  it("asks rather than guessing when the answer is uncertain", () => {
    expect(resolve("A5", answer("p2", 0.6))).toEqual({ kind: "ask", mentioned: [] });
    expect(resolve("A5", undefined)).toEqual({ kind: "ask", mentioned: [] });
    expect(resolve("A23", answer("p1", 0.4))).toEqual({
      kind: "send",
      recipients: [PEIG],
      joins: [],
    });
  });

  it("sends to the partner with mentioned people joining", () => {
    expect(resolve("A15", answer("no_one", 0.9, { p1: 0.9, p2: 0.1 }))).toEqual({
      kind: "send",
      recipients: [ROISIN],
      joins: [MICHEAL],
    });
  });

  it("maps references, absent people and unknown names", () => {
    expect(resolve("A9", answer("r1"))).toEqual({ kind: "which", among: [MICHEAL, ROISIN] });
    expect(resolve("A21", answer("e1"))).toEqual({ kind: "not_here", person: PEIG });
    expect(resolve("A12", answer("not_listed"))).toEqual({ kind: "neutral" });
  });

  it("makes no call with nobody present and nobody met", () => {
    expect(resolve("A28", answer("p1"))).toEqual({ kind: "idle" });
  });
});

describe("scoring", () => {
  it("counts a send outside the label as a confident wrong route", () => {
    expect(
      score({ kind: "ask" }, { kind: "send", recipients: [MICHEAL], joins: [] }),
    ).toMatchObject({ correct: false, confidentWrongRoute: true });
    expect(
      score(
        { kind: "send", recipients: [ROISIN], joins: [MICHEAL] },
        { kind: "send", recipients: [ROISIN], joins: [] },
      ),
    ).toMatchObject({ correct: false, confidentWrongRoute: false });
  });

  it("counts a question the label does not expect as unnecessary", () => {
    expect(score({ kind: "send", recipients: [ROISIN] }, { kind: "ask", mentioned: [] })).toEqual({
      correct: false,
      confidentWrongRoute: false,
      unnecessaryQuestion: true,
    });
  });

  it("reads today's engine from the harness observation", () => {
    const names = new Map([
      ["Mícheál Connolly", MICHEAL],
      ["Róisín Connolly", ROISIN],
    ]);
    const observation = {
      id: "A25",
      run: 0,
      recipients: [],
      asked: false,
      choices: [],
      narration: ["Mícheál is not here."],
      intentCalls: 1,
      presentAfter: [],
      unexpectedRole: null,
    };
    expect(baselineOutcome(byId("A25"), observation, names)).toEqual({
      kind: "not_here",
      person: MICHEAL,
    });
    expect(
      baselineOutcome(byId("A9"), { ...observation, asked: true, choices: ["2", "3"] }, names),
    ).toEqual({ kind: "which", among: [MICHEAL, ROISIN] });
  });
});

describe("proposed uncertain rule", () => {
  const people = loadPeople(npcs);
  const a25 = byId("A25");
  const uncertainElsewhere = answer("e1", 0.76);

  it("sends an uncertain answer about someone elsewhere to the one person present", () => {
    expect(
      resolveDecision(a25, buildInput(a25, people), uncertainElsewhere, startingThresholds),
    ).toEqual({ kind: "send", recipients: [PEIG], joins: [] });
  });

  it("gives that option's reply instead under the proposal", () => {
    expect(
      resolveDecision(a25, buildInput(a25, people), uncertainElsewhere, {
        ...startingThresholds,
        proposedUncertainRule: true,
      }),
    ).toEqual({ kind: "not_here", person: MICHEAL });
  });
});

describe("proposed uncertain rule with nobody present", () => {
  it("says the person met is not here rather than the idle message", () => {
    const a33 = byId("A33");
    expect(
      resolveDecision(a33, buildInput(a33, loadPeople(npcs)), answer("e1", 0.67), {
        ...startingThresholds,
        proposedUncertainRule: true,
      }),
    ).toEqual({ kind: "not_here", person: MICHEAL });
  });
});

describe("intent resolution (§4.6)", () => {
  const people = loadPeople(npcs);
  const places = loadPlaces(worldPath);
  const choice = (id: string, p = 0.95) => ({
    choice: id,
    confidence: p,
    probabilities: { [id]: p },
  });
  const decide = (
    id: string,
    parts: Partial<Record<"action" | "destination" | "target" | "recipient", [string, number?]>>,
    thresholds = startingThresholds,
  ) => {
    const item = byId(id);
    const input = buildInput(item, people, places);
    const pick = (key: string) => {
      const value = parts[key as keyof typeof parts];
      return value === undefined ? undefined : choice(value[0], value[1]);
    };
    const phraseId = (text: string) =>
      [...input.phrases.entries()].find(([, phrase]) => phrase === text)?.[0] ?? text;
    const target = parts.target === undefined ? undefined : choice(phraseId(parts.target[0]));
    return resolveInput(
      item,
      input,
      {
        action: pick("action")!,
        recipient: pick("recipient") ?? choice("no_one"),
        mentioned: {},
        ...(pick("destination") === undefined ? {} : { destination: pick("destination")! }),
        ...(target === undefined ? {} : { target }),
      },
      thresholds,
    );
  };

  it("travels to the chosen known place", () => {
    expect(decide("B4", { action: ["move"], destination: ["l2"] })).toEqual({
      kind: "move",
      place: OFFICE,
    });
  });

  it("echoes the selected phrase for a place it does not know", () => {
    expect(
      decide("B6", { action: ["move"], destination: ["not_listed"], target: ["the fairy fort"] }),
    ).toEqual({ kind: "move_unknown", phrase: "the fairy fort" });
  });

  it("asks where to when a move names no usable place", () => {
    expect(decide("B6", { action: ["move"], destination: ["none"] })).toEqual({
      kind: "move_where",
    });
  });

  it("treats an uncertain move as speech, so nobody travels by mistake", () => {
    expect(decide("B7", { action: ["move", 0.5], destination: ["l3"], recipient: ["p1"] })).toEqual(
      { kind: "send", recipients: [PEIG], joins: [] },
    );
  });

  it("takes the top narration-only action even when unsure", () => {
    expect(decide("B11", { action: ["look", 0.5] })).toEqual({ kind: "look" });
    expect(decide("B13", { action: ["examine", 0.5], target: ["the pigeonholes"] })).toEqual({
      kind: "examine",
      phrase: "the pigeonholes",
    });
    expect(decide("B14", { action: ["interact", 0.4] })).toEqual({
      kind: "interact",
      phrase: null,
    });
  });

  it("drops a line as none only when sure", () => {
    expect(decide("B16", { action: ["none", 0.9] })).toEqual({ kind: "none" });
    expect(decide("A20", { action: ["none", 0.5], recipient: ["no_one", 0.9] })).toEqual({
      kind: "ask",
      mentioned: [],
    });
  });

  it("scores an unwanted trip as a confident wrong route", () => {
    expect(score({ kind: "send", recipients: [PEIG] }, { kind: "move", place: OFFICE })).toEqual({
      correct: false,
      confidentWrongRoute: true,
      unnecessaryQuestion: false,
    });
    expect(
      score({ kind: "interact", phrase: "the gate" }, { kind: "interact", phrase: null }),
    ).toMatchObject({ correct: true });
  });
});

describe("today's engine on intent lines", () => {
  const places = loadPlaces(worldPath);
  const names = new Map([["Peig Hannigan", PEIG]]);
  const observe = (id: string, extra: Partial<BaselineObservation>): BaselineObservation => ({
    id,
    run: 0,
    recipients: [],
    asked: false,
    choices: [],
    narration: [],
    intentCalls: 1,
    presentAfter: [],
    unexpectedRole: null,
    ...extra,
  });

  it("reads travel from the scene after the line", () => {
    expect(
      baselineOutcome(byId("B1"), observe("B1", { sceneAfter: "Letter Office" }), names, places),
    ).toEqual({ kind: "move", place: OFFICE });
  });

  it("reads an unknown place, a look and an examine from the narration", () => {
    expect(
      baselineOutcome(
        byId("B6"),
        observe("B6", {
          sceneAfter: "Kilteevan Village",
          narration: ['You haven\'t the faintest notion how to reach "the fairy fort". You can'],
        }),
        names,
        places,
      ),
    ).toEqual({ kind: "move_unknown", phrase: "the fairy fort" });
    const village = places.find((place) => place.id === 1)!;
    expect(
      baselineOutcome(
        byId("B10"),
        observe("B10", { sceneAfter: village.name, narration: [village.description] }),
        names,
        places,
      ),
    ).toEqual({ kind: "look" });
    expect(
      baselineOutcome(
        byId("B12"),
        observe("B12", {
          sceneAfter: village.name,
          narration: [
            "You look more closely at Peig's satchel. There is nothing more noteworthy about it.",
          ],
        }),
        names,
        places,
      ),
    ).toEqual({ kind: "examine", phrase: "Peig's satchel" });
  });

  it("reads a narrated action with no dialogue call as interact", () => {
    expect(
      baselineOutcome(
        byId("B16"),
        observe("B16", {
          sceneAfter: "Kilteevan Village",
          events: [{ kind: "npc_dialogue", content: "You asdf qwerty." }],
        }),
        names,
        places,
      ),
    ).toEqual({ kind: "interact", phrase: null });
  });

  it("settles locally only commands the engine made no call for", () => {
    const local = localOutcomes(
      [
        observe("B1", { intentCalls: 0 }),
        observe("B7", { intentCalls: 0 }),
        observe("B4", { intentCalls: 1 }),
      ],
      (observation) =>
        observation.id === "B7"
          ? { kind: "send", recipients: [PEIG], joins: [] }
          : { kind: "move", place: OFFICE },
    );
    expect([...local.keys()]).toEqual(["B1#0"]);
  });
});
