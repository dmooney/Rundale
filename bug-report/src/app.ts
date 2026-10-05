import rateLimit from "@fastify/rate-limit";
import Fastify, { type FastifyInstance } from "fastify";
import type { ReporterAuthenticator } from "./auth.js";
import { parseReport } from "./report.js";
import type { ReportStore } from "./store.js";

export interface AppOptions {
  authenticator: ReporterAuthenticator;
  store: ReportStore;
  /** Reports one reporter may send per hour. */
  hourlyLimit: number;
  /** Requests one address may make per minute, checked before credentials. */
  perMinuteLimit?: number;
  logger?: boolean;
  now?: () => number;
}

/** Base64 screenshot plus the report and fields, with room to spare. */
const BODY_LIMIT = 10 * 1024 * 1024;
const HOUR_MS = 60 * 60 * 1000;

/**
 * `POST /v1/reports` stores one bug report in the private inbox and answers
 * `202 {reportId}`. Resending a stored report answers the same without
 * storing it again or counting it; like any report, a resend over the hourly
 * limit answers 429, and the phone keeps it for later. Nothing the phone
 * sends is logged.
 */
export async function buildApp(options: AppOptions): Promise<FastifyInstance> {
  const now = options.now ?? Date.now;
  // Cloud Run's front end is one proxy hop and appends the real client
  // address last in X-Forwarded-For. Trusting exactly one hop takes that entry;
  // anything a client put earlier in the header is ignored.
  const app = Fastify({
    logger: options.logger ?? false,
    bodyLimit: BODY_LIMIT,
    trustProxy: (_address, hop) => hop === 0,
  });
  // Per-address limit before any credential check, so unauthenticated floods
  // cannot spend Firebase verification calls.
  await app.register(rateLimit, { max: options.perMinuteLimit ?? 30, timeWindow: "1 minute" });
  const recent = new Map<string, number[]>();

  /** Claims one of the player's hourly slots before storing, so concurrent
   * requests cannot all pass the check; returns the claim's time, or `null`
   * over the limit. Players with no recent reports are forgotten. */
  const claim = (uid: string): number | null => {
    const cutoff = now() - HOUR_MS;
    for (const [player, times] of recent) {
      const current = times.filter((time) => time > cutoff);
      if (current.length === 0) recent.delete(player);
      else recent.set(player, current);
    }
    const times = recent.get(uid) ?? [];
    if (times.length >= options.hourlyLimit) return null;
    const at = now();
    times.push(at);
    recent.set(uid, times);
    return at;
  };
  const release = (uid: string, at: number) => {
    const times = recent.get(uid);
    const index = times?.indexOf(at) ?? -1;
    if (times !== undefined && index >= 0) times.splice(index, 1);
  };

  app.get("/health", async () => ({ ok: true }));

  app.post("/v1/reports", async (request, reply) => {
    const reporter = await options.authenticator.authenticate(request.headers);
    if (reporter === null) return reply.code(401).send({ error: "unauthenticated" });
    const parsed = parseReport(request.body);
    if (!parsed.ok) return reply.code(400).send({ error: parsed.error });
    const claimed = claim(reporter.uid);
    if (claimed === null) return reply.code(429).send({ error: "too many reports" });
    try {
      const outcome = await options.store.save(parsed.value, reporter, new Date(now()));
      // A resend stores nothing, so it gives its slot back.
      if (outcome === "duplicate") release(reporter.uid, claimed);
      return reply.code(202).send({ reportId: parsed.value.reportId });
    } catch (error) {
      release(reporter.uid, claimed);
      request.log.error({ err: error }, "report storage failed");
      return reply.code(503).send({ error: "the report could not be stored" });
    }
  });

  return app;
}
