// Test helpers for the disposable Postgres that scripts/test-db.sh starts. Each
// caller gets its own database copied from the migrated template, and URLs for
// each role. Tests connect as convt_web, convt_billing or convt_server, like the
// apps do.

import pg from "pg";

import { connect, type Db } from "./client";

export type TestDatabase = {
  name: string;
  ownerUrl: string;
  webUrl: string;
  serverUrl: string;
  billingUrl: string;
  /** Connections opened through `open` are closed by `drop`. */
  open: (role: "owner" | "web" | "server" | "billing") => Promise<{ client: pg.Client; db: Db }>;
  drop: () => Promise<void>;
};

function env(name: string): string {
  const value = process.env[name];
  if (!value)
    throw new Error(`${name} is not set; run tests through packages/db/scripts/test-db.sh`);
  return value;
}

export function hasTestDatabase(): boolean {
  return Boolean(process.env.TEST_PG_ADMIN_URL);
}

export async function freshDatabase(): Promise<TestDatabase> {
  const host = env("TEST_PG_HOST");
  const port = env("TEST_PG_PORT");
  const name = `t_${crypto.randomUUID().replace(/-/g, "").slice(0, 16)}`;
  const admin = new pg.Client({ connectionString: env("TEST_PG_ADMIN_URL") });
  await admin.connect();
  try {
    await admin.query(
      `create database ${name} template ${env("TEST_TEMPLATE_DB")} owner convt_owner`,
    );
  } finally {
    await admin.end();
  }
  const url = (role: string, pw: string) => `postgresql://${role}:${pw}@${host}:${port}/${name}`;
  const urls = {
    owner: url("convt_owner", env("TEST_OWNER_PASSWORD")),
    web: url("convt_web", env("TEST_WEB_PASSWORD")),
    server: url("convt_server", env("TEST_SERVER_PASSWORD")),
    billing: url("convt_billing", env("TEST_BILLING_PASSWORD")),
  };
  const opened: pg.Client[] = [];
  return {
    name,
    ownerUrl: urls.owner,
    webUrl: urls.web,
    serverUrl: urls.server,
    billingUrl: urls.billing,
    async open(role) {
      const c = await connect(urls[role]);
      opened.push(c.client);
      return c;
    },
    async drop() {
      await Promise.allSettled(opened.map((c) => c.end()));
      const a = new pg.Client({ connectionString: env("TEST_PG_ADMIN_URL") });
      await a.connect();
      try {
        await a.query(`drop database if exists ${name} with (force)`);
      } finally {
        await a.end();
      }
    },
  };
}
