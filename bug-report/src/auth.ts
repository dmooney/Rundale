/** Verifies the two Firebase credentials the iPhone app already sends to
 * Limerick Endpoints: a Firebase ID token and an App Check token. Both are
 * mandatory, and App Check must name an allowed app. */

export interface FirebaseVerifier {
  verifyIdToken(token: string): Promise<{ uid: string }>;
  verifyAppCheckToken(token: string): Promise<{ appId: string }>;
}

export interface Reporter {
  uid: string;
  appId: string;
}

export interface CredentialHeaders {
  authorization?: string | undefined;
  "x-firebase-appcheck"?: string | string[] | undefined;
}

export class ReporterAuthenticator {
  constructor(
    private readonly verifier: FirebaseVerifier,
    private readonly allowedAppIds: readonly string[],
  ) {}

  /** The verified reporter, or `null` for any missing or invalid credential. */
  async authenticate(headers: CredentialHeaders): Promise<Reporter | null> {
    const authorization = headers.authorization;
    const appCheck = headers["x-firebase-appcheck"];
    if (
      typeof authorization !== "string" ||
      !authorization.startsWith("Bearer ") ||
      typeof appCheck !== "string" ||
      appCheck.length === 0
    )
      return null;
    const idToken = authorization.slice("Bearer ".length);
    if (idToken.length === 0 || idToken.includes(" ")) return null;
    try {
      const [identity, app] = await Promise.all([
        this.verifier.verifyIdToken(idToken),
        this.verifier.verifyAppCheckToken(appCheck),
      ]);
      if (!this.allowedAppIds.includes(app.appId)) return null;
      return { uid: identity.uid, appId: app.appId };
    } catch {
      return null;
    }
  }
}
