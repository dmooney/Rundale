export class InMemoryRateGate {
  private readonly events = new Map<string, number[]>();

  constructor(private readonly maximumPerMinute: number) {}

  allow(keys: readonly string[], now = Date.now()): boolean {
    const cutoff = now - 60_000;
    for (const key of keys) {
      const recent = (this.events.get(key) ?? []).filter((timestamp) => timestamp > cutoff);
      if (recent.length >= this.maximumPerMinute) return false;
    }
    for (const key of keys) {
      const recent = (this.events.get(key) ?? []).filter((timestamp) => timestamp > cutoff);
      recent.push(now);
      this.events.set(key, recent);
    }
    return true;
  }
}
