import fs from "fs";
import path from "path";

export const QDIR = ".quality-harness";
export const MAX_FILE_BYTES = 50 * 1024;

export function ensureQualityDir() {
  fs.mkdirSync(QDIR, { recursive: true });
}

export function truncateContent(text: string, maxBytes = MAX_FILE_BYTES): string {
  const b = Buffer.from(text, "utf8");
  if (b.length <= maxBytes) return text;
  return `${b.subarray(0, maxBytes).toString("utf8")}\n...TRUNCATED...`;
}

export function safeRead(file: string): string | null {
  try {
    const content = fs.readFileSync(file, "utf8");
    return truncateContent(content);
  } catch {
    return null;
  }
}

export function writeJson(file: string, value: unknown) {
  ensureQualityDir();
  fs.writeFileSync(file, `${JSON.stringify(value, null, 2)}\n`, "utf8");
}

export function rel(p: string): string { return path.relative(process.cwd(), p); }
