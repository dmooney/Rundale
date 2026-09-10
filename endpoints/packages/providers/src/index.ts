import type { ProviderId } from "@parish/domain";
import {
  RuntimeError,
  type ModelProvider,
  type ProviderExecutionContext,
  type ProviderInvocation,
  type ProviderRegistry,
  type ProviderResult,
} from "@parish/runtime";

export interface FakeProviderBehavior {
  output?: unknown;
  outputs?: unknown[];
  delayMs?: number;
  error?: RuntimeError;
  usage?: ProviderResult["usage"];
}

export class FakeProvider implements ModelProvider {
  readonly id = "fake" as const;
  private callCount = 0;

  constructor(private readonly behavior: FakeProviderBehavior = {}) {}

  async execute(
    _invocation: ProviderInvocation,
    context: ProviderExecutionContext,
  ): Promise<ProviderResult> {
    this.callCount += 1;
    if (context.signal.aborted) throw new DOMException("Aborted", "AbortError");
    if (this.behavior.delayMs !== undefined) {
      await new Promise<void>((resolve, reject) => {
        const timeout = setTimeout(resolve, this.behavior.delayMs);
        context.signal.addEventListener("abort", () => {
          clearTimeout(timeout);
          reject(new DOMException("Aborted", "AbortError"));
        });
      });
    }
    if (this.behavior.error !== undefined) throw this.behavior.error;
    const output =
      this.behavior.outputs?.[this.callCount - 1] ??
      this.behavior.output ??
      ({ result: "fake-provider-result" } satisfies Record<string, unknown>);
    return {
      output: structuredClone(output),
      usage: this.behavior.usage ?? { inputTokens: 10, outputTokens: 5, totalTokens: 15 },
      providerRequestId: `fake_${this.callCount}`,
      finishReason: "stop",
    };
  }
}

export class StaticProviderRegistry implements ProviderRegistry {
  private readonly providers: ReadonlyMap<ProviderId, ModelProvider>;

  constructor(providers: readonly ModelProvider[]) {
    this.providers = new Map(providers.map((provider) => [provider.id, provider]));
  }

  get(id: ProviderId): ModelProvider {
    const provider = this.providers.get(id);
    if (provider === undefined) {
      throw new RuntimeError("PROVIDER_UNAVAILABLE", "The configured provider is unavailable.");
    }
    return provider;
  }
}

export interface ModelPrice {
  inputPerMillionUsd: number;
  outputPerMillionUsd: number;
}

export class FixedPriceCostCalculator {
  constructor(private readonly prices: Readonly<Record<string, ModelPrice>>) {}

  isConfigured(provider: ProviderId, model: string): boolean {
    return this.prices[`${provider}/${model}`] !== undefined;
  }

  estimate(provider: ProviderId, model: string, usage: ProviderResult["usage"]): string {
    const price = this.prices[`${provider}/${model}`];
    if (price === undefined) {
      if (provider === "fake") return "0.000000";
      throw new RuntimeError("PROVIDER_UNAVAILABLE", "The configured model is unavailable.");
    }
    const cost =
      ((usage.inputTokens ?? 0) * price.inputPerMillionUsd +
        (usage.outputTokens ?? 0) * price.outputPerMillionUsd) /
      1_000_000;
    return cost.toFixed(6);
  }
}

export { GoogleProvider, normalizeGoogleError } from "./google/adapter.js";
export { OpenAIProvider, normalizeOpenAIError } from "./openai/adapter.js";
