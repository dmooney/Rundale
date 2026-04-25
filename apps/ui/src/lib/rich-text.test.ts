import { describe, it, expect } from "vitest";
import { segmentText, type RichSegment } from "./rich-text";

describe("segmentText", () => {
  it("returns plain segment for empty hints", () => {
    expect(segmentText("Hello world", [], [], "")).toEqual([
      { text: "Hello world", kind: "plain" },
    ]);
  });

  it("returns empty array for empty content", () => {
    expect(segmentText("", ["féile"], [], "")).toEqual([]);
  });

  it("highlights Irish words", () => {
    const result = segmentText("He said fáilte warmly", ["fáilte"], [], "");
    expect(result).toEqual([
      { text: "He said ", kind: "plain" },
      { text: "fáilte", kind: "irish" },
      { text: " warmly", kind: "plain" },
    ]);
  });

  it("highlights NPC names", () => {
    const result = segmentText("Seán walked home", [], ["Seán"], "");
    expect(result).toEqual([
      { text: "Seán", kind: "name" },
      { text: " walked home", kind: "plain" },
    ]);
  });

  it("highlights location name", () => {
    const result = segmentText("Welcome to Kilteevan", [], [], "Kilteevan");
    expect(result).toEqual([
      { text: "Welcome to ", kind: "plain" },
      { text: "Kilteevan", kind: "location" },
    ]);
  });

  it("irish priority beats name on overlap", () => {
    const result = segmentText(
      "The word féile is nice",
      ["féile"],
      ["féile"],
      "",
    );
    expect(result).toEqual([
      { text: "The word ", kind: "plain" },
      { text: "féile", kind: "irish" },
      { text: " is nice", kind: "plain" },
    ]);
  });

  it("handles multiple words in a single category", () => {
    const result = segmentText(
      "A cáca and bainne please",
      ["cáca", "bainne"],
      [],
      "",
    );
    expect(result).toEqual([
      { text: "A ", kind: "plain" },
      { text: "cáca", kind: "irish" },
      { text: " and ", kind: "plain" },
      { text: "bainne", kind: "irish" },
      { text: " please", kind: "plain" },
    ]);
  });

  it("is case-insensitive", () => {
    const result = segmentText("FÁILTE to you", ["fáilte"], [], "");
    expect(result).toEqual([
      { text: "FÁILTE", kind: "irish" },
      { text: " to you", kind: "plain" },
    ]);
  });

  it("does not match partial words", () => {
    const result = segmentText("unfailing effort", ["fail"], [], "");
    expect(result).toEqual([{ text: "unfailing effort", kind: "plain" }]);
  });

  it("handles regex-special characters in words", () => {
    const result = segmentText("Price is $5.00 today", ["$5.00"], [], "");
    expect(result).toEqual([
      { text: "Price is ", kind: "plain" },
      { text: "$5.00", kind: "irish" },
      { text: " today", kind: "plain" },
    ]);
  });

  it("filters empty strings from word lists", () => {
    const result = segmentText("A féile day", ["", "féile", ""], [], "");
    expect(result).toEqual([
      { text: "A ", kind: "plain" },
      { text: "féile", kind: "irish" },
      { text: " day", kind: "plain" },
    ]);
  });

  it("higher-priority match wins even when a lower-priority match starts earlier", () => {
    // "An Cailín" is registered as a name (lower priority) starting at col 4;
    // "Cailín" is registered as an irish word (higher priority) starting at col 7.
    // Both matches overlap.  Per the docstring, priority resolves overlaps:
    // the irish "Cailín" should win even though the name "An Cailín" starts first.
    const result = segmentText(
      "Say An Cailín please",
      ["Cailín"],
      ["An Cailín"],
      "",
    );
    expect(result.some((s) => s.kind === "irish" && s.text === "Cailín")).toBe(
      true,
    );
    expect(result.every((s) => s.kind !== "name")).toBe(true);
  });

  it("longest match wins when words overlap", () => {
    const result = segmentText(
      "The An Cailín spoke",
      ["An Cailín", "An"],
      [],
      "",
    );
    const kinds = result.map((s) => `${s.kind}:${s.text}`);
    expect(kinds).toContain("irish:An Cailín");
  });
});
