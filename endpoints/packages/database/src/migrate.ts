import { migrate } from "drizzle-orm/node-postgres/migrator";
import { createDatabase } from "./index.js";

const databaseUrl = process.env.DATABASE_URL;
if (databaseUrl === undefined) throw new Error("DATABASE_URL is required.");
const database = createDatabase(databaseUrl);
try {
  await migrate(database.db, {
    migrationsFolder: new URL("../migrations", import.meta.url).pathname,
  });
} finally {
  await database.close();
}
