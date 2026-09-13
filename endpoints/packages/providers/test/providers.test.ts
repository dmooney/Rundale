import { describe, expect, it } from "vitest";
import { FakeProvider, FixedPriceCostCalculator, StaticProviderRegistry } from "../src/index.js";

describe("provider boundary", () => {
  it("normalizes deterministic fake output and usage", async () => {
    const provider = new FakeProvider({
      output: { value: 2 },
      usage: { inputTokens: 100, outputTokens: 50 },
    });
    const result = await provider.execute(
      {
        model: "fake-v1",
        instructions: "",
        values: {},
        attachments: [],
        outputSchema: {},
        parameters: { maxOutputTokens: 10 },
      },
      { requestId: "req", deadline: new Date(), signal: new AbortController().signal },
    );
    expect(result).toMatchObject({ output: { value: 2 }, usage: { inputTokens: 100 } });
    expect(new StaticProviderRegistry([provider]).get("fake")).toBe(provider);
  });

  it("calculates costs in one isolated component", () => {
    const calculator = new FixedPriceCostCalculator({
      "fake/fake-v1": { inputPerMillionUsd: 1, outputPerMillionUsd: 2 },
    });
    expect(
      calculator.estimate("fake", "fake-v1", {
        inputTokens: 1_000_000,
        outputTokens: 500_000,
      }),
    ).toBe("2.000000");
  });

  it("fails closed when a live model has no configured price", () => {
    const calculator = new FixedPriceCostCalculator({});
    expect(calculator.isConfigured("openai", "unpriced-model")).toBe(false);
    expect(() => calculator.estimate("openai", "unpriced-model", {})).toThrow(
      "The configured model is unavailable.",
    );
  });
});
