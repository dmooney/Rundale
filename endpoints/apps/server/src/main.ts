import { createDatabase } from "@parish/database";
import { applicationDefault, initializeApp } from "firebase-admin/app";
import { getAuth } from "firebase-admin/auth";
import { buildServer } from "./app.js";
import { readServerConfig } from "./config.js";
import {
  DevelopmentCreatorAuthenticator,
  FirebaseCreatorAuthenticator,
} from "./auth/creator-auth.js";
import { ControlService } from "./control/service.js";
import { PlaygroundService } from "./control/playground-service.js";
import { PostgresControlRepository } from "./infrastructure/postgres-control-repository.js";
import { PostgresCreatorIdentityRepository } from "./infrastructure/postgres-identity-repository.js";
import { PostgresInvocationRepository } from "./infrastructure/postgres-invocation-repository.js";
import {
  FakeProvider,
  FixedPriceCostCalculator,
  GoogleProvider,
  OpenAIProvider,
  StaticProviderRegistry,
} from "@parish/providers";
import { DeterministicRuntime, type ModelProvider } from "@parish/runtime";
import { InvocationService } from "./invocation/service.js";

const config = readServerConfig();
const database = createDatabase(config.databaseUrl);
const identities = new PostgresCreatorIdentityRepository(database.db);
const authenticator =
  config.authMode === "firebase"
    ? new FirebaseCreatorAuthenticator(
        getAuth(
          initializeApp({
            credential: applicationDefault(),
            projectId: config.firebaseProjectId!,
          }),
        ),
        config.ownerFirebaseUid,
        identities,
      )
    : new DevelopmentCreatorAuthenticator(identities);
const controlService = new ControlService(new PostgresControlRepository(database.db), {
  allowedModels: config.allowedModels,
});
const providers: ModelProvider[] =
  config.providerMode === "live"
    ? [
        OpenAIProvider.fromApiKey(config.openaiApiKey!),
        config.googleProviderAuth === "vertex-ai"
          ? GoogleProvider.fromVertexAI(config.googleCloudProject!, config.googleCloudLocation!)
          : GoogleProvider.fromApiKey(config.googleApiKey!),
      ]
    : [new FakeProvider()];
const providerRegistry = new StaticProviderRegistry(providers);
const runtime = new DeterministicRuntime(
  providerRegistry,
  new FixedPriceCostCalculator(config.modelPrices),
  { allowedModels: config.allowedModels },
);
const invocationService = new InvocationService(
  new PostgresInvocationRepository(database.db),
  runtime,
  {
    globalInferenceEnabled: config.globalInferenceEnabled,
    requestsPerMinute: config.requestsPerMinute,
    timeoutMs: config.providerTimeoutMs,
    requestsPerDay: config.requestsPerDay,
  },
);
const playgroundService = new PlaygroundService(
  new PostgresControlRepository(database.db),
  new PostgresInvocationRepository(database.db),
  runtime,
  {
    globalInferenceEnabled: config.globalInferenceEnabled,
    timeoutMs: config.providerTimeoutMs,
    requestsPerDay: config.requestsPerDay,
  },
);
const server = await buildServer(config, {
  health: { ready: database.ping },
  control: { service: controlService, playground: playgroundService, authenticator },
  invocation: { service: invocationService },
});

const shutdown = async () => {
  await server.close();
  await database.close();
};
process.once("SIGINT", () => void shutdown());
process.once("SIGTERM", () => void shutdown());

await server.listen({ port: config.port, host: config.host });
