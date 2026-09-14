import type { MobileAppBinding } from "./auth/mobile-auth.js";

export interface ServerConfig {
  port: number;
  host: string;
  databaseUrl: string;
  webOrigin: string;
  maxRequestBytes: number;
  maxImageBytes: number;
  maxImagePixels: number;
  requestsPerMinute: number;
  requestsPerDay: number;
  globalInferenceEnabled: boolean;
  providerMode: "fake" | "live";
  authMode: "development" | "firebase";
  firebaseProjectId?: string;
  ownerFirebaseUid: string;
  allowedModels: ReadonlySet<string>;
  providerTimeoutMs: number;
  openaiApiKey?: string;
  googleApiKey?: string;
  googleProviderAuth: "api-key" | "vertex-ai";
  googleCloudProject?: string;
  googleCloudLocation?: string;
  modelPrices: Record<string, { inputPerMillionUsd: number; outputPerMillionUsd: number }>;
  mobileAppBindings: readonly MobileAppBinding[];
}

function positiveInteger(value: string | undefined, fallback: number, name: string): number {
  const parsed = Number(value ?? fallback);
  if (!Number.isSafeInteger(parsed) || parsed <= 0)
    throw new Error(`${name} must be a positive integer.`);
  return parsed;
}

function requiredEnvironmentValue(
  environment: NodeJS.ProcessEnv,
  name: string,
  reason: string,
): string {
  const value = environment[name]?.trim();
  if (value === undefined || value.length === 0) {
    throw new Error(`${name} is required ${reason}.`);
  }
  return value;
}

export function readServerConfig(environment: NodeJS.ProcessEnv = process.env): ServerConfig {
  const providerMode = environment.PROVIDER_MODE ?? "fake";
  if (providerMode !== "fake" && providerMode !== "live") {
    throw new Error("PROVIDER_MODE must be 'fake' or 'live'.");
  }
  const googleProviderAuth = environment.GOOGLE_PROVIDER_AUTH ?? "api-key";
  if (googleProviderAuth !== "api-key" && googleProviderAuth !== "vertex-ai") {
    throw new Error("GOOGLE_PROVIDER_AUTH must be 'api-key' or 'vertex-ai'.");
  }
  if (providerMode === "live" && !environment.OPENAI_API_KEY) {
    throw new Error("OPENAI_API_KEY is required when PROVIDER_MODE=live.");
  }
  if (providerMode === "live" && googleProviderAuth === "api-key" && !environment.GOOGLE_API_KEY) {
    throw new Error("GOOGLE_API_KEY is required when Google provider auth uses an API key.");
  }
  if (
    providerMode === "live" &&
    googleProviderAuth === "vertex-ai" &&
    !environment.GOOGLE_CLOUD_PROJECT
  ) {
    throw new Error("GOOGLE_CLOUD_PROJECT is required when Google provider auth uses Vertex AI.");
  }
  const isProduction = environment.NODE_ENV === "production";
  const isCloudRun = (environment.K_SERVICE?.trim().length ?? 0) > 0;
  const authMode = environment.AUTH_MODE ?? "firebase";
  if (authMode !== "development" && authMode !== "firebase") {
    throw new Error("AUTH_MODE is invalid.");
  }
  if (isProduction && authMode !== "firebase") {
    throw new Error("Production requires AUTH_MODE=firebase.");
  }
  if (isCloudRun && authMode !== "firebase") {
    throw new Error("Cloud Run requires AUTH_MODE=firebase.");
  }
  if (isProduction && providerMode !== "live") {
    throw new Error("Production requires PROVIDER_MODE=live.");
  }
  const firebaseProjectId =
    authMode === "firebase"
      ? requiredEnvironmentValue(environment, "FIREBASE_PROJECT_ID", "when AUTH_MODE=firebase")
      : environment.FIREBASE_PROJECT_ID?.trim();
  const requiresDeploymentSettings = authMode === "firebase" || isProduction || isCloudRun;
  const mobileBindingsText = environment.MOBILE_APP_BINDINGS_JSON?.trim();
  const mobileAppBindings: MobileAppBinding[] = [];
  if (mobileBindingsText !== undefined && mobileBindingsText.length > 0) {
    const parsed: unknown = JSON.parse(mobileBindingsText);
    if (!Array.isArray(parsed)) throw new Error("MOBILE_APP_BINDINGS_JSON must be an array.");
    for (const item of parsed) {
      if (item === null || typeof item !== "object" || Array.isArray(item)) {
        throw new Error("Invalid mobile app binding.");
      }
      const value = item as Record<string, unknown>;
      const allowedKeys = new Set([
        "appId",
        "organizationId",
        "organizationSlug",
        "endpointVersions",
        "dailyInvocationQuota",
      ]);
      if (Object.keys(value).some((key) => !allowedKeys.has(key))) {
        throw new Error("Invalid mobile app binding.");
      }
      const strings = ["appId", "organizationId", "organizationSlug"];
      if (
        strings.some(
          (key) => typeof value[key] !== "string" || (value[key] as string).trim() === "",
        )
      ) {
        throw new Error("Invalid mobile app binding.");
      }
      if (
        value.endpointVersions === null ||
        typeof value.endpointVersions !== "object" ||
        Array.isArray(value.endpointVersions)
      ) {
        throw new Error("Invalid mobile app binding endpoint versions.");
      }
      const endpointVersions = value.endpointVersions as Record<string, unknown>;
      if (
        Object.keys(endpointVersions).length === 0 ||
        Object.entries(endpointVersions).some(
          ([slug, versions]) =>
            !/^[a-z0-9]+(?:-[a-z0-9]+)*$/.test(slug) ||
            !Array.isArray(versions) ||
            versions.length === 0 ||
            versions.some((version) => !Number.isSafeInteger(version) || (version as number) < 1) ||
            new Set(versions).size !== versions.length,
        )
      ) {
        throw new Error("Invalid mobile app binding endpoint versions.");
      }
      if (!/^[a-z0-9]+(?:-[a-z0-9]+)*$/.test(value.organizationSlug as string)) {
        throw new Error("Invalid mobile app binding organization slug.");
      }
      if (
        isProduction &&
        !/^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/.test(
          value.organizationId as string,
        )
      ) {
        throw new Error("Production mobile organizationId must be a UUID.");
      }
      if (mobileAppBindings.some((binding) => binding.appId === value.appId)) {
        throw new Error("Mobile app IDs must be unique.");
      }
      const quota = value.dailyInvocationQuota;
      if (!Number.isSafeInteger(quota) || (quota as number) < 1 || (quota as number) > 10_000_000) {
        throw new Error("Invalid mobile app binding quota.");
      }
      mobileAppBindings.push({
        appId: value.appId as string,
        organizationId: value.organizationId as string,
        organizationSlug: value.organizationSlug as string,
        endpointVersions: endpointVersions as Record<string, number[]>,
        dailyInvocationQuota: quota as number,
      });
    }
  } else if (isProduction || isCloudRun) {
    throw new Error("MOBILE_APP_BINDINGS_JSON is required for production mobile authentication.");
  }
  const databaseUrl = requiresDeploymentSettings
    ? requiredEnvironmentValue(environment, "DATABASE_URL", "for Firebase or production deployment")
    : (environment.DATABASE_URL ?? "postgres://parish:parish@localhost:5432/parish");
  const webOrigin = requiresDeploymentSettings
    ? requiredEnvironmentValue(environment, "WEB_ORIGIN", "for Firebase or production deployment")
    : (environment.WEB_ORIGIN ?? "http://localhost:3000");
  const ownerFirebaseUid = requiresDeploymentSettings
    ? requiredEnvironmentValue(
        environment,
        "PARISH_OWNER_FIREBASE_UID",
        "for Firebase or production deployment",
      )
    : (environment.PARISH_OWNER_FIREBASE_UID ?? "user_synthetic_owner");
  const configuredLiveModels = [
    ...(environment.OPENAI_ALLOWED_MODELS ?? "")
      .split(",")
      .map((model) => model.trim())
      .filter(Boolean)
      .map((model) => `openai/${model}`),
    ...(environment.GOOGLE_ALLOWED_MODELS ?? "")
      .split(",")
      .map((model) => model.trim())
      .filter(Boolean)
      .map((model) => `google/${model}`),
  ];
  const allowedModels = new Set(providerMode === "fake" ? ["fake/fake-v1"] : configuredLiveModels);
  const modelPrices: ServerConfig["modelPrices"] = {};
  if (environment.MODEL_PRICES_JSON !== undefined) {
    const parsed: unknown = JSON.parse(environment.MODEL_PRICES_JSON);
    if (parsed === null || typeof parsed !== "object" || Array.isArray(parsed)) {
      throw new Error("MODEL_PRICES_JSON must be an object.");
    }
    for (const [key, value] of Object.entries(parsed)) {
      if (
        value === null ||
        typeof value !== "object" ||
        !("inputPerMillionUsd" in value) ||
        !("outputPerMillionUsd" in value) ||
        typeof value.inputPerMillionUsd !== "number" ||
        typeof value.outputPerMillionUsd !== "number" ||
        !Number.isFinite(value.inputPerMillionUsd) ||
        !Number.isFinite(value.outputPerMillionUsd) ||
        value.inputPerMillionUsd < 0 ||
        value.outputPerMillionUsd < 0
      ) {
        throw new Error(`Invalid model price for '${key}'.`);
      }
      modelPrices[key] = {
        inputPerMillionUsd: value.inputPerMillionUsd,
        outputPerMillionUsd: value.outputPerMillionUsd,
      };
    }
  }
  if (
    providerMode === "live" &&
    [...allowedModels].some((model) => model !== "fake/fake-v1" && modelPrices[model] === undefined)
  ) {
    throw new Error("MODEL_PRICES_JSON must price every allowed live model.");
  }
  return {
    port: positiveInteger(environment.PORT, 3001, "PORT"),
    host: environment.HOST ?? "0.0.0.0",
    databaseUrl,
    webOrigin,
    maxRequestBytes: positiveInteger(
      environment.MAX_REQUEST_BYTES,
      10_485_760,
      "MAX_REQUEST_BYTES",
    ),
    maxImageBytes: positiveInteger(environment.MAX_IMAGE_BYTES, 8_388_608, "MAX_IMAGE_BYTES"),
    maxImagePixels: positiveInteger(environment.MAX_IMAGE_PIXELS, 40_000_000, "MAX_IMAGE_PIXELS"),
    requestsPerMinute: positiveInteger(environment.REQUESTS_PER_MINUTE, 60, "REQUESTS_PER_MINUTE"),
    requestsPerDay: positiveInteger(environment.REQUESTS_PER_DAY, 1000, "REQUESTS_PER_DAY"),
    globalInferenceEnabled: environment.GLOBAL_INFERENCE_ENABLED !== "false",
    providerMode,
    authMode,
    ...(firebaseProjectId === undefined ? {} : { firebaseProjectId }),
    ownerFirebaseUid,
    allowedModels,
    modelPrices,
    providerTimeoutMs: positiveInteger(
      environment.PROVIDER_TIMEOUT_MS,
      60_000,
      "PROVIDER_TIMEOUT_MS",
    ),
    ...(environment.OPENAI_API_KEY === undefined
      ? {}
      : { openaiApiKey: environment.OPENAI_API_KEY }),
    ...(environment.GOOGLE_API_KEY === undefined
      ? {}
      : { googleApiKey: environment.GOOGLE_API_KEY }),
    googleProviderAuth,
    ...(environment.GOOGLE_CLOUD_PROJECT === undefined
      ? {}
      : { googleCloudProject: environment.GOOGLE_CLOUD_PROJECT }),
    ...(googleProviderAuth !== "vertex-ai"
      ? {}
      : { googleCloudLocation: environment.GOOGLE_CLOUD_LOCATION ?? "global" }),
    mobileAppBindings,
  };
}
