import { describe, expect, it } from "vitest";
import { readServerConfig } from "../src/config.js";

describe("server configuration", () => {
  it("fails closed unless local development authentication is explicit", () => {
    expect(() => readServerConfig({ PROVIDER_MODE: "fake" })).toThrow(/FIREBASE_PROJECT_ID/);
    expect(
      readServerConfig({
        PROVIDER_MODE: "fake",
        FIREBASE_PROJECT_ID: "synthetic-project",
        DATABASE_URL: "postgres://synthetic",
        WEB_ORIGIN: "http://localhost:3000",
        PARISH_OWNER_FIREBASE_UID: "synthetic-owner",
      }).authMode,
    ).toBe("firebase");
    expect(readServerConfig({ AUTH_MODE: "development" }).authMode).toBe("development");
  });

  it("rejects development authentication on Cloud Run", () => {
    expect(() =>
      readServerConfig({
        K_SERVICE: "parish-server",
        AUTH_MODE: "development",
      }),
    ).toThrow(/Cloud Run requires AUTH_MODE=firebase/);
  });

  it("requires Firebase and live providers in production", () => {
    expect(() =>
      readServerConfig({
        NODE_ENV: "production",
        PROVIDER_MODE: "live",
        OPENAI_API_KEY: "synthetic",
        GOOGLE_API_KEY: "synthetic",
      }),
    ).toThrow(/FIREBASE_PROJECT_ID/);
    expect(() => readServerConfig({ NODE_ENV: "production", AUTH_MODE: "development" })).toThrow(
      /AUTH_MODE=firebase/,
    );
  });

  it("requires explicit prices for every allowed live model", () => {
    expect(() =>
      readServerConfig({
        NODE_ENV: "development",
        AUTH_MODE: "development",
        PROVIDER_MODE: "live",
        OPENAI_API_KEY: "synthetic",
        GOOGLE_API_KEY: "synthetic",
        OPENAI_ALLOWED_MODELS: "model-a",
        GOOGLE_ALLOWED_MODELS: "model-b",
        MODEL_PRICES_JSON: JSON.stringify({
          "openai/model-a": { inputPerMillionUsd: 1, outputPerMillionUsd: 2 },
        }),
      }),
    ).toThrow(/price every allowed live model/);
  });

  it("accepts non-negative versioned price configuration", () => {
    const config = readServerConfig({
      AUTH_MODE: "development",
      MODEL_PRICES_JSON: JSON.stringify({
        "fake/fake-v1": { inputPerMillionUsd: 0, outputPerMillionUsd: 0 },
      }),
    });
    expect(config.modelPrices["fake/fake-v1"]).toEqual({
      inputPerMillionUsd: 0,
      outputPerMillionUsd: 0,
    });
  });

  it("rejects non-finite model prices", () => {
    expect(() =>
      readServerConfig({
        AUTH_MODE: "development",
        MODEL_PRICES_JSON: '{"fake/fake-v1":{"inputPerMillionUsd":1e999,"outputPerMillionUsd":0}}',
      }),
    ).toThrow(/Invalid model price/);
  });

  it("requires deployment settings for Firebase authentication", () => {
    const base = {
      AUTH_MODE: "firebase",
      FIREBASE_PROJECT_ID: "synthetic-project",
      PROVIDER_MODE: "fake",
    };
    expect(() => readServerConfig(base)).toThrow(/DATABASE_URL/);
    expect(() => readServerConfig({ ...base, DATABASE_URL: "postgres://synthetic" })).toThrow(
      /WEB_ORIGIN/,
    );
    expect(
      readServerConfig({
        ...base,
        DATABASE_URL: "postgres://synthetic",
        WEB_ORIGIN: "https://app.example",
        PARISH_OWNER_FIREBASE_UID: "synthetic-owner",
      }).databaseUrl,
    ).toBe("postgres://synthetic");
  });

  it("keeps the fake model out of the live runtime allowlist", () => {
    const config = readServerConfig({
      AUTH_MODE: "development",
      PROVIDER_MODE: "live",
      OPENAI_API_KEY: "synthetic",
      GOOGLE_API_KEY: "synthetic",
      OPENAI_ALLOWED_MODELS: "model-a",
      MODEL_PRICES_JSON: JSON.stringify({
        "openai/model-a": { inputPerMillionUsd: 1, outputPerMillionUsd: 2 },
      }),
    });

    expect(config.allowedModels).toEqual(new Set(["openai/model-a"]));
  });

  it("keeps live models out of the fake runtime allowlist", () => {
    const config = readServerConfig({
      AUTH_MODE: "development",
      PROVIDER_MODE: "fake",
      OPENAI_ALLOWED_MODELS: "model-a",
      GOOGLE_ALLOWED_MODELS: "model-b",
    });

    expect(config.allowedModels).toEqual(new Set(["fake/fake-v1"]));
  });

  it("supports Google Vertex AI identity without an API key", () => {
    const config = readServerConfig({
      AUTH_MODE: "development",
      PROVIDER_MODE: "live",
      OPENAI_API_KEY: "synthetic",
      GOOGLE_PROVIDER_AUTH: "vertex-ai",
      GOOGLE_CLOUD_PROJECT: "synthetic-project",
      OPENAI_ALLOWED_MODELS: "model-a",
      GOOGLE_ALLOWED_MODELS: "model-b",
      MODEL_PRICES_JSON: JSON.stringify({
        "openai/model-a": { inputPerMillionUsd: 1, outputPerMillionUsd: 2 },
        "google/model-b": { inputPerMillionUsd: 0.3, outputPerMillionUsd: 2.5 },
      }),
    });
    expect(config.googleProviderAuth).toBe("vertex-ai");
    expect(config.googleCloudProject).toBe("synthetic-project");
    expect(config.googleCloudLocation).toBe("global");
    expect(config.googleApiKey).toBeUndefined();
  });

  it("fails closed when Vertex AI is selected without a project", () => {
    expect(() =>
      readServerConfig({
        AUTH_MODE: "development",
        PROVIDER_MODE: "live",
        OPENAI_API_KEY: "synthetic",
        GOOGLE_PROVIDER_AUTH: "vertex-ai",
      }),
    ).toThrow(/GOOGLE_CLOUD_PROJECT/);
  });
});
