import { describe, expect, it } from "vitest";
import { issueApiKey, parseApiKey, permitsEndpoint, verifyApiKey } from "../src/index.js";

describe("invocation API keys", () => {
  it("issues a one-time secret and stores only its digest and prefix", () => {
    const key = issueApiKey();
    expect(key.secret).toMatch(/^sfk_live_/);
    expect(key.digest).not.toContain(key.secret);
    expect(parseApiKey(key.secret)).toEqual({ prefix: key.prefix });
    expect(verifyApiKey(key.secret, key.digest)).toBe(true);
    expect(verifyApiKey(`${key.secret}x`, key.digest)).toBe(false);
  });

  it("supports wildcard and Endpoint-specific scopes", () => {
    expect(permitsEndpoint(["invoke:endpoint:*"], "extractor")).toBe(true);
    expect(permitsEndpoint(["invoke:endpoint:extractor"], "extractor")).toBe(true);
    expect(permitsEndpoint(["invoke:endpoint:other"], "extractor")).toBe(false);
  });
});
