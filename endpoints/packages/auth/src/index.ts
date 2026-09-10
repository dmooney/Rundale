import { createHash, randomBytes, timingSafeEqual } from "node:crypto";

const keyPattern = /^sfk_live_([a-zA-Z0-9]{12})_([a-zA-Z0-9_-]{32,})$/;

export interface IssuedApiKey {
  secret: string;
  prefix: string;
  digest: string;
}

export function hashApiKey(secret: string): string {
  return createHash("sha256").update(secret, "utf8").digest("hex");
}

export function issueApiKey(): IssuedApiKey {
  const prefix = randomBytes(6).toString("hex");
  const material = randomBytes(32).toString("base64url");
  const secret = `sfk_live_${prefix}_${material}`;
  return { secret, prefix, digest: hashApiKey(secret) };
}

export function parseApiKey(secret: string): { prefix: string } | null {
  const match = keyPattern.exec(secret);
  return match?.[1] === undefined ? null : { prefix: match[1] };
}

export function verifyApiKey(secret: string, expectedDigest: string): boolean {
  const actual = Buffer.from(hashApiKey(secret), "hex");
  const expected = Buffer.from(expectedDigest, "hex");
  return actual.length === expected.length && timingSafeEqual(actual, expected);
}

export function permitsEndpoint(scopes: readonly string[], endpointSlug: string): boolean {
  return scopes.includes("invoke:endpoint:*") || scopes.includes(`invoke:endpoint:${endpointSlug}`);
}
