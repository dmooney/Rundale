import {
  RuntimeError,
  type DecisionAnswer,
  type DecisionInvocation,
  type DecisionResult,
  type ModelProvider,
  type ProviderExecutionContext,
  type ProviderInvocation,
  type ProviderResult,
} from "@limerick/runtime";

// One adapter for the Decisions request shape: OpenRouter's alpha Decisions
// API accepts TypeSafe's System One schema verbatim, so the same body serves
// both routes (ADR 014). Checked against vendor documentation on 2026-10-10:
//   https://openrouter.ai/docs/api/api-reference/alphadecisions/submit-a-decisions-questions-and-answers-request
//   https://docs.typesafe.ai/api

export type DecisionsProviderId = "openrouter" | "typesafe";

export const decisionsEndpoints: Readonly<Record<DecisionsProviderId, string>> = {
  openrouter: "https://openrouter.ai/api/alpha/decisions",
  typesafe: "https://api.typesafe.ai/v1/systemone",
};

type FetchLike = (input: string, init: RequestInit) => Promise<Response>;

export class DecisionsProvider implements ModelProvider {
  readonly supportsStreaming = false;

  constructor(
    readonly id: DecisionsProviderId,
    private readonly apiKey: string,
    private readonly fetchImpl: FetchLike = fetch,
    private readonly url: string = decisionsEndpoints[id],
  ) {}

  async decide(
    invocation: DecisionInvocation,
    context: ProviderExecutionContext,
  ): Promise<DecisionResult> {
    let response: Response;
    try {
      response = await this.fetchImpl(this.url, {
        method: "POST",
        headers: {
          authorization: `Bearer ${this.apiKey}`,
          "content-type": "application/json",
        },
        body: JSON.stringify({
          model: invocation.model,
          state: invocation.state,
          questions: invocation.questions,
        }),
        signal: context.signal,
      });
    } catch (error) {
      throw normalizeDecisionsError(error);
    }
    if (!response.ok) {
      await response.body?.cancel().catch(() => undefined);
      throw normalizeDecisionsStatus(response.status);
    }
    let body: unknown;
    try {
      body = await response.json();
    } catch (error) {
      if (error instanceof DOMException) throw normalizeDecisionsError(error);
      throw new RuntimeError("MODEL_ERROR", "The model provider returned an unreadable response.");
    }
    return parseDecisionsResponse(body);
  }

  execute(
    _invocation: ProviderInvocation,
    _context: ProviderExecutionContext,
  ): Promise<ProviderResult> {
    void _invocation;
    void _context;
    return Promise.reject(
      new RuntimeError("PROVIDER_UNAVAILABLE", "Decision models answer decision Endpoints only."),
    );
  }
}

function record(value: unknown): Record<string, unknown> | undefined {
  return value !== null && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : undefined;
}

function malformed(): RuntimeError {
  return new RuntimeError("MODEL_ERROR", "The model provider returned an unreadable response.");
}

/** Reads `{ answers, usage, id }`; the runtime checks answers against the offered options. */
export function parseDecisionsResponse(body: unknown): DecisionResult {
  const root = record(body);
  const answers = record(root?.answers);
  if (root === undefined || answers === undefined) throw malformed();
  const parsed: Record<string, DecisionAnswer> = {};
  for (const [key, value] of Object.entries(answers)) {
    const answer = record(value);
    if (answer?.type === "noul" && typeof answer.noul === "number") {
      parsed[key] = { type: "noul", noul: answer.noul };
    } else if (
      answer?.type === "choice" &&
      typeof answer.choice === "string" &&
      typeof answer.confidence === "number" &&
      record(answer.probabilities) !== undefined
    ) {
      const probabilities: Record<string, number> = {};
      for (const [option, p] of Object.entries(record(answer.probabilities)!)) {
        if (typeof p !== "number") throw malformed();
        probabilities[option] = p;
      }
      parsed[key] = {
        type: "choice",
        choice: answer.choice,
        confidence: answer.confidence,
        probabilities,
      };
    } else {
      throw malformed();
    }
  }
  const usage = record(root.usage);
  const inputTokens = usage?.input_tokens;
  const outputTokens = usage?.output_tokens;
  return {
    answers: parsed,
    usage: {
      ...(typeof inputTokens === "number" ? { inputTokens } : {}),
      ...(typeof outputTokens === "number" ? { outputTokens } : {}),
      ...(typeof inputTokens === "number" && typeof outputTokens === "number"
        ? { totalTokens: inputTokens + outputTokens }
        : {}),
    },
    ...(typeof root.id === "string" ? { providerRequestId: root.id } : {}),
  };
}

export function normalizeDecisionsStatus(status: number): RuntimeError {
  if (status === 429 || status === 529) {
    return new RuntimeError("PROVIDER_RATE_LIMITED", "The model provider is rate limited.", true);
  }
  if (status === 408 || status === 524) {
    return new RuntimeError("REQUEST_TIMEOUT", "The model provider timed out.", true);
  }
  if (status >= 500) {
    return new RuntimeError("PROVIDER_UNAVAILABLE", "The model provider is unavailable.", true);
  }
  if (status === 401 || status === 402 || status === 403) {
    return new RuntimeError("PROVIDER_UNAVAILABLE", "The model provider is unavailable.");
  }
  return new RuntimeError("MODEL_ERROR", "The model provider could not complete the request.");
}

export function normalizeDecisionsError(error: unknown): RuntimeError {
  if (error instanceof RuntimeError) return error;
  if (
    error instanceof DOMException &&
    (error.name === "AbortError" || error.name === "TimeoutError")
  ) {
    return new RuntimeError("REQUEST_TIMEOUT", "The model provider timed out.", true);
  }
  return new RuntimeError("PROVIDER_UNAVAILABLE", "The model provider is unavailable.", true);
}
