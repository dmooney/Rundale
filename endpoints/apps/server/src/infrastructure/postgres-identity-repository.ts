import { and, eq } from "drizzle-orm";
import { organizationMembers, users, type Database } from "@parish/database";
import type { CreatorIdentityRepository } from "../auth/creator-auth.js";
import type { CreatorPrincipal } from "../control/contracts.js";

export class PostgresCreatorIdentityRepository implements CreatorIdentityRepository {
  constructor(private readonly database: Database) {}

  async findOwnerByExternalId(externalAuthId: string): Promise<CreatorPrincipal | null> {
    const [result] = await this.database
      .select({ userId: users.id, organizationId: organizationMembers.organizationId })
      .from(users)
      .innerJoin(
        organizationMembers,
        and(eq(organizationMembers.userId, users.id), eq(organizationMembers.role, "owner")),
      )
      .where(eq(users.externalAuthId, externalAuthId))
      .limit(1);
    return result === undefined ? null : { ...result, role: "owner" };
  }
}
