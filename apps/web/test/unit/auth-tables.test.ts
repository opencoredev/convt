// Better Auth's adapter must find every field it writes in packages/db's schema. An
// upgrade that adds a column fails here instead of at sign-in.

import { expect, test } from "bun:test";

import * as schema from "@convt/db/schema";
import { diffSchema, getExpectedSchema } from "@better-auth/core/db/internal";
import { getTableColumns, is, Table } from "drizzle-orm";

import { authOptions } from "../../src/server/auth";
import { readEnv } from "../../src/server/env";

test("the Drizzle schema has every table and column Better Auth uses", () => {
  const env = readEnv({
    ENV: "test",
    BETTER_AUTH_URL: "http://localhost:3000",
    BETTER_AUTH_SECRET: "x".repeat(64),
    MAIL_TRANSPORT: "log",
    OAUTH_MOCK_URL: "http://127.0.0.1:4100",
  });
  const options = authOptions({ db: {} as never, background: () => {} }, env, {
    startCookies: false,
  });
  const tables = Object.entries(schema)
    .filter(([, t]) => is(t, Table))
    .map(([name, t]) => ({
      name,
      columns: Object.entries(getTableColumns(t as Table)).map(([key, c]) => ({
        name: key,
        nullable: !c.notNull,
        hasDefault: c.hasDefault,
      })),
    }));
  const findings = diffSchema(getExpectedSchema(options, { usePlural: true }), tables);
  expect(findings).toEqual([]);
});
