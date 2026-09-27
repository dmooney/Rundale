import type { FastifyRequest } from "fastify";
import type { CreatorPrincipal } from "../control/contracts.js";

export interface CreatorIdentityRepository {
  findOwnerByExternalId(externalAuthId: string): Promise<CreatorPrincipal | null>;
}

export interface CreatorAuthenticator {
  authenticate(request: FastifyRequest): Promise<CreatorPrincipal | null>;
}

export interface FirebaseTokenVerifier {
  verifyIdToken(token: string, checkRevoked?: boolean): Promise<{ uid: string }>;
}

export class FirebaseCreatorAuthenticator implements CreatorAuthenticator {
  constructor(
    private readonly verifier: FirebaseTokenVerifier,
    private readonly ownerFirebaseUid: string,
    private readonly identities: CreatorIdentityRepository,
  ) {}

  async authenticate(request: FastifyRequest): Promise<CreatorPrincipal | null> {
    const authorization = request.headers.authorization;
    if (typeof authorization !== "string" || !authorization.startsWith("Bearer ")) return null;
    const token = authorization.slice("Bearer ".length);
    if (token.length === 0 || token.includes(" ")) return null;

    try {
      const decoded = await this.verifier.verifyIdToken(token, true);
      if (decoded.uid !== this.ownerFirebaseUid) return null;
      return this.identities.findOwnerByExternalId(decoded.uid);
    } catch {
      return null;
    }
  }
}

export class DevelopmentCreatorAuthenticator implements CreatorAuthenticator {
  constructor(private readonly identities: CreatorIdentityRepository) {}

  async authenticate(request: FastifyRequest): Promise<CreatorPrincipal | null> {
    const externalId = request.headers["x-parish-owner-id"];
    if (typeof externalId !== "string" || externalId.length === 0) return null;
    return this.identities.findOwnerByExternalId(externalId);
  }
}
