import { afterAll, beforeAll, expect, test } from "bun:test";
import { sql } from "drizzle-orm";

import { createHarness, type Harness } from "../../src/testing";

let h: Harness;
beforeAll(async () => {
  h = await createHarness();
});
afterAll(async () => h?.close());

test("billing cleans expired auth rows in bounded batches, retaining live rows and a day of rates", async () => {
  const now = h.mock.now();
  const expired = new Date(now.getTime() - 1);
  const live = new Date(now.getTime() + 60000);
  await h.owner.execute(sql`
    insert into verifications (id, identifier, value, expires_at)
    select 'marker_' || n, 'otp-issued:' || n, 'x', ${expired} from generate_series(1,1001) n`);
  await h.owner.execute(sql`
    insert into verifications (id, identifier, value, expires_at) values
    ('old_code', 'sign-in:old', 'x', ${expired}), ('live_code', 'sign-in:live', 'x', ${live})`);
  await h.owner.execute(sql`
    insert into rate_limits (id, key, count, last_request) values
    ('old_rate', 'old', 1, ${now.getTime() - 90000000}),
    ('live_rate', 'live', 1, ${now.getTime() - 82800000})`);
  await h.owner.execute(sql`
    insert into otp_send_limits (key, window_start, count, expires_at) values
    ('old', ${expired}, 1, ${expired}), ('live', ${now}, 1, ${live})`);
  expect(await h.service.cleanupAuth()).toEqual({
    verifications: 1001,
    rateLimits: 1,
    otpSendLimits: 1,
  });
  expect(await h.service.cleanupAuth()).toEqual({
    verifications: 1,
    rateLimits: 0,
    otpSendLimits: 0,
  });
  expect(await h.service.cleanupAuth()).toEqual({
    verifications: 0,
    rateLimits: 0,
    otpSendLimits: 0,
  });
  expect(await h.q(sql`select id from verifications`)).toEqual([{ id: "live_code" }]);
  expect(await h.q(sql`select id from rate_limits`)).toEqual([{ id: "live_rate" }]);
  expect(await h.q(sql`select key from otp_send_limits`)).toEqual([{ key: "live" }]);
});

test("cleanup retains rows refreshed while it waits for their transaction", async () => {
  const lock = await h.tdb.open("owner");
  const cases = [
    {
      table: "verifications",
      id: "refresh-marker",
      insert: sql`insert into verifications (id, identifier, value, expires_at) values ('refresh-marker', 'otp-issued:refresh', 'x', now() - interval '1 hour')`,
      refresh: sql`update verifications set expires_at = now() + interval '1 hour' where id = 'refresh-marker'`,
    },
    {
      table: "verifications",
      id: "refresh-code",
      insert: sql`insert into verifications (id, identifier, value, expires_at) values ('refresh-code', 'sign-in:refresh', 'x', now() - interval '1 hour')`,
      refresh: sql`update verifications set expires_at = now() + interval '1 hour' where id = 'refresh-code'`,
    },
    {
      table: "rate_limits",
      id: "refresh-rate",
      insert: sql`insert into rate_limits (id, key, count, last_request) values ('refresh-rate', 'refresh', 1, 0)`,
      refresh: sql`update rate_limits set last_request = ${Date.now()} where id = 'refresh-rate'`,
    },
    {
      table: "otp_send_limits",
      id: "refresh-send",
      insert: sql`insert into otp_send_limits (key, window_start, count, expires_at) values ('refresh-send', now(), 1, now() - interval '1 hour')`,
      refresh: sql`update otp_send_limits set expires_at = now() + interval '1 hour' where key = 'refresh-send'`,
    },
  ];
  for (const c of cases) {
    await h.owner.execute(c.insert);
    await lock.client.query("begin");
    let pending: ReturnType<typeof h.service.cleanupAuth> | undefined;
    try {
      await lock.db.execute(c.refresh);
      pending = h.service.cleanupAuth();
      const deadline = Date.now() + 5000;
      let waiting = false;
      while (Date.now() < deadline) {
        const [r] = await h.q<{ waiting: boolean }>(sql`
          select exists (
            select 1 from pg_locks l join pg_stat_activity a on a.pid = l.pid
            where a.datname = current_database() and not l.granted
          ) as waiting`);
        if (r.waiting) {
          waiting = true;
          break;
        }
        await Bun.sleep(10);
      }
      expect(waiting).toBe(true);
      await lock.client.query("commit");
      await pending;
      const id = c.table === "otp_send_limits" ? "key" : "id";
      expect(
        await h.q(
          sql`select 1 from ${sql.identifier(c.table)} where ${sql.identifier(id)} = ${c.id}`,
        ),
      ).toHaveLength(1);
    } finally {
      await lock.client.query("rollback");
      await pending;
    }
  }
}, 30000);
