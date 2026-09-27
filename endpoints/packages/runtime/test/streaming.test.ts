import { describe, expect, it } from "vitest";
import { DeterministicRuntime, JsonStringProjection, RuntimeError } from "../src/index.js";

describe("streaming JSON projection", () => {
  it("reports streaming capability without starting provider work", () => {
    const provider = (supportsStreaming: boolean) => ({
      id: "google" as const,
      supportsStreaming,
      execute: async () => ({ output: {}, usage: {} }),
      stream: async function* () {},
    });
    const costs = { estimate: () => "0" };
    const runtime = new DeterministicRuntime(
      { get: (id) => (id === "openai" ? { ...provider(false), id } : provider(true)) },
      costs,
    );
    const version = {
      inferenceConfig: {
        maxOutputTokens: 10,
        retryCount: 0 as const,
        streaming: { version: 1 as const, textField: "result" },
      },
      providerConfig: { provider: "google" as const, model: "m" },
    } as never;
    expect(runtime.supportsStreaming(version)).toBe(true);
    expect(
      runtime.supportsStreaming({ ...version, providerConfig: { provider: "openai", model: "m" } }),
    ).toBe(false);
    expect(
      runtime.supportsStreaming({
        ...version,
        inferenceConfig: { ...version.inferenceConfig, streaming: undefined },
      }),
    ).toBe(false);
  });

  it("decodes fragmented escaped text and ignores unrelated fields", () => {
    const projection = new JsonStringProjection("result");
    const output = [
      ...projection.push('{"other":"secret","result":"hel'),
      ...projection.push("lo\\nwo"),
      ...projection.push('rld\\"!"}'),
    ].join("");
    projection.finish();
    expect(output).toBe('hello\nworld"!');
  });

  it("rejects truncated and oversized streams", () => {
    const truncated = new JsonStringProjection("result");
    truncated.push('{"result":"unfinished');
    expect(() => truncated.finish()).toThrow(RuntimeError);
    const bounded = new JsonStringProjection("result", 8);
    expect(() => bounded.push('{"result":"too long"}')).toThrow(/too large/);
  });

  it("decodes a fragmented surrogate pair", () => {
    const projection = new JsonStringProjection("result");
    const output = [
      ...projection.push('{"nested":{"result":"hidden"},"result":"\\uD83'),
      ...projection.push('D\\uDE03"}'),
    ].join("");
    projection.finish();
    expect(output).toBe("😃");
  });

  it("rejects duplicate fields and wrong target types", () => {
    const duplicate = new JsonStringProjection("result");
    expect(() => duplicate.push('{"result":"a","result":"b"}')).toThrow(/Duplicate/);
    const wrongType = new JsonStringProjection("result");
    expect(() => wrongType.push('{"result":{"nested":true}}')).toThrow(/not a string/);
  });

  it("bounds output by UTF-8 bytes", () => {
    const projection = new JsonStringProjection("result", 256, 3);
    expect(() => projection.push('{"result":"😃"}')).toThrow(/too large/);
  });

  it("coalesces tiny provider fragments and validates the final structured output", async () => {
    const raw = '{"private":"hidden","result":"The road is quiet after rain."}';
    const provider = {
      id: "fake" as const,
      supportsStreaming: true,
      execute: async () => ({ output: {}, usage: {} }),
      stream: async function* () {
        for (const text of raw) yield { type: "delta" as const, text };
        yield {
          type: "completed" as const,
          result: {
            output: { result: "The road is quiet after rain." },
            usage: { totalTokens: 7 },
          },
        };
      },
    };
    const runtime = new DeterministicRuntime(
      { get: () => provider },
      { estimate: () => "0.000000" },
    );
    const events = [];
    for await (const event of runtime.stream(
      {
        inputSchema: { type: "object", additionalProperties: false },
        outputSchema: {
          type: "object",
          properties: { result: { type: "string" } },
          required: ["result"],
          additionalProperties: false,
        },
        instructions: "fixture",
        providerConfig: { provider: "fake", model: "fake-v1" },
        inferenceConfig: {
          maxOutputTokens: 64,
          retryCount: 0,
          streaming: { version: 1, textField: "result" },
        },
      } as never,
      { values: {}, attachments: [] },
      { requestId: "request", deadline: new Date(), signal: new AbortController().signal },
    ))
      events.push(event);
    const deltas = events.filter((event) => event.type === "delta");
    expect(deltas.map((event) => event.content).join("")).toBe("The road is quiet after rain.");
    expect(deltas.length).toBeLessThan(5);
    expect(JSON.stringify(events)).not.toContain("hidden");
    expect(events.at(-1)).toMatchObject({ type: "completed" });
  });

  it("records a failed attempt when the completed output is invalid", async () => {
    const runtime = new DeterministicRuntime(
      {
        get: () => ({
          id: "fake" as const,
          supportsStreaming: true,
          execute: async () => ({ output: {}, usage: {} }),
          stream: async function* () {
            yield { type: "delta" as const, text: '{"result":"provisional"}' };
            yield {
              type: "completed" as const,
              result: { output: { wrong: true }, usage: { totalTokens: 3 } },
            };
          },
        }),
      },
      { estimate: () => "0.000000" },
    );
    let failure: RuntimeError | undefined;
    try {
      for await (const event of runtime.stream(
        {
          inputSchema: { type: "object" },
          outputSchema: {
            type: "object",
            properties: { result: { type: "string" } },
            required: ["result"],
            additionalProperties: false,
          },
          instructions: "fixture",
          providerConfig: { provider: "fake", model: "fake-v1" },
          inferenceConfig: {
            maxOutputTokens: 64,
            retryCount: 0,
            streaming: { version: 1, textField: "result" },
          },
        } as never,
        { values: {}, attachments: [] },
        { requestId: "request", deadline: new Date(), signal: new AbortController().signal },
      )) {
        void event;
      }
    } catch (error) {
      failure = error as RuntimeError;
    }
    expect(failure).toMatchObject({
      code: "OUTPUT_VALIDATION_FAILED",
      attempts: [{ status: "failed", errorCode: "OUTPUT_VALIDATION_FAILED" }],
    });
  });
});
