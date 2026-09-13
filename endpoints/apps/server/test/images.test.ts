import { describe, expect, it } from "vitest";
import { assertImageDimensions } from "../src/invocation/images.js";

function png(width: number, height: number): Uint8Array {
  const bytes = Uint8Array.from([
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 13, 0x49, 0x48, 0x44, 0x52, 0, 0, 0, 0,
    0, 0, 0, 0,
  ]);
  const view = new DataView(bytes.buffer);
  view.setUint32(16, width);
  view.setUint32(20, height);
  return bytes;
}

describe("image dimensions", () => {
  it("accepts an image within the decoded pixel limit", () => {
    expect(() => assertImageDimensions(png(100, 200), "image/png", 20_000)).not.toThrow();
  });

  it("rejects a decompression-bomb-sized image before provider execution", () => {
    expect(() => assertImageDimensions(png(10_000, 10_000), "image/png", 40_000_000)).toThrow(
      /pixel limit/,
    );
  });
});
