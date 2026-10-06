import { drizzle, type NodePgDatabase } from "drizzle-orm/node-postgres";
import pg from "pg";

import * as schema from "./schema";

export type Db = NodePgDatabase<typeof schema>;

/** A Drizzle client over one `pg` connection. Callers own `client.end()`. */
export function createDb(client: pg.Client | pg.PoolClient): Db {
  return drizzle(client, { schema, casing: "snake_case" });
}

export async function connect(url: string): Promise<{ client: pg.Client; db: Db }> {
  const client = new pg.Client({ connectionString: url });
  await client.connect();
  return { client, db: createDb(client) };
}

export async function withDb<T>(
  url: string,
  fn: (db: Db, client: pg.Client) => Promise<T>,
): Promise<T> {
  const { client, db } = await connect(url);
  try {
    return await fn(db, client);
  } finally {
    await client.end();
  }
}
