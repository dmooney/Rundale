/** The report the iPhone app sends, validated. */

/** The engine's report ceiling (`limerick_diagnostics::mobile_report`). */
export const REPORT_LIMIT = 50_000;
export const DESCRIPTION_LIMIT = 2_000;
const SHORT_FIELD_LIMIT = 200;
/** Decoded PNG bytes. An iPhone screenshot PNG is about 1-3 MB. */
export const SCREENSHOT_LIMIT = 6 * 1024 * 1024;
const REPORT_ID = /^[A-Za-z0-9-]{8,64}$/;
const PNG_SIGNATURE = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);

export interface BugReport {
  /** Client-generated identity, so a resent report files once. */
  reportId: string;
  description: string;
  report: string;
  build: string | undefined;
  device: string | undefined;
  screenshot: Buffer | undefined;
}

export type Parsed = { ok: true; value: BugReport } | { ok: false; error: string };

function chars(text: string): number {
  return [...text].length;
}

function optionalShort(body: Record<string, unknown>, key: string): string | undefined | null {
  const value = body[key];
  if (value === undefined || value === null) return undefined;
  if (typeof value !== "string" || chars(value) > SHORT_FIELD_LIMIT) return null;
  return value;
}

/** Validates a request body. Rejects rather than trims, so nothing the
 * phone sent is silently lost. */
export function parseReport(body: unknown): Parsed {
  if (typeof body !== "object" || body === null || Array.isArray(body))
    return { ok: false, error: "the body must be a JSON object" };
  const fields = body as Record<string, unknown>;
  const reportId = fields.reportId;
  if (typeof reportId !== "string" || !REPORT_ID.test(reportId))
    return { ok: false, error: "reportId must be 8-64 letters, digits, or dashes" };
  const report = fields.report;
  if (typeof report !== "string" || report.trim().length === 0 || chars(report) > REPORT_LIMIT)
    return { ok: false, error: `report must be 1-${REPORT_LIMIT} characters` };
  const description = fields.description ?? "";
  if (typeof description !== "string" || chars(description) > DESCRIPTION_LIMIT)
    return { ok: false, error: `description must be at most ${DESCRIPTION_LIMIT} characters` };
  const build = optionalShort(fields, "build");
  const device = optionalShort(fields, "device");
  if (build === null || device === null)
    return { ok: false, error: `build and device must be at most ${SHORT_FIELD_LIMIT} characters` };
  let screenshot: Buffer | undefined;
  if (fields.screenshot !== undefined && fields.screenshot !== null) {
    if (typeof fields.screenshot !== "string")
      return { ok: false, error: "screenshot must be base64 PNG" };
    screenshot = Buffer.from(fields.screenshot, "base64");
    if (screenshot.length > SCREENSHOT_LIMIT)
      return { ok: false, error: `screenshot must be at most ${SCREENSHOT_LIMIT} bytes` };
    if (!screenshot.subarray(0, 8).equals(PNG_SIGNATURE))
      return { ok: false, error: "screenshot must be base64 PNG" };
  }
  return { ok: true, value: { reportId, description, report, build, device, screenshot } };
}
