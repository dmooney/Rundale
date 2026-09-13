import type { FastifyRequest } from "fastify";
import { describe, expect, it, vi } from "vitest";
import {
  FirebaseCreatorAuthenticator,
  type CreatorIdentityRepository,
  type FirebaseTokenVerifier,
} from "../src/auth/creator-auth.js";

const principal = { userId: "user-1", organizationId: "org-1", role: "owner" as const };

function request(authorization?: string): FastifyRequest {
  return { headers: authorization === undefined ? {} : { authorization } } as FastifyRequest;
}

describe("Firebase creator authentication", () => {
  it("accepts only a valid, non-revoked token for the configured owner UID", async () => {
    const verifyIdToken = vi.fn(async () => ({ uid: "firebase-owner" }));
    const findOwnerByExternalId = vi.fn(async () => principal);
    const authenticator = new FirebaseCreatorAuthenticator(
      { verifyIdToken } satisfies FirebaseTokenVerifier,
      "firebase-owner",
      { findOwnerByExternalId } satisfies CreatorIdentityRepository,
    );

    await expect(authenticator.authenticate(request("Bearer valid-token"))).resolves.toEqual(
      principal,
    );
    expect(verifyIdToken).toHaveBeenCalledWith("valid-token", true);
    expect(findOwnerByExternalId).toHaveBeenCalledWith("firebase-owner");
  });

  it("rejects missing, invalid, and non-owner credentials", async () => {
    const verifyIdToken = vi
      .fn()
      .mockResolvedValueOnce({ uid: "someone-else" })
      .mockRejectedValueOnce(new Error("expired"));
    const findOwnerByExternalId = vi.fn(async () => principal);
    const authenticator = new FirebaseCreatorAuthenticator(
      { verifyIdToken } as FirebaseTokenVerifier,
      "firebase-owner",
      { findOwnerByExternalId },
    );

    await expect(authenticator.authenticate(request())).resolves.toBeNull();
    await expect(authenticator.authenticate(request("Basic value"))).resolves.toBeNull();
    await expect(authenticator.authenticate(request("Bearer other-token"))).resolves.toBeNull();
    await expect(authenticator.authenticate(request("Bearer expired-token"))).resolves.toBeNull();
    expect(findOwnerByExternalId).not.toHaveBeenCalled();
  });
});
