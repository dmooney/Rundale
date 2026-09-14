import type { FastifyRequest } from "fastify";

export interface MobileCredentialRequest {
  headers: {
    authorization?: string | undefined;
    "x-firebase-appcheck"?: string | undefined;
  };
}

export interface MobileAppBinding {
  appId: string;
  organizationId: string;
  organizationSlug: string;
  endpointVersions: Readonly<Record<string, readonly number[]>>;
  dailyInvocationQuota: number;
}

export interface MobileIdentityVerifier {
  verifyIdToken(token: string): Promise<{ uid: string }>;
  verifyAppCheckToken(token: string): Promise<{ appId: string }>;
}

export interface MobilePrincipal {
  kind: "mobile";
  uid: string;
  appId: string;
  organizationId: string;
  organizationSlug: string;
  dailyInvocationQuota: number;
  rateIdentity: string;
  allowedEndpointVersions: Readonly<Record<string, readonly number[]>>;
}

/** Verifies both Firebase credentials. App Check is deliberately mandatory. */
export class FirebaseMobileAuthenticator {
  constructor(
    private readonly verifier: MobileIdentityVerifier,
    private readonly bindings: readonly MobileAppBinding[],
  ) {}

  async authenticate(
    request: MobileCredentialRequest | FastifyRequest,
  ): Promise<MobilePrincipal | null> {
    const authorization = request.headers.authorization;
    const appCheck = request.headers["x-firebase-appcheck"];
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
      const binding = this.bindings.find((item) => item.appId === app.appId);
      if (binding === undefined) return null;
      return {
        kind: "mobile",
        uid: identity.uid,
        appId: app.appId,
        organizationId: binding.organizationId,
        organizationSlug: binding.organizationSlug,
        dailyInvocationQuota: binding.dailyInvocationQuota,
        allowedEndpointVersions: binding.endpointVersions,
        rateIdentity: `mobile:${app.appId}:${identity.uid}`,
      };
    } catch {
      return null;
    }
  }
}
