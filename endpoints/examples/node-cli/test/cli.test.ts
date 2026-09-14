import { writeFile } from "node:fs/promises";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { CliError, invokeImage, parseOptions } from "../src/cli.js";

describe("TypeScript image CLI", () => {
  it("requires the API key through environment configuration", () => {
    expect(() =>
      parseOptions(["--image", "fixture.png", "--endpoint", "https://api.example.test"]),
    ).toThrow("PARISH_API_KEY");
    expect(() =>
      parseOptions(
        ["--image", "fixture.png", "--endpoint", "https://api.example.test", "--api-key", "secret"],
        {},
      ),
    ).toThrow(CliError);
  });

  it("accepts the argument separator forwarded by pnpm", () => {
    expect(
      parseOptions(["--", "--image", "fixture.png", "--endpoint", "https://api.example.test"], {
        PARISH_API_KEY: "sfk_live_secret",
      }),
    ).toMatchObject({
      imagePath: "fixture.png",
      endpointUrl: "https://api.example.test/",
      apiKey: "sfk_live_secret",
    });
  });

  it("sends one multipart image and validates returned JSON", async () => {
    const base = process.env.TMPDIR ?? "/tmp";
    const imagePath = join(base, `parish-cli-${process.pid}.png`);
    const schemaPath = join(base, `parish-cli-${process.pid}.schema.json`);
    await writeFile(imagePath, Uint8Array.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]));
    await writeFile(
      schemaPath,
      JSON.stringify({
        type: "object",
        properties: { result: { type: "string" } },
        required: ["result"],
        additionalProperties: false,
      }),
    );
    let request: RequestInit | undefined;
    const result = await invokeImage(
      {
        endpointUrl: "https://api.example.test/v1/endpoints/acme/parser",
        imagePath,
        apiKey: "sfk_live_secret",
        schemaPath,
      },
      async (_input, init) => {
        request = init;
        return new Response(JSON.stringify({ result: "validated" }), {
          status: 200,
          headers: { "content-type": "application/json" },
        });
      },
    );
    expect(result).toEqual({ result: "validated" });
    expect(request?.headers).toEqual({ authorization: "Bearer sfk_live_secret" });
    expect(request?.body).toBeInstanceOf(FormData);
  });

  it("rejects an invalid output schema before making a request", async () => {
    const base = process.env.TMPDIR ?? "/tmp";
    const schemaPath = join(base, `parish-cli-${process.pid}.invalid-schema.json`);
    await writeFile(schemaPath, JSON.stringify({ oneOf: [] }));
    let requested = false;

    await expect(
      invokeImage(
        {
          endpointUrl: "https://api.example.test/v1/endpoints/acme/parser",
          imagePath: join(base, "missing-image.png"),
          apiKey: "sfk_live_secret",
          schemaPath,
        },
        async () => {
          requested = true;
          return new Response(JSON.stringify({ result: "unexpected" }), { status: 200 });
        },
      ),
    ).rejects.toThrow("JSON Schema uses invalid or unsupported constructs.");
    expect(requested).toBe(false);
  });
});
