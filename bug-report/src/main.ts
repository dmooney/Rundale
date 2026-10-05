import { Storage } from "@google-cloud/storage";
import { applicationDefault, initializeApp } from "firebase-admin/app";
import { getAppCheck } from "firebase-admin/app-check";
import { getAuth } from "firebase-admin/auth";
import { buildApp } from "./app.js";
import { ReporterAuthenticator } from "./auth.js";
import { BucketReportStore } from "./store.js";

function required(name: string): string {
  const value = process.env[name]?.trim();
  if (!value) throw new Error(`${name} is required`);
  return value;
}

const firebase = initializeApp({
  credential: applicationDefault(),
  projectId: required("FIREBASE_PROJECT_ID"),
});
const authenticator = new ReporterAuthenticator(
  {
    verifyIdToken: (token) => getAuth(firebase).verifyIdToken(token, true),
    verifyAppCheckToken: async (token) => {
      const decoded = await getAppCheck(firebase).verifyToken(token);
      return { appId: decoded.appId };
    },
  },
  required("ALLOWED_APP_IDS")
    .split(",")
    .map((id) => id.trim())
    .filter((id) => id.length > 0),
);
const app = buildApp({
  authenticator,
  store: new BucketReportStore(new Storage().bucket(required("REPORT_BUCKET"))),
  hourlyLimit: Number(process.env.HOURLY_REPORT_LIMIT ?? "20"),
  logger: true,
});

await app.listen({ host: "0.0.0.0", port: Number(process.env.PORT ?? "8080") });
