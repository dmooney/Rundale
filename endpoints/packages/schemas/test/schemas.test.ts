import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import type { JsonSchema } from "@parish/domain";
import {
  compileSchema,
  formatValidationErrors,
  SchemaDefinitionError,
  validateSchemaDefinition,
} from "../src/index.js";

const seedPacketSchema = JSON.parse(
  readFileSync(
    new URL("../../../examples/node-cli/seed-packet-output-schema.json", import.meta.url),
  ),
) as JsonSchema;

describe("constrained JSON Schema", () => {
  it("accepts the platform image extension", () => {
    expect(() =>
      validateSchemaDefinition({
        type: "object",
        properties: {
          image: {
            type: "string",
            contentMediaType: "image/*",
            "x-semantic-type": "image",
          },
        },
        required: ["image"],
        additionalProperties: false,
      }),
    ).not.toThrow();
  });

  it("rejects image fields outside the single top-level property boundary", () => {
    const imageField = {
      type: "string",
      contentMediaType: "image/*",
      "x-semantic-type": "image",
    };
    const invalidSchemas = [
      { type: "string", ...imageField },
      {
        type: "object",
        properties: {
          nested: { type: "object", properties: { image: imageField } },
        },
      },
      {
        type: "object",
        properties: { images: { type: "array", items: imageField } },
      },
      {
        type: "object",
        properties: { optionalImage: { anyOf: [imageField, { type: "null" }] } },
      },
      {
        type: "object",
        properties: {
          firstImage: imageField,
          secondImage: { ...imageField },
        },
      },
    ];

    for (const schema of invalidSchemas) {
      expect(() => validateSchemaDefinition(schema)).toThrow(SchemaDefinitionError);
    }
  });

  it("accepts the current SeedPacket output contract fixture", () => {
    expect(() => validateSchemaDefinition(seedPacketSchema)).not.toThrow();
    const validate = compileSchema(seedPacketSchema);
    const validSeedPacket = {
      name: "Lettuce",
      variety: "Buttercrunch",
      brand: "Example Seeds",
      category: "Vegetable",
      description: "A leafy green.",
      plantingInstructions: "Sow in prepared soil.",
      regionalTiming: "Spring",
      regionalMapCrop: null,
      plantImageCrop: null,
      packetIdentifiers: "SKU-1",
      packetNotes: "",
      containerSuitability: null,
      daysToMaturityMin: null,
      daysToMaturityMax: null,
      plantingWindows: [
        {
          frostAnchor: "lastFrost",
          startIndoorsOffsetDays: null,
          directSowOffsetDays: null,
          transplantOffsetDays: null,
        },
      ],
      seedSpacingInches: null,
      thinningSpacingInches: null,
      rowSpacingInches: null,
      depthInches: null,
      sun: null,
      confidence: null,
      packetEvidence: { schemaVersion: 2, facts: [] },
    };
    expect(validate(validSeedPacket)).toBe(true);
    expect(validate({})).toBe(false);
    expect(
      validate({
        ...validSeedPacket,
        plantingWindows: [
          ...validSeedPacket.plantingWindows,
          ...validSeedPacket.plantingWindows,
          ...validSeedPacket.plantingWindows,
          ...validSeedPacket.plantingWindows,
          ...validSeedPacket.plantingWindows,
        ],
      }),
    ).toBe(false);
  });

  it("supports a nullable anyOf with one typed branch", () => {
    const validate = compileSchema({
      anyOf: [{ type: "string" }, { type: "null" }],
    });

    expect(validate("value")).toBe(true);
    expect(validate(null)).toBe(true);
    expect(validate(42)).toBe(false);
  });

  it("supports bounded arrays with maxItems", () => {
    const validate = compileSchema({
      type: "array",
      maxItems: 2,
      items: { type: "string" },
    });

    expect(validate(["one", "two"])).toBe(true);
    expect(validate(["one", "two", "three"])).toBe(false);
  });

  it("rejects malformed nullable composition and limits", () => {
    const invalidSchemas = [
      { anyOf: [{ type: "string" }] },
      { anyOf: [{ type: "string" }, { type: "number" }, { type: "null" }] },
      { anyOf: [{ type: "string" }, { type: "number" }] },
      { anyOf: [{ type: "null" }, { type: "null" }] },
      { anyOf: [{ type: ["string", "null"] }, { type: "null" }] },
      { anyOf: [{ type: "string" }, false] },
      { type: "array", maxItems: -1 },
      { type: "array", maxItems: 1.5 },
      { type: "array", maxItems: "2" },
    ];

    for (const schema of invalidSchemas) {
      expect(() => validateSchemaDefinition(schema)).toThrow(SchemaDefinitionError);
    }
  });

  it("rejects unsupported composition and references", () => {
    for (const schema of [{ oneOf: [] }, { $ref: "#/$defs/value" }]) {
      expect(() => validateSchemaDefinition(schema)).toThrow(SchemaDefinitionError);
    }
  });

  it("validates values independently", () => {
    const validate = compileSchema({
      type: "object",
      properties: { result: { type: "string", minLength: 1 } },
      required: ["result"],
      additionalProperties: false,
    });
    expect(validate({ result: "ok" })).toBe(true);
    expect(validate({})).toBe(false);
    expect(formatValidationErrors(validate.errors)[0]?.path).toBe("$");
  });
});
