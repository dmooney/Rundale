import { getApps, initializeApp } from "firebase/app";
import { getAuth } from "firebase/auth";

export function firebaseIsConfigured(): boolean {
  return [
    process.env.NEXT_PUBLIC_FIREBASE_API_KEY,
    process.env.NEXT_PUBLIC_FIREBASE_AUTH_DOMAIN,
    process.env.NEXT_PUBLIC_FIREBASE_PROJECT_ID,
    process.env.NEXT_PUBLIC_FIREBASE_APP_ID,
  ].every((value) => value !== undefined && value.length > 0);
}

export function getFirebaseAuth() {
  if (!firebaseIsConfigured()) throw new Error("Firebase client configuration is incomplete.");
  const apiKey = process.env.NEXT_PUBLIC_FIREBASE_API_KEY!;
  const authDomain = process.env.NEXT_PUBLIC_FIREBASE_AUTH_DOMAIN!;
  const projectId = process.env.NEXT_PUBLIC_FIREBASE_PROJECT_ID!;
  const appId = process.env.NEXT_PUBLIC_FIREBASE_APP_ID!;
  const app =
    getApps()[0] ??
    initializeApp({
      apiKey,
      authDomain,
      projectId,
      appId,
    });
  return getAuth(app);
}
