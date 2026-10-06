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
    ('old_rate', 'old', 1, ${now.getTime() - 86400001}),
    ('live_rate', 'live', 1, ${now.getTime() - 86400000})`);
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
