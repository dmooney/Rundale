import type { EndpointVersionSnapshot } from "@parish/domain";
import type { RuntimeAttempt } from "@parish/runtime";
import type {
  InvocationApiKey,
  InvocationRepository,
  InvocationStart,
  ResolvedEndpoint,
} from "../src/invocation/contracts.js";
import { InvocationQuotaExceededError } from "../src/invocation/contracts.js";

export class MemoryInvocationRepository implements InvocationRepository {
  public attempts: RuntimeAttempt[] = [];
  public finalStatus: "succeeded" | "failed" | null = null;
  public finalSuccessResult: Parameters<InvocationRepository["finalizeSuccess"]>[1] | null = null;
  public finalFailureResult: Parameters<InvocationRepository["finalizeFailure"]>[1] | null = null;
  public created = 0;
  public used = 0;
  public inferenceEnabled = true;
  public productionVersion = 1;
  public forceFinalizationFailure = false;
  private readonly invocationStates = new Map<string, "running" | "succeeded" | "failed">();

  constructor(
    public key: InvocationApiKey,
    public readonly endpoint: Omit<ResolvedEndpoint, "version">,
    public readonly versions: EndpointVersionSnapshot[],
  ) {}

  async findApiKeyByPrefix(prefix: string): Promise<InvocationApiKey | null> {
    return prefix === "abcdefghijkl" ? this.key : null;
  }

  async resolveProduction(
    organizationSlug: string,
    endpointSlug: string,
  ): Promise<ResolvedEndpoint | null> {
    return this.resolveVersion(organizationSlug, endpointSlug, this.productionVersion);
  }

  async resolveVersion(
    organizationSlug: string,
    endpointSlug: string,
    version: number,
  ): Promise<ResolvedEndpoint | null> {
    const snapshot = this.versions.find((item) => item.version === version);
    if (
      organizationSlug !== "acme" ||
      endpointSlug !== this.endpoint.endpointSlug ||
      snapshot === undefined
    ) {
      return null;
    }
    return { ...this.endpoint, version: snapshot };
  }

  async isInferenceEnabled(_provider: string, _model: string): Promise<boolean> {
    void _provider;
    void _model;
    return this.inferenceEnabled;
  }

  async createInvocation(_start: InvocationStart, dailyInvocationQuota: number): Promise<string> {
    void _start;
    if (this.used >= Math.min(this.key.dailyInvocationQuota, dailyInvocationQuota)) {
      throw new InvocationQuotaExceededError();
    }
    this.created += 1;
    this.used += 1;
    const id = `invocation_${this.created}`;
    this.invocationStates.set(id, "running");
    return id;
  }

  async recordAttempts(_invocationId: string, attempts: readonly RuntimeAttempt[]): Promise<void> {
    this.attempts.push(...attempts);
  }

  async finalizeSuccess(
    _invocationId: string,
    result: Parameters<InvocationRepository["finalizeSuccess"]>[1],
  ): Promise<boolean> {
    this.finalSuccessResult = result;
    if (this.forceFinalizationFailure || this.invocationStates.get(_invocationId) !== "running") {
      return false;
    }
    this.invocationStates.set(_invocationId, "succeeded");
    this.finalStatus = "succeeded";
    return true;
  }

  async finalizeFailure(
    _invocationId: string,
    result: Parameters<InvocationRepository["finalizeFailure"]>[1],
  ): Promise<boolean> {
    this.finalFailureResult = result;
    if (this.forceFinalizationFailure || this.invocationStates.get(_invocationId) !== "running") {
      return false;
    }
    this.invocationStates.set(_invocationId, "failed");
    this.finalStatus = "failed";
    return true;
  }

  async touchApiKey(): Promise<void> {}
}
