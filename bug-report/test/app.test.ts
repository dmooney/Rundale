import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { buildApp } from "../src/app.js";
import { ReporterAuthenticator, type FirebaseVerifier } from "../src/auth.js";
import type { BugReport } from "../src/report.js";
import {
  storedReport,
  type ReportStore,
  type SaveOutcome,
  type StoredReport,
} from "../src/store.js";

const APP_ID = "1:877612517009:ios:586f98a2cc3e7d0c676130";
const PNG = Buffer.concat([
  Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
  Buffer.from("rest of the image"),
]);

const verifier: FirebaseVerifier = {
  async verifyIdToken(token) {
    if (!token.startsWith("id-")) throw new Error("bad id token");
    return { uid: token.slice(3) };
  },
  async verifyAppCheckToken(token) {
    if (token === "bad") throw new Error("bad app check");
    return { appId: token };
  },
};

class FakeStore implements ReportStore {
  saved: { json: StoredReport; screenshotBytes: number | undefined }[] = [];
  fail = false;

  async save(
    report: BugReport,
    reporter: { uid: string; appId: string },
    at: Date,
  ): Promise<SaveOutcome> {
    if (this.fail) throw new Error("storage is down");
    if (this.saved.some((entry) => entry.json.reportId === report.reportId)) return "duplicate";
    this.saved.push({
      json: storedReport(report, reporter, at),
      screenshotBytes: report.screenshot?.length,
    });
    return "stored";
  }
}

async function setup(hourlyLimit = 20, perMinuteLimit = 100) {
  const store = new FakeStore();
  const app = await buildApp({
    authenticator: new ReporterAuthenticator(verifier, [APP_ID]),
    store,
    hourlyLimit,
    perMinuteLimit,
    now: () => Date.parse("2026-10-05T16:00:00Z"),
  });
  const send = (body: unknown, headers: Record<string, string> = {}) =>
    app.inject({
      method: "POST",
      url: "/v1/reports",
      headers: {
        authorization: "Bearer id-player1",
        "x-firebase-appcheck": APP_ID,
        ...headers,
      },
      payload: body as Record<string, unknown>,
    });
  return { app, store, send };
}

const report = (overrides: Record<string, unknown> = {}) => ({
  reportId: "8d6c1b0e-5a37-4c1e-9d55-0a1f2b3c4d5e",
  description: "Mícheál never looked up",
  report: "Rundale bug report\nMícheál never looked up\n\nScene: Connolly Cottage",
  build: "0.1.0 (1284)",
  device: "iPhone17,2 iOS 26.6",
  screenshot: PNG.toString("base64"),
  ...overrides,
});

describe("POST /v1/reports", () => {
  it("accepts the report exactly as the iPhone app encodes it", async () => {
    // The Swift side encodes against this same file (RundaleBugReportTests).
    const fixture = JSON.parse(
      readFileSync(new URL("./fixtures/phone-report.json", import.meta.url), "utf8"),
    );
    const { store, send } = await setup();
    expect((await send(fixture)).statusCode).toBe(202);
    expect(store.saved[0]?.json.reportId).toBe(fixture.reportId);
    expect(store.saved[0]?.json.screenshot).toBe(true);
  });

  it("stores the report and screenshot privately and answers 202", async () => {
    const { store, send } = await setup();
    const response = await send(report());
    expect(response.statusCode).toBe(202);
    expect(response.json()).toEqual({ reportId: "8d6c1b0e-5a37-4c1e-9d55-0a1f2b3c4d5e" });
    expect(store.saved).toEqual([
      {
        json: {
          reportId: "8d6c1b0e-5a37-4c1e-9d55-0a1f2b3c4d5e",
          receivedAt: "2026-10-05T16:00:00.000Z",
          reporter: { uid: "player1", appId: APP_ID },
          description: "Mícheál never looked up",
          report: "Rundale bug report\nMícheál never looked up\n\nScene: Connolly Cottage",
          build: "0.1.0 (1284)",
          device: "iPhone17,2 iOS 26.6",
          screenshot: true,
        },
        screenshotBytes: PNG.length,
      },
    ]);
  });

  it("answers a resend over the limit with 429 like any report", async () => {
    const { store, send } = await setup(1);
    expect((await send(report())).statusCode).toBe(202);
    expect((await send(report())).statusCode).toBe(429);
    expect(store.saved).toHaveLength(1);
  });

  it("answers a resend of a stored report with 202", async () => {
    const { store, send } = await setup();
    expect((await send(report())).statusCode).toBe(202);
    expect((await send(report())).statusCode).toBe(202);
    expect(store.saved).toHaveLength(1);
  });

  it("stores a report without a screenshot", async () => {
    const { store, send } = await setup();
    expect((await send(report({ screenshot: undefined }))).statusCode).toBe(202);
    expect(store.saved[0]?.json.screenshot).toBe(false);
  });

  it("answers 503 when storage fails, so the phone keeps the report", async () => {
    const { store, send } = await setup();
    store.fail = true;
    expect((await send(report())).statusCode).toBe(503);
    store.fail = false;
    expect((await send(report())).statusCode).toBe(202);
  });

  it.each([
    ["no ID token", { authorization: "" }],
    ["a bad ID token", { authorization: "Bearer nope" }],
    ["no App Check token", { "x-firebase-appcheck": "" }],
    ["a bad App Check token", { "x-firebase-appcheck": "bad" }],
    ["another app", { "x-firebase-appcheck": "1:1:ios:other" }],
  ])("refuses %s", async (_, headers) => {
    const { store, send } = await setup();
    expect((await send(report(), headers)).statusCode).toBe(401);
    expect(store.saved).toHaveLength(0);
  });

  it.each([
    ["no reportId", { reportId: undefined }],
    ["a reportId that could escape its folder", { reportId: "../../etc/passwd" }],
    ["an empty report", { report: "  " }],
    ["a report over the limit", { report: "x".repeat(50_001) }],
    ["a long description", { description: "x".repeat(2_001) }],
    ["a screenshot that is not PNG", { screenshot: Buffer.from("GIF89a....").toString("base64") }],
  ])("rejects %s", async (_, overrides) => {
    const { store, send } = await setup();
    expect((await send(report(overrides))).statusCode).toBe(400);
    expect(store.saved).toHaveLength(0);
  });

  it("limits each address per minute before checking credentials", async () => {
    const { send } = await setup(20, 2);
    const forged = { authorization: "Bearer nope" };
    expect((await send(report(), forged)).statusCode).toBe(401);
    expect((await send(report(), forged)).statusCode).toBe(401);
    expect((await send(report(), forged)).statusCode).toBe(429);
  });

  it("limits reports per player per hour", async () => {
    const { send } = await setup(2);
    for (const id of ["aaaaaaaa", "bbbbbbbb"])
      expect((await send(report({ reportId: id }))).statusCode).toBe(202);
    expect((await send(report({ reportId: "cccccccc" }))).statusCode).toBe(429);
    expect(
      (await send(report({ reportId: "cccccccc" }), { authorization: "Bearer id-player2" }))
        .statusCode,
    ).toBe(202);
  });
});
