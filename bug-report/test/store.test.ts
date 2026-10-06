import type { Bucket } from "@google-cloud/storage";
import { describe, expect, it } from "vitest";
import { BucketReportStore } from "../src/store.js";

/** Records writes; a create-only write to an existing object fails 412. */
function fakeBucket() {
  const objects = new Map<string, { data: string | Buffer; options: Record<string, unknown> }>();
  const bucket = {
    file(name: string) {
      return {
        async save(data: string | Buffer, options: Record<string, unknown>) {
          const precondition = options.preconditionOpts as
            { ifGenerationMatch?: number } | undefined;
          if (precondition?.ifGenerationMatch === 0 && objects.has(name))
            throw Object.assign(new Error("precondition failed"), { code: 412 });
          objects.set(name, { data, options });
        },
      };
    },
  } as unknown as Bucket;
  return { bucket, objects };
}

const report = {
  reportId: "aaaaaaaa",
  description: "",
  report: "Rundale bug report",
  build: undefined,
  device: undefined,
  screenshot: Buffer.from([0x89, 0x50, 0x4e, 0x47]),
};
const reporter = { uid: "u1", appId: "app" };

describe("BucketReportStore", () => {
  it("writes the screenshot, then a create-only report.json", async () => {
    const { bucket, objects } = fakeBucket();
    const store = new BucketReportStore(bucket);
    expect(await store.save(report, reporter, new Date(0))).toBe("stored");
    expect([...objects.keys()]).toEqual([
      "inbox/aaaaaaaa/screenshot.png",
      "inbox/aaaaaaaa/report.json",
    ]);
    const json = JSON.parse(String(objects.get("inbox/aaaaaaaa/report.json")?.data));
    expect(json).toMatchObject({ reportId: "aaaaaaaa", screenshot: true, build: null });
    expect(objects.get("inbox/aaaaaaaa/screenshot.png")?.options.contentType).toBe("image/png");
  });

  it("reports a resend as a duplicate without overwriting it", async () => {
    const { bucket, objects } = fakeBucket();
    const store = new BucketReportStore(bucket);
    await store.save(report, reporter, new Date(0));
    expect(await store.save({ ...report, report: "changed" }, reporter, new Date(1))).toBe(
      "duplicate",
    );
    expect(String(objects.get("inbox/aaaaaaaa/report.json")?.data)).toContain("Rundale bug report");
  });

  it("finishes a report whose screenshot arrived before a failure", async () => {
    const { bucket, objects } = fakeBucket();
    const store = new BucketReportStore(bucket);
    await bucket.file("inbox/aaaaaaaa/screenshot.png").save(Buffer.from("png"), {});
    expect(await store.save(report, reporter, new Date(0))).toBe("stored");
    expect(objects.has("inbox/aaaaaaaa/report.json")).toBe(true);
  });
});
