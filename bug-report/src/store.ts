import type { Bucket } from "@google-cloud/storage";
import type { Reporter } from "./auth.js";
import type { BugReport } from "./report.js";

/** What the triage agent reads from `inbox/<reportId>/report.json`. */
export interface StoredReport {
  reportId: string;
  receivedAt: string;
  reporter: Reporter;
  description: string;
  report: string;
  build: string | null;
  device: string | null;
  /** Whether `inbox/<reportId>/screenshot.png` exists. */
  screenshot: boolean;
}

export type SaveOutcome = "stored" | "duplicate";

/** The private inbox the triage agent pulls from (`/bug-triage`). */
export interface ReportStore {
  save(report: BugReport, reporter: Reporter, receivedAt: Date): Promise<SaveOutcome>;
}

export function storedReport(
  report: BugReport,
  reporter: Reporter,
  receivedAt: Date,
): StoredReport {
  return {
    reportId: report.reportId,
    receivedAt: receivedAt.toISOString(),
    reporter,
    description: report.description,
    report: report.report,
    build: report.build ?? null,
    device: report.device ?? null,
    screenshot: report.screenshot !== undefined,
  };
}

/**
 * A private Cloud Storage bucket. The screenshot is written first and
 * `report.json` last, so a report the agent can see is complete. A report
 * whose `report.json` already exists is a resend: the create-only
 * precondition refuses to overwrite it.
 */
export class BucketReportStore implements ReportStore {
  constructor(private readonly bucket: Bucket) {}

  async save(report: BugReport, reporter: Reporter, receivedAt: Date): Promise<SaveOutcome> {
    const prefix = `inbox/${report.reportId}`;
    const json = this.bucket.file(`${prefix}/report.json`);
    try {
      if (report.screenshot !== undefined) {
        await this.bucket.file(`${prefix}/screenshot.png`).save(report.screenshot, {
          contentType: "image/png",
          resumable: false,
          preconditionOpts: { ifGenerationMatch: 0 },
        });
      }
    } catch (error) {
      // A resend whose screenshot already arrived; report.json decides.
      if ((error as { code?: number }).code !== 412) throw error;
    }
    try {
      await json.save(JSON.stringify(storedReport(report, reporter, receivedAt), null, 2), {
        contentType: "application/json",
        resumable: false,
        preconditionOpts: { ifGenerationMatch: 0 },
      });
      return "stored";
    } catch (error) {
      if ((error as { code?: number }).code === 412) return "duplicate";
      throw error;
    }
  }
}
