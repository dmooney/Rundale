import Fastify, { type FastifyInstance } from "fastify";
import type { ReporterAuthenticator } from "./auth.js";
import { parseReport } from "./report.js";
import type { ReportStore } from "./store.js";

export interface AppOptions {
  authenticator: ReporterAuthenticator;
  store: ReportStore;
  /** Reports one reporter may send per hour. */
  hourlyLimit: number;
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
export function buildApp(options: AppOptions): FastifyInstance {
  const now = options.now ?? Date.now;
  const app = Fastify({ logger: options.logger ?? false, bodyLimit: BODY_LIMIT });
  const recent = new Map<string, number[]>();

  const allowed = (uid: string): boolean => {
    const cutoff = now() - HOUR_MS;
    const times = (recent.get(uid) ?? []).filter((time) => time > cutoff);
    recent.set(uid, times);
    return times.length < options.hourlyLimit;
  };

  app.get("/health", async () => ({ ok: true }));

  app.post("/v1/reports", async (request, reply) => {
    const reporter = await options.authenticator.authenticate(request.headers);
    if (reporter === null) return reply.code(401).send({ error: "unauthenticated" });
    const parsed = parseReport(request.body);
    if (!parsed.ok) return reply.code(400).send({ error: parsed.error });
    if (!allowed(reporter.uid)) return reply.code(429).send({ error: "too many reports" });
    try {
      const outcome = await options.store.save(parsed.value, reporter, new Date(now()));
      if (outcome === "stored") recent.get(reporter.uid)?.push(now());
      return reply.code(202).send({ reportId: parsed.value.reportId });
    } catch (error) {
      request.log.error({ err: error }, "report storage failed");
      return reply.code(503).send({ error: "the report could not be stored" });
    }
  });

  return app;
}
