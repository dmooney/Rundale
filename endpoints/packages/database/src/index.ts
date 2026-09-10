import { drizzle } from "drizzle-orm/node-postgres";
import { sql } from "drizzle-orm";
import { Pool } from "pg";
import * as schema from "./schema.js";

export type Database = ReturnType<typeof createDatabase>["db"];

export function createDatabase(databaseUrl: string) {
  const pool = new Pool({ connectionString: databaseUrl, max: 10 });
  const db = drizzle(pool, { schema });
  return {
    db,
    pool,
    ping: async () => void (await db.execute(sql`select 1`)),
    close: () => pool.end(),
  };
}

export * from "./schema.js";
