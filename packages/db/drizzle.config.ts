import { defineConfig } from "drizzle-kit";

// Used by `drizzle-kit generate`, `check` and `export` (db:ci). Migrations are
// applied by src/cli/migrate.ts, never by drizzle-kit, the Worker or convt-server.
export default defineConfig({
  dialect: "postgresql",
  schema: "./src/schema/index.ts",
  out: "./migrations",
  casing: "snake_case",
  migrations: { schema: "drizzle", table: "__drizzle_migrations" },
});
