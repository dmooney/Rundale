import { describe, expect, it } from "vitest";
import { DecisionsProvider, decisionsEndpoints } from "../src/index.js";

const invocation = {
  model: "typesafe/jev-1.13",
  state: { line: "Hello" },
  questions: {
    recipient: {
      type: "choice" as const,
      instructions: "Who is `line` spoken to?",
      criteria: { p1: "First person.", p2: "Second person." },
    },
  },
};

const context = {
  requestId: "req_1",
  deadline: new Date(Date.now() + 1_000),
  signal: new AbortController().signal,
};

function respond(status: number, body: unknown) {
  const requests: { url: string; init: RequestInit }[] = [];
  const fetchImpl = (url: string, init: RequestInit) => {
    requests.push({ url, init });
    return Promise.resolve(
      new Response(JSON.stringify(body), {
        status,
        headers: { "content-type": "application/json" },
      }),
    );
  };
  return { fetchImpl, requests };
}

// The documented OpenRouter example response, trimmed to one question.
const documented = {
  id: "gen-dec-1789738314-X5e5eKGQdvR9rblyX250",
  model: "typesafe/jev-1.13-20260917",
  provider: "TypeSafe",
  answers: {
    recipient: {
      type: "choice",
      choice: "p2",
      confidence: 0.75,
      probabilities: { p1: 0.16, p2: 0.84 },
    },
  },
  usage: { input_tokens: 476, output_tokens: 70, cost: 0.000019992 },
};

describe("Decisions adapter", () => {
  it("posts model, state and questions with the key as a bearer token", async () => {
    const { fetchImpl, requests } = respond(200, documented);
    const provider = new DecisionsProvider("openrouter", "synthetic-key", fetchImpl);
    const result = await provider.decide(invocation, context);
    expect(requests).toHaveLength(1);
    expect(requests[0]!.url).toBe(decisionsEndpoints.openrouter);
    expect(new Headers(requests[0]!.init.headers).get("authorization")).toBe(
      "Bearer synthetic-key",
    );
    expect(JSON.parse(requests[0]!.init.body as string)).toEqual(invocation);
    expect(result).toEqual({
      answers: documented.answers,
      usage: { inputTokens: 476, outputTokens: 70, totalTokens: 546 },
      providerRequestId: documented.id,
    });
  });

  it("calls TypeSafe's own API for the direct route", async () => {
    const { fetchImpl, requests } = respond(200, documented);
    await new DecisionsProvider("typesafe", "synthetic-key", fetchImpl).decide(invocation, context);
    expect(requests[0]!.url).toBe("https://api.typesafe.ai/v1/systemone");
  });

  it.each([
    [401, "PROVIDER_UNAVAILABLE", false],
    [402, "PROVIDER_UNAVAILABLE", false],
    [429, "PROVIDER_RATE_LIMITED", true],
    [529, "PROVIDER_RATE_LIMITED", true],
    [524, "REQUEST_TIMEOUT", true],
    [502, "PROVIDER_UNAVAILABLE", true],
    [400, "MODEL_ERROR", false],
  ])("normalizes HTTP %i to %s", async (status, code, retryable) => {
    const { fetchImpl } = respond(status, { error: { message: "synthetic" } });
    await expect(
      new DecisionsProvider("openrouter", "k", fetchImpl).decide(invocation, context),
    ).rejects.toMatchObject({ code, retryable });
  });

  it("treats an aborted request as a timeout", async () => {
    const provider = new DecisionsProvider("openrouter", "k", () =>
      Promise.reject(new DOMException("Aborted", "AbortError")),
    );
    await expect(provider.decide(invocation, context)).rejects.toMatchObject({
      code: "REQUEST_TIMEOUT",
    });
  });

  it("rejects malformed answers", async () => {
    const { fetchImpl } = respond(200, {
      answers: { recipient: { type: "choice", choice: 3 } },
    });
    await expect(
      new DecisionsProvider("openrouter", "k", fetchImpl).decide(invocation, context),
    ).rejects.toMatchObject({ code: "MODEL_ERROR" });
  });

  it("serves decision Endpoints only", async () => {
    const { fetchImpl } = respond(200, documented);
    await expect(
      new DecisionsProvider("openrouter", "k", fetchImpl).execute(
        {
          model: "m",
          instructions: "i",
          values: {},
          attachments: [],
          outputSchema: {},
          parameters: { maxOutputTokens: 1 },
        },
        context,
      ),
    ).rejects.toMatchObject({ code: "PROVIDER_UNAVAILABLE" });
  });
});
