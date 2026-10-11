import type { Expected, LabelledCase } from "./cases.js";
import { fold, type BuiltInput, type Place } from "./world.js";

/** What the game does with a line. */
export type Outcome =
  | { kind: "send"; recipients: number[]; joins: number[] }
  | { kind: "ask"; mentioned: number[] }
  | { kind: "which"; among: number[] }
  | { kind: "neutral" }
  | { kind: "not_here"; person: number }
  | { kind: "idle" }
  | { kind: "move"; place: number }
  | { kind: "move_unknown"; phrase: string | null }
  /** A move with no destination the engine can use: "Where to?". */
  | { kind: "move_where" }
  | { kind: "look" }
  | { kind: "examine"; phrase: string | null }
  | { kind: "interact"; phrase: string | null }
  | { kind: "none" }
  | { kind: "failed" };

interface ChoiceAnswer {
  choice: string;
  confidence: number;
  probabilities: Record<string, number>;
}

export interface DecisionOutput {
  recipient: ChoiceAnswer;
  mentioned: Record<string, number>;
  /** Present in `rundale-input` answers (§4.6). */
  action?: ChoiceAnswer;
  destination?: ChoiceAnswer;
  target?: ChoiceAnswer;
  atmosphere?: ChoiceAnswer;
}

export interface Thresholds {
  /** Minimum probability of the chosen option in question 1. */
  choice: number;
  /** Minimum yes probability for question 2. */
  mention: number;
  /** Minimum probability of the chosen action, destination and target (§4.6); default 0.7. */
  action?: number;
  /**
   * The results document's proposal, not the approved design: an uncertain
   * answer whose most likely option is not a person present (someone
   * elsewhere, a name not listed, or a reference) gives that option's reply
   * and sends nothing, instead of reaching the one person present.
   */
  proposedUncertainRule?: boolean;
}

export const startingThresholds: Thresholds = { choice: 0.8, mention: 0.7, action: 0.7 };

const top = (answer: ChoiceAnswer | undefined) =>
  answer === undefined
    ? { choice: undefined, p: 0 }
    : { choice: answer.choice, p: answer.probabilities[answer.choice] ?? 0 };

/**
 * One `rundale-input` answer (§4.6): the action, then the answer that action
 * uses. Look, examine and interact only narrate and change nothing, so the top
 * action is taken. Travel changes the world and "none" drops the line, so they
 * must clear the threshold; below it the line is treated as speech, the
 * game's default, where recipient resolution still asks when it is unsure.
 */
export function resolveInput(
  item: LabelledCase,
  input: BuiltInput,
  output: DecisionOutput | undefined,
  thresholds: Thresholds,
): Outcome {
  if (output?.action === undefined) return resolveDecision(item, input, output, thresholds);
  const floor = thresholds.action ?? 0.7;
  const action = top(output.action);
  const phrase = () => {
    const target = top(output.target);
    return target.p >= floor && target.choice !== undefined
      ? (input.phrases.get(target.choice) ?? null)
      : null;
  };
  // "none" drops the line, which loses speech meant for someone, so like a
  // move it must clear the threshold.
  const narrationOnly = ["look", "examine", "interact"].includes(action.choice ?? "");
  if (action.p >= floor || narrationOnly) {
    switch (action.choice) {
      case "move": {
        const destination = top(output.destination);
        if (destination.p < floor || destination.choice === "none") return { kind: "move_where" };
        if (destination.choice === "not_listed") return { kind: "move_unknown", phrase: phrase() };
        const place = input.places.get(destination.choice ?? "");
        return place === undefined ? { kind: "failed" } : { kind: "move", place };
      }
      case "look":
        return { kind: "look" };
      case "examine":
        return { kind: "examine", phrase: phrase() };
      case "interact":
        return { kind: "interact", phrase: phrase() };
      case "none":
        return { kind: "none" };
      default:
        break;
    }
  }
  return resolveDecision(item, input, output, thresholds);
}

/** Whether System 1 runs for the case at all (design §3.1, "Nobody present"). */
export function decisionRuns(item: LabelledCase): boolean {
  return item.present.length > 0 || item.introduced.length > 0;
}

function unaddressed(item: LabelledCase, mentioned: number[], uncertain: boolean): Outcome {
  if (!uncertain && item.partner !== undefined && item.present.includes(item.partner)) {
    return {
      kind: "send",
      recipients: [item.partner],
      joins: mentioned.filter((id) => id !== item.partner),
    };
  }
  if (item.present.length === 1) return { kind: "send", recipients: [item.present[0]!], joins: [] };
  if (item.present.length === 0) return { kind: "idle" };
  return { kind: "ask", mentioned };
}

/**
 * The resolution table of design §3.1 step 4 and the unaddressed rules of
 * step 5, applied to one validated System 1 answer. `undefined` is a failed
 * call other than lost connectivity, which counts as uncertain (§4.4).
 */
export function resolveDecision(
  item: LabelledCase,
  input: BuiltInput,
  output: DecisionOutput | undefined,
  thresholds: Thresholds,
): Outcome {
  if (!decisionRuns(item)) return { kind: "idle" };
  if (output === undefined) return unaddressed(item, [], true);
  const presentIds = input.values.present.map((option) => option.optionId);
  const mentioned = presentIds
    .filter((optionId) => (output.mentioned[optionId] ?? 0) >= thresholds.mention)
    .map((optionId) => input.people.get(optionId)!);
  const choice = output.recipient.choice;
  const probability = output.recipient.probabilities[choice] ?? 0;
  if (probability < thresholds.choice) {
    const presentChoice = input.values.present.some((option) => option.optionId === choice);
    if (
      thresholds.proposedUncertainRule === true &&
      !presentChoice &&
      choice !== "all" &&
      choice !== "no_one"
    ) {
      return resolveDecision(
        item,
        input,
        { ...output, recipient: { ...output.recipient, probabilities: { [choice]: 1 } } },
        { ...thresholds, proposedUncertainRule: false },
      );
    }
    if (item.present.length === 0) return { kind: "idle" };
    // Question 2 has its own threshold, so a confident mention still joins
    // after the player's choice (§3.1 step 5).
    return unaddressed(item, mentioned, true);
  }
  if (choice === "all") {
    return item.present.length === 0
      ? { kind: "idle" }
      : { kind: "send", recipients: [...item.present], joins: [] };
  }
  if (choice === "not_listed") return { kind: "neutral" };
  if (choice === "no_one") return unaddressed(item, mentioned, false);
  const reference = input.references.get(choice);
  if (reference !== undefined) return { kind: "which", among: reference };
  const person = input.people.get(choice);
  if (person === undefined) return { kind: "failed" };
  if (choice.startsWith("p")) return { kind: "send", recipients: [person], joins: [] };
  return { kind: "not_here", person };
}

export interface BaselineObservation {
  id: string;
  run: number;
  recipients: string[];
  asked: boolean;
  choices: string[];
  narration: string[];
  intentCalls: number;
  presentAfter: string[];
  unexpectedRole: string | null;
  sceneAfter?: string;
  events?: { kind: string; content: unknown }[];
}

/**
 * What today's engine did, read from the FFI harness: dialogue calls are
 * recipients; a clarification is "which" when it does not offer All; a
 * "{name} is not here." line is a not-here reply; nothing sent and nothing
 * said about absence is neutral or idle, whichever the case expects, since
 * today's engine has no neutral reply of its own.
 */
export function baselineOutcome(
  item: LabelledCase,
  observation: BaselineObservation,
  nameToId: Map<string, number>,
  places: Place[] = [],
): Outcome {
  if (observation.unexpectedRole !== null) return { kind: "failed" };
  const start = places.find((place) => place.id === item.place)?.name;
  const end = places.find((place) => place.name === observation.sceneAfter);
  if (start !== undefined && end !== undefined && end.name !== start) {
    return { kind: "move", place: end.id };
  }
  const examined = observation.narration
    .map((line) => /^You look more closely at (.+)\. There is nothing/.exec(line)?.[1])
    .find((phrase) => phrase !== undefined);
  if (examined !== undefined) return { kind: "examine", phrase: examined };
  const unknownPlace = observation.narration
    .map((line) => /how to reach "(.+?)"/.exec(line)?.[1])
    .find((phrase) => phrase !== undefined);
  if (unknownPlace !== undefined) return { kind: "move_unknown", phrase: unknownPlace };
  const here = places.find((place) => place.id === item.place);
  if (
    here !== undefined &&
    here.description.length > 0 &&
    observation.narration.some((line) => line.startsWith(here.description.slice(0, 40)))
  ) {
    return { kind: "look" };
  }
  // Today's engine narrates a physical action as "You …" with no dialogue call.
  const narratedAction = (observation.events ?? []).some(
    (event) =>
      event.kind === "npc_dialogue" &&
      typeof event.content === "string" &&
      event.content.startsWith("You "),
  );
  if (narratedAction && observation.recipients.length === 0 && !observation.asked) {
    return { kind: "interact", phrase: null };
  }
  if (observation.asked) {
    const among = observation.choices
      .map((id) => Number(id))
      .filter((id) => Number.isInteger(id) && id > 0);
    return among.length === observation.choices.length
      ? { kind: "which", among }
      : { kind: "ask", mentioned: [] };
  }
  if (observation.recipients.length > 0) {
    const ids = observation.recipients.map((name) => nameToId.get(name) ?? -1);
    return { kind: "send", recipients: ids, joins: [] };
  }
  const absent = observation.narration
    .map((line) => /^(.+) is not here\.$/.exec(line)?.[1])
    .find((name) => name !== undefined);
  if (absent !== undefined) {
    // Today's engine says the first name, or the full name for some paths.
    const id = [...nameToId.entries()].find(
      ([name]) => name === absent || name.split(" ")[0] === absent,
    )?.[1];
    return { kind: "not_here", person: id ?? -1 };
  }
  return item.expected.kind === "idle" ? { kind: "idle" } : { kind: "neutral" };
}

export interface Score {
  correct: boolean;
  /**
   * A line sent to someone the label says should not get it without asking,
   * or travel the label does not ask for.
   */
  confidentWrongRoute: boolean;
  /** A question where the label expects none. */
  unnecessaryQuestion: boolean;
}

const sameList = (left: number[], right: number[]) =>
  left.length === right.length && left.every((value, index) => value === right[index]);
const sameSet = (left: number[], right: number[]) => sameList([...left].sort(), [...right].sort());

/**
 * Scores an outcome against the label. Recipients must match in order, with
 * joins after them; "Ask" with a mentioned person needs that person noted.
 * Sending to anyone outside the label's recipients and joins, or sending
 * where the label sends nothing, is a confident wrong route.
 */
export function score(expected: Expected, outcome: Outcome): Score {
  const asks = outcome.kind === "ask" || outcome.kind === "which";
  const expectsQuestion = expected.kind === "ask" || expected.kind === "which";
  let confidentWrongRoute = false;
  if (outcome.kind === "send") {
    const allowed =
      expected.kind === "send" ? [...expected.recipients, ...(expected.joins ?? [])] : [];
    confidentWrongRoute = [...outcome.recipients, ...outcome.joins].some(
      (id) => !allowed.includes(id),
    );
  }
  let correct = false;
  switch (expected.kind) {
    case "send":
      correct =
        outcome.kind === "send" &&
        sameList(outcome.recipients, expected.recipients) &&
        sameList(outcome.joins, expected.joins ?? []);
      break;
    case "ask":
      correct = outcome.kind === "ask" && sameSet(outcome.mentioned, expected.mentioned ?? []);
      break;
    case "which":
      correct = outcome.kind === "which" && sameSet(outcome.among, expected.among);
      break;
    case "not_here":
      correct = outcome.kind === "not_here" && outcome.person === expected.person;
      break;
    case "move":
      correct = outcome.kind === "move" && outcome.place === expected.place;
      break;
    case "interact":
      // The engine echoes the player's own line for an action, so only the
      // action is scored.
      correct = outcome.kind === "interact";
      break;
    case "move_unknown":
    case "examine":
      correct =
        outcome.kind === expected.kind &&
        outcome.phrase !== null &&
        fold(outcome.phrase) === fold(expected.phrase);
      break;
    default:
      correct = outcome.kind === expected.kind;
  }
  // Travel the label does not ask for moves the player, which they cannot undo
  // with a question.
  const wrongTravel =
    outcome.kind === "move" && !(expected.kind === "move" && expected.place === outcome.place);
  return {
    correct,
    confidentWrongRoute: confidentWrongRoute || wrongTravel,
    unnecessaryQuestion: asks && !expectsQuestion,
  };
}

/**
 * Outcomes the local parser settles with no model call, keyed `id#run`: the
 * baseline's outcome wherever today's engine made no intent call and acted on
 * a command (§4.6: such lines never reach System 1).
 */
export type LocalOutcomes = Map<string, Outcome>;
const talkKinds = new Set(["send", "ask", "which", "neutral", "not_here", "idle"]);

export function localOutcomes(
  observations: BaselineObservation[],
  outcome: (observation: BaselineObservation) => Outcome,
): LocalOutcomes {
  const local: LocalOutcomes = new Map();
  for (const observation of observations) {
    const result = outcome(observation);
    if (observation.intentCalls === 0 && !talkKinds.has(result.kind)) {
      local.set(`${observation.id}#${observation.run}`, result);
    }
  }
  return local;
}
