import { readFileSync } from "node:fs";
import type { Fixture, LabelledCase } from "./cases.js";
import { MARY, PEIG, ROISIN } from "./cases.js";

export interface Person {
  id: number;
  name: string;
  description: string;
  occupation: string;
  home?: number;
}

/** The canonical NPCs, changed as the Rust fixtures change them. */
export function loadPeople(npcsPath: string, fixture?: Fixture): Map<number, Person> {
  const raw = JSON.parse(readFileSync(npcsPath, "utf8")) as {
    npcs: {
      id: number;
      name: string;
      brief_description: string;
      occupation: string;
      home?: number;
    }[];
  };
  const people = new Map<number, Person>(
    raw.npcs.map((npc) => [
      npc.id,
      {
        id: npc.id,
        name: npc.name,
        description: npc.brief_description,
        occupation: npc.occupation,
        ...(npc.home === undefined ? {} : { home: Number(npc.home) }),
      },
    ]),
  );
  if (fixture === "two_drovers") {
    const roisin = people.get(ROISIN)!;
    people.set(ROISIN, { ...roisin, occupation: "Smallholder and cattle drover" });
  }
  if (fixture === "mary_elsewhere") {
    const peig = people.get(PEIG)!;
    people.set(MARY, {
      ...peig,
      id: MARY,
      name: "Mary Brennan",
      description: "a stout woman with a basket of eggs",
      occupation: "Egg-seller",
    });
  }
  return people;
}

/**
 * Case- and accent-folds text (design §3.5): removes accents from precomposed
 * letters and drops combining marks, so "Roisin" matches "Róisín".
 */
export function fold(text: string): string {
  return text.normalize("NFD").replace(/\p{M}/gu, "").toLowerCase();
}

const words = (text: string) => fold(text).match(/\p{L}+/gu) ?? [];
const minor = new Set(["and", "the", "of", "a"]);

export interface Place {
  id: number;
  name: string;
  aliases: string[];
  description: string;
}

/** The canonical places, with the first sentence of each description. */
export function loadPlaces(worldPath: string): Place[] {
  const raw = JSON.parse(readFileSync(worldPath, "utf8")) as {
    locations: { id: number; name: string; aliases?: string[]; description_template: string }[];
  };
  return raw.locations
    .map((location) => ({
      id: Number(location.id),
      name: location.name,
      aliases: location.aliases ?? [],
      description: (location.description_template.split(/(?<=\.)\s/)[0] ?? "")
        .replace(/\{[^}]*\}/g, "")
        .trim(),
    }))
    .sort((a, b) => a.id - b.id);
}

const determiners = new Set([
  "the",
  "a",
  "an",
  "my",
  "his",
  "her",
  "their",
  "your",
  "our",
  "this",
  "that",
]);
const prepositions = new Set([
  "on",
  "in",
  "at",
  "to",
  "from",
  "with",
  "by",
  "for",
  "of",
  "behind",
  "under",
  "over",
  "near",
  "into",
  "about",
  "before",
  "after",
  "across",
  "around",
  "beside",
]);

/**
 * Candidate phrases cut from the line (design §4.6): each run of words that
 * starts at a determiner, a possessive or a capitalised word and ends before
 * punctuation or a preposition, with every shortening from the end. The
 * phrases keep the player's own spelling so a chosen one can be echoed.
 */
export function candidatePhrases(line: string, limit = 24): string[] {
  const clauses = line.split(/[,.;:!?]/);
  const found: string[] = [];
  const add = (phrase: string) => {
    if (phrase.length > 0 && !found.includes(phrase) && found.length < limit) found.push(phrase);
  };
  for (const clause of clauses) {
    const tokens = clause.trim().split(/\s+/).filter(Boolean);
    for (let start = 0; start < tokens.length; start += 1) {
      const word = tokens[start]!;
      const lower = fold(word);
      // A capital opens a phrase only past the clause's first word, where it
      // marks a name rather than the start of a sentence.
      const opens =
        determiners.has(lower) ||
        /'s$|s'$/.test(lower) ||
        (start > 0 && /^\p{Lu}/u.test(word) && lower !== "i");
      if (!opens) continue;
      let end = start + 1;
      while (end < tokens.length && !prepositions.has(fold(tokens[end]!))) end += 1;
      for (let stop = end; stop > start; stop -= 1) {
        // A determiner alone names nothing.
        if (stop === start + 1 && determiners.has(lower)) continue;
        add(tokens.slice(start, stop).join(" "));
      }
    }
  }
  return found;
}

export interface AddresseeInput {
  role: "player_intent";
  line: string;
  present: {
    optionId: string;
    name: string;
    description: string;
    occupation: string;
    introduced: boolean;
  }[];
  references: { optionId: string; phrase: string; people: string[] }[];
  /** "all", offered only with two or more present. */
  everyone: { optionId: string }[];
  /** "someone_else", offered only with two or more present. */
  someoneElse: { optionId: string }[];
  /** "no_one", offered unless exactly one person is present. */
  noOne: { optionId: string }[];
  /** The people present again, with two or more present: mention subjects. */
  mentionable: { optionId: string; name: string; description: string }[];
  places: {
    optionId: string;
    name: string;
    aliases: string[];
    description: string;
    residents: string[];
  }[];
  phrases: { optionId: string; text: string }[];
}

export interface BuiltInput {
  values: AddresseeInput;
  /** Option id to person id, for the people present. */
  people: Map<string, number>;
  /** Reference option id to the person ids it fits. */
  references: Map<string, number[]>;
  /** Place option id to place id. */
  places: Map<string, number>;
  /** Phrase option id to the phrase. */
  phrases: Map<string, string>;
}

/**
 * The engine's input for one line (design §4.3), as
 * `limerick_core::intent_decision::intent_input` builds it: people present in
 * roster (id) order, one reference per word of the line that is a first name,
 * family name or occupation word of several people present, and the group
 * options and mention subjects only when they mean something. Nobody
 * elsewhere is offered.
 */
export function buildInput(
  item: LabelledCase,
  people: Map<number, Person>,
  knownPlaces: Place[] = [],
): BuiltInput {
  const ids = [...people.keys()].sort((a, b) => a - b);
  const present = ids.filter((id) => item.present.includes(id));
  const optionPeople = new Map<string, number>();
  const presentOptions = present.map((id, index) => {
    const person = people.get(id)!;
    const optionId = `p${index + 1}`;
    optionPeople.set(optionId, id);
    return {
      optionId,
      name: person.name,
      description: person.description,
      occupation: person.occupation,
      introduced: item.introduced.includes(id),
    };
  });
  const several = present.length >= 2;
  const group = (optionId: string) => (several ? [{ optionId }] : []);
  const references = new Map<string, number[]>();
  const referenceOptions: AddresseeInput["references"] = [];
  const seen = new Set<string>();
  const original = item.line.match(/\p{L}+/gu) ?? [];
  for (const [index, word] of words(item.line).entries()) {
    if (minor.has(word) || seen.has(word)) continue;
    const fits = present.filter((id) => {
      const person = people.get(id)!;
      const names = words(person.name);
      const roles = words(person.occupation).filter((part) => !minor.has(part));
      return names[0] === word || names.at(-1) === word || roles.includes(word);
    });
    if (fits.length < 2) continue;
    seen.add(word);
    const optionId = `r${referenceOptions.length + 1}`;
    references.set(optionId, fits);
    referenceOptions.push({
      optionId,
      phrase: original[index] ?? word,
      people: fits.map((id) => people.get(id)!.name),
    });
  }
  const places = new Map<string, number>();
  const placeOptions = knownPlaces.map((place, index) => {
    const optionId = `l${index + 1}`;
    places.set(optionId, place.id);
    return {
      optionId,
      name: place.name,
      aliases: place.aliases,
      description: place.description,
      residents: [...people.values()]
        .filter((person) => person.home === place.id)
        .map((person) => person.name),
    };
  });
  const phrases = new Map<string, string>();
  const phraseOptions = candidatePhrases(item.line).map((text, index) => {
    const optionId = `s${index + 1}`;
    phrases.set(optionId, text);
    return { optionId, text };
  });
  return {
    values: {
      role: "player_intent",
      line: item.line,
      present: presentOptions,
      references: referenceOptions,
      everyone: group("all"),
      someoneElse: group("someone_else"),
      noOne: present.length === 1 ? [] : [{ optionId: "no_one" }],
      mentionable: several
        ? presentOptions.map(({ optionId, name, description }) => ({
            optionId,
            name,
            description,
          }))
        : [],
      places: placeOptions,
      phrases: phraseOptions,
    },
    people: optionPeople,
    references,
    places,
    phrases,
  };
}
