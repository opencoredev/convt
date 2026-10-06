// Local-only destructive fixtures. Financial facts are retained for audit.
import { createHash, randomBytes } from "node:crypto";
import pg from "pg";
import { localFetch, localUrl as local, meterMatches, type Receipt } from "./safety";
import { newId } from "@convt/license";
import { assertOwnedDatabase } from "../../../packages/db/src/guard";
import { deliveryHeaders } from "../../billing-mock/src/sign";

const envFile = async (path: string) =>
  Object.fromEntries(
    (await Bun.file(path).text()).split("\n").flatMap((l) => {
      const m = /^([A-Z_]+)=(.*)$/.exec(l);
      return m ? [[m[1], m[2]]] : [];
    }),
  );
const web = local(process.env.WEB_URL ?? "");
const api = local(process.env.API_URL ?? "");
const services = await envFile(".convt-dev/services.env");
const billing = await envFile("apps/billing/.dev.vars");
const dbUrl = process.env.LOAD_DATABASE_URL;
if (!dbUrl || process.env.CONVT_LOAD_LOCAL !== "1")
  throw Error("Set CONVT_LOAD_LOCAL=1 and LOAD_DATABASE_URL to this checkout's owner URL");
local(dbUrl);
if (dbUrl !== services.OWNER_DATABASE_URL)
  throw Error("LOAD_DATABASE_URL must match this checkout's guarded owner URL");
const concurrency = Number(process.env.LOAD_CONCURRENCY ?? 25);
if (!Number.isInteger(concurrency) || concurrency < 20 || concurrency > 50)
  throw Error("LOAD_CONCURRENCY must be 20..50");
assertOwnedDatabase(dbUrl);
const db = new pg.Client({
  connectionString: dbUrl,
  connectionTimeoutMillis: 30_000,
  query_timeout: 30_000,
});
await db.connect();
const testIp = `2001:db8:${randomBytes(6).toString("hex").match(/.{4}/g)!.join(":")}::1`;
const run = `load_${Date.now()}_${randomBytes(4).toString("hex")}`;
const users: string[] = [];
const failures: string[] = [];
const samples: Record<string, { ms: number; status: number; unexpected: boolean }[]> = {};
const check = (ok: boolean, detail: string) => {
  if (!ok) failures.push(detail);
};
async function request(name: string, url: string, init: RequestInit = {}, expected = [200]) {
  const begin = performance.now();
  try {
    const res = await localFetch(url, {
      ...init,
    });
    const bytes = new Uint8Array(await res.arrayBuffer());
    (samples[name] ??= []).push({
      ms: performance.now() - begin,
      status: res.status,
      unexpected: !expected.includes(res.status),
    });
    if (!expected.includes(res.status))
      failures.push(
        `${name}: HTTP ${res.status}: ${new TextDecoder().decode(bytes).slice(0, 180)}`,
      );
    return { status: res.status, bytes, json: () => JSON.parse(new TextDecoder().decode(bytes)) };
  } catch (e) {
    (samples[name] ??= []).push({ ms: performance.now() - begin, status: 0, unexpected: true });
    failures.push(`${name}: ${String(e)}`);
    throw e;
  }
}
const batch = async (n: number, task: (i: number) => Promise<unknown>) => {
  let next = 0;
  await Promise.all(
    Array.from({ length: Math.min(n, concurrency) }, async () => {
      while (next < n) {
        const i = next++;
        try {
          await task(i);
        } catch (e) {
          failures.push(String(e));
        }
      }
    }),
  );
};
const json = (body: unknown, key?: string): RequestInit => ({
  method: "POST",
  headers: {
    "content-type": "application/json",
    ...(key ? { authorization: `Bearer ${key}` } : {}),
    origin: web,
    "cf-connecting-ip": testIp,
  },
  body: JSON.stringify(body),
});
const svg =
  '<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32"><rect width="32" height="32" fill="red"/></svg>';
const data = { input_format: "svg", target_format: "png", input_bytes: Buffer.byteLength(svg) };
async function fixture(cap: number) {
  const id = newId("usr");
  const keyId = newId("key");
  const sub = newId("sub");
  const email = `${run}-${users.length}@convt.test`;
  users.push(id);
  const key = `cvt_live_${randomBytes(16).toString("hex")}`;
  await db.query(
    "insert into users(id,name,email,email_verified) values($1,'Load fixture',$2,true)",
    [id, email],
  );
  await db.query(
    "insert into subscriptions(id,provider_subscription_id,user_id,email,kind,status,spend_cap_cents,card_seen_at,current_period_start,current_period_end) values($1,$1,$2,$3,'api','active',$4,now(),date_trunc('month',now()),now()+interval '1 month')",
    [sub, id, email, cap],
  );
  await db.query(
    "insert into api_keys(id,user_id,name,prefix,secret_hash) values($1,$2,'Load fixture',$3,$4)",
    [keyId, id, key.slice(0, 24), createHash("sha256").update(key).digest()],
  );
  return { id, key, sub };
}
async function finish(id: string, key: string) {
  const begin = performance.now();
  const put = await request("upload", uploadUrls.get(id)!, {
    method: "PUT",
    headers: { "content-length": String(data.input_bytes) },
    body: svg,
  });
  check(put.status === 200, "upload failed");
  await request("start", `${api}/v1/jobs/${id}/start`, json({}, key));
  const deadline = Date.now() + 180_000;
  let status = "";
  while (Date.now() < deadline) {
    await Bun.sleep(2000);
    const r = await request("poll", `${api}/v1/jobs/${id}`, {
      headers: { authorization: `Bearer ${key}` },
    });
    status = r.json().status;
    if (["succeeded", "failed", "cancelled"].includes(status)) break;
  }
  check(status === "succeeded", `${id}: terminal ${status}`);
  if (status !== "succeeded") return;
  const r = await request("download-url", `${api}/v1/jobs/${id}/download`, {
    headers: { authorization: `Bearer ${key}` },
  });
  const output = await request("download", r.json().outputs[0].url);
  check(
    Buffer.from(output.bytes.slice(0, 8)).equals(Buffer.from([137, 80, 78, 71, 13, 10, 26, 10])),
    `${id}: invalid PNG`,
  );
  (samples["conversion-total"] ??= []).push({
    ms: performance.now() - begin,
    status: 200,
    unexpected: false,
  });
}
const uploadUrls = new Map<string, string>();
let peakConnections = 0;
const timer = setInterval(() => {
  void db
    .query("select count(*)::int as n from pg_stat_activity where datname=current_database()")
    .then((r) => {
      peakConnections = Math.max(peakConnections, r.rows[0].n);
    })
    .catch((e) => failures.push(`DB monitor: ${String(e)}`));
}, 500);
try {
  // Warm compilation and connections outside measured traffic.
  for (const path of ["/", "/pricing", "/download"])
    await localFetch(web + path).then((r) => r.text());
  await batch(300, (i) =>
    request(
      ["landing", "pricing", "download-page"][i % 3],
      web + ["/", "/pricing", "/download"][i % 3],
    ),
  );
  // A dedicated address proves the per-email limit without clearing shared buckets.
  const otpEmail = `${run}-otp@convt.test`;
  const otp: number[] = [];
  const ipStatuses: number[] = [];
  await batch(20, async (i) => {
    const r = await request(
      "email-code",
      `${web}/api/auth/email-otp/send-verification-otp`,
      {
        ...json({ email: otpEmail, type: "sign-in" }),
        headers: {
          ...(json({}).headers as Record<string, string>),
          "cf-connecting-ip": testIp.replace(/::1$/, `::${(i + 100).toString(16)}`),
        },
      },
      [200, 429],
    );
    otp.push(r.status);
  });
  check(
    otp.filter((s) => s === 200).length === 3 && otp.includes(429),
    "Email send limit did not hold",
  );
  await batch(20, async (i) => {
    const r = await request(
      "email-code-ip",
      `${web}/api/auth/email-otp/send-verification-otp`,
      {
        ...json({ email: `${run}-ip${i}@convt.test`, type: "sign-in" }),
        headers: {
          ...(json({}).headers as Record<string, string>),
          "cf-connecting-ip": testIp.replace(/::1$/, "::2"),
        },
      },
      [200, 429],
    );
    ipStatuses.push(r.status);
  });
  check(
    ipStatuses.filter((s) => s === 200).length === 10 &&
      ipStatuses.filter((s) => s === 429).length === 10,
    "IP send limit did not hold",
  );
  // Device auth uses a PKCE exchange, not an OAuth device polling grant.
  const device: number[] = [];
  await batch(50, async () => {
    const r = await request(
      "device-token",
      `${web}/api/device/token`,
      json({ code: "A".repeat(43), verifier: "B".repeat(43) }),
      [400, 429],
    );
    device.push(r.status);
  });
  check(
    device.filter((s) => s === 400).length === 30 && device.filter((s) => s === 429).length === 20,
    "Device token limit did not hold",
  );
  const keys: Awaited<ReturnType<typeof fixture>>[] = [];
  for (let i = 0; i < concurrency; i++) keys.push(await fixture(100));
  await batch(keys.length, async (i) => {
    const key = keys[i].key;
    const r = await request("create", `${api}/v1/jobs`, json(data, key));
    const body = r.json();
    uploadUrls.set(body.job.id, body.upload_url);
    await finish(body.job.id, key);
  });
  const quota = await fixture(3);
  const accepted: string[] = [];
  let rejected = 0;
  await batch(30, async () => {
    const r = await request("quota-burst", `${api}/v1/jobs`, json(data, quota.key), [200, 403]);
    const b = r.json();
    if (r.status === 200) {
      accepted.push(b.job.id);
      uploadUrls.set(b.job.id, b.upload_url);
    } else {
      check(b.error?.code === "limit_reached", `Unexpected quota rejection ${JSON.stringify(b)}`);
      rejected++;
    }
  });
  check(
    accepted.length === 3 && rejected === 27,
    `Quota accepted ${accepted.length}, rejected ${rejected}`,
  );
  await batch(accepted.length, (i) => finish(accepted[i], quota.key));
  await request("quota-after-settlement", `${api}/v1/jobs`, json(data, quota.key), [403]);
  // Replay a genuine paid order from the local provider through the HTTP Worker.
  const mock = local(services.BILLING_MOCK_URL);
  const checkout = newId("chk");
  await db.query(
    "insert into checkouts(id,product,nonce_hash,nonce_expires_at,status) values($1,'desktop',$2,now()+interval '1 hour','open')",
    [checkout, randomBytes(32)],
  );
  const co = await localFetch(`${mock}/v1/checkouts/`, {
    ...json({
      products: ["prod_local_desktop"],
      allow_trial: false,
      metadata: { convt_checkout: checkout },
      success_url: web + "/checkout/success",
    }),
    headers: {
      "content-type": "application/json",
      authorization: `Bearer ${billing.POLAR_ACCESS_TOKEN}`,
    },
  }).then((r) => r.json());
  await db.query("update checkouts set provider_checkout_id=$2 where id=$1", [checkout, co.id]);
  await localFetch(
    `${mock}/admin/complete-checkout`,
    json({ checkout_id: co.id, card: "4242", email: `${run}-buyer@convt.test` }),
  ).then((r) => r.text());
  const state = await localFetch(`${mock}/admin/state`).then((r) => r.json());
  const order = state.orders.find((o: { checkout_id: string }) => o.checkout_id === co.id);
  check(Boolean(order), "Mock paid order missing");
  const eventId = `msg_${run}`;
  const body = JSON.stringify({
    type: "order.paid",
    timestamp: new Date().toISOString(),
    api_version: "2026-10",
    data: order,
  });
  const headers = deliveryHeaders(
    billing.POLAR_WEBHOOK_SECRET,
    "standard",
    eventId,
    Math.floor(Date.now() / 1000),
    body,
  );
  await batch(100, () =>
    request("webhook-replay", `${web}/webhooks/polar`, { method: "POST", headers, body }),
  );
  const replay = (
    await db.query(
      "select (select count(*)::int from webhook_events where provider_event_id=$1 and status='processed') events,(select count(*)::int from orders where checkout_id=$2) orders,(select count(*)::int from licenses l join orders o on o.id=l.order_id where o.checkout_id=$2) licenses,(select count(*)::int from email_outbox where to_email=$3 and kind='license_issued') emails",
      [eventId, checkout, `${run}-buyer@convt.test`],
    )
  ).rows[0];
  check(
    Object.values(replay).every((n) => n === 1),
    `Replay side effects ${JSON.stringify(replay)}`,
  );
  await Bun.sleep(3000);
  const accounting = (
    await db.query(
      `select
    (select count(*)::int from cloud_jobs where user_id=any($1) and reservation='open') open_reservations,
    (select count(*)::int from cloud_jobs where user_id=any($1) and status='succeeded') succeeded,
    (select count(*)::int from cloud_jobs where user_id=any($1) and (attempt<>1 or lease_expires_at is not null)) fencing_anomalies,
    (select count(*)::int from usage_events where user_id=any($1) and kind='api_conversion') usage_events,
    (select count(*)::int from usage_events where user_id=any($1) and kind='api_conversion' and (quantity<>1 or amount_cents<>1)) invalid_usage,
    (select coalesce(sum(amount_cents),0)::int from usage_events where subscription_id=$2) quota_cents,
    (select count(*)::int from (select job_id,kind from usage_events where user_id=any($1) and corrects is null group by job_id,kind having count(*)>1) d) duplicate_usage`,
      [users, quota.sub],
    )
  ).rows[0];
  check(
    accounting.open_reservations === 0 &&
      accounting.succeeded === concurrency + 3 &&
      accounting.usage_events === concurrency + 3 &&
      accounting.quota_cents === 3 &&
      accounting.duplicate_usage === 0 &&
      accounting.invalid_usage === 0 &&
      accounting.fencing_anomalies === 0,
    `Accounting ${JSON.stringify(accounting)}`,
  );
  let meter: { checked: boolean; events?: number } = { checked: false };
  if (process.env.LOAD_METER_URL) {
    const state = (await localFetch(local(process.env.LOAD_METER_URL) + "/state").then((r) =>
      r.json(),
    )) as {
      events: {
        external_id: string;
        external_customer_id: string;
        metadata: { quantity: number };
      }[];
    };
    const rows = (
      await db.query(
        "select job_id,user_id,reported_at from usage_events where user_id=any($1) and kind='api_conversion'",
        [users],
      )
    ).rows;
    const delivered = (state.events as Receipt[]).filter((e) =>
      users.includes(e.external_customer_id),
    );
    check(
      rows.length === concurrency + 3 && meterMatches(state.events, rows, users),
      "Meter delivery missing or over-counted",
    );
    meter = { checked: true, events: delivered.length };
  }
  console.log(
    JSON.stringify({ run, concurrency, peakConnections, accounting, replay, meter }, null, 2),
  );
} finally {
  clearInterval(timer);
  // Revoke credentials and expire only this run's jobs. API cleanup removes objects.
  if (users.length) {
    await db.query("update api_keys set revoked_at=now() where user_id=any($1)", [users]);
    await db.query("update cloud_jobs set expires_at=now() where user_id=any($1)", [users]);
    await db.query(
      "update subscriptions set status='canceled',ended_at=now() where user_id=any($1)",
      [users],
    );
  }
  await db.end();
  const summary = Object.fromEntries(
    Object.entries(samples).map(([name, rs]) => {
      const times = rs.map((r) => r.ms).sort((a, b) => a - b);
      const p = (q: number) =>
        Math.round(times[Math.max(0, Math.ceil(q * times.length) - 1)] * 100) / 100;
      return [
        name,
        {
          requests: rs.length,
          p50: p(0.5),
          p95: p(0.95),
          p99: p(0.99),
          unexpected: rs.filter((r) => r.unexpected).length,
          fiveXX: rs.filter((r) => r.status >= 500).length,
          errorRate: rs.filter((r) => r.unexpected).length / rs.length,
          statuses: Object.fromEntries(
            [...new Set(rs.map((r) => r.status))].map((s) => [
              s,
              rs.filter((r) => r.status === s).length,
            ]),
          ),
        },
      ];
    }),
  );
  console.log(JSON.stringify({ summary, failures }, null, 2));
  if (failures.length) process.exitCode = 1;
}
