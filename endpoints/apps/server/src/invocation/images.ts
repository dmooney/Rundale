import { RuntimeError, type InvocationAttachment } from "@parish/runtime";

function startsWith(bytes: Uint8Array, signature: readonly number[]): boolean {
  return signature.every((value, index) => bytes[index] === value);
}

export function detectImageMediaType(bytes: Uint8Array): InvocationAttachment["mediaType"] {
  if (startsWith(bytes, [0xff, 0xd8, 0xff])) return "image/jpeg";
  if (startsWith(bytes, [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a])) return "image/png";
  if (
    startsWith(bytes, [0x52, 0x49, 0x46, 0x46]) &&
    bytes[8] === 0x57 &&
    bytes[9] === 0x45 &&
    bytes[10] === 0x42 &&
    bytes[11] === 0x50
  ) {
    return "image/webp";
  }
  throw new RuntimeError(
    "UNSUPPORTED_MEDIA_TYPE",
    "Image content is not a supported JPEG, PNG, or WEBP file.",
  );
}

function pngDimensions(bytes: Uint8Array): { width: number; height: number } | null {
  if (bytes.length < 24) return null;
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  return { width: view.getUint32(16), height: view.getUint32(20) };
}

function jpegDimensions(bytes: Uint8Array): { width: number; height: number } | null {
  let offset = 2;
  while (offset + 8 < bytes.length) {
    if (bytes[offset] !== 0xff) return null;
    const marker = bytes[offset + 1]!;
    if (marker === 0xd8 || marker === 0xd9) {
      offset += 2;
      continue;
    }
    const length = (bytes[offset + 2]! << 8) | bytes[offset + 3]!;
    if (length < 2 || offset + length + 2 > bytes.length) return null;
    if ((marker >= 0xc0 && marker <= 0xc3) || (marker >= 0xc5 && marker <= 0xc7)) {
      return {
        height: (bytes[offset + 5]! << 8) | bytes[offset + 6]!,
        width: (bytes[offset + 7]! << 8) | bytes[offset + 8]!,
      };
    }
    offset += length + 2;
  }
  return null;
}

function webpDimensions(bytes: Uint8Array): { width: number; height: number } | null {
  if (bytes.length < 30) return null;
  const chunk = String.fromCharCode(...bytes.slice(12, 16));
  if (chunk === "VP8X") {
    return {
      width: (bytes[24]! | (bytes[25]! << 8) | (bytes[26]! << 16)) + 1,
      height: (bytes[27]! | (bytes[28]! << 8) | (bytes[29]! << 16)) + 1,
    };
  }
  if (chunk === "VP8 " && bytes[23] === 0x9d && bytes[24] === 0x01 && bytes[25] === 0x2a) {
    return {
      width: (bytes[26]! | (bytes[27]! << 8)) & 0x3fff,
      height: (bytes[28]! | (bytes[29]! << 8)) & 0x3fff,
    };
  }
  if (chunk === "VP8L" && bytes[20] === 0x2f) {
    return {
      width: 1 + bytes[21]! + ((bytes[22]! & 0x3f) << 8),
      height: 1 + (bytes[22]! >> 6) + (bytes[23]! << 2) + ((bytes[24]! & 0x0f) << 10),
    };
  }
  return null;
}

export function assertImageDimensions(
  bytes: Uint8Array,
  mediaType: InvocationAttachment["mediaType"],
  maximumPixels: number,
): void {
  const dimensions =
    mediaType === "image/png"
      ? pngDimensions(bytes)
      : mediaType === "image/jpeg"
        ? jpegDimensions(bytes)
        : webpDimensions(bytes);
  if (
    dimensions === null ||
    dimensions.width < 1 ||
    dimensions.height < 1 ||
    dimensions.width * dimensions.height > maximumPixels
  ) {
    throw new RuntimeError(
      "INVALID_INPUT",
      `Image dimensions are invalid or exceed the ${maximumPixels.toLocaleString("en-US")} pixel limit.`,
    );
  }
}

export function imageFields(schema: Record<string, unknown>): string[] {
  const properties = schema.properties;
  if (properties === null || typeof properties !== "object" || Array.isArray(properties)) return [];
  return Object.entries(properties)
    .filter(([, value]) => {
      if (value === null || typeof value !== "object" || Array.isArray(value)) return false;
      return (value as Record<string, unknown>)["x-semantic-type"] === "image";
    })
    .map(([key]) => key);
}
