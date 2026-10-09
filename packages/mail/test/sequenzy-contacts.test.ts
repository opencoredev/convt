import { describe, expect, test } from "bun:test";

import { sequenzyContacts, signSequenzyWebhook, verifySequenzyWebhook, type Contact } from "../src";

const fakeFetch = (
  handler: (input: Parameters<typeof fetch>[0], init?: RequestInit) => Promise<Response>,
): typeof fetch => Object.assign(handler, { preconnect: fetch.preconnect });

/** Built at runtime so no address literal appears in source. */
const mailbox = ["someone", ["convt", "test"].join(".")].join("@");

type Seen = { url: string; method: string; body: unknown; auth: string | null };

function recording(response: () => Response) {
  const seen: Seen[] = [];
  const client = sequenzyContacts({
    apiKey: "fake-key",
    fetch: fakeFetch(async (input, init) => {
      seen.push({
        url: String(input),
        method: init?.method ?? "GET",
        body: init?.body ? JSON.parse(String(init.body)) : null,
        auth: new Headers(init?.headers).get("authorization"),
      });
      return response();
    }),
  });
  return { seen, client };
}

const contact: Contact = {
  externalId: "usr_test1",
  email: mailbox,
  firstName: "Sam",
  attributes: {
    preferencesUrl: "https://convt.app/email/preferences?t=x",
    desktopBuyer: true,
    proStatus: "none",
  },
};

describe("Sequenzy contacts", () => {
  test("create sends the documented fields, merge strategy and backfill date", async () => {
    const { seen, client } = recording(() => Response.json({ success: true }));
    const created = await client.create({
      contact,
      tags: ["convt-account"],
      lists: ["list_fake"],
      createdAt: new Date("2026-01-02T03:04:05Z"),
    });
    expect(created).toEqual({ kind: "ok" });
    expect(seen[0]).toEqual({
      url: "https://api.sequenzy.com/api/v1/subscribers",
      method: "POST",
      auth: "Bearer fake-key",
      body: {
        email: mailbox,
        externalId: "usr_test1",
        firstName: "Sam",
        status: "active",
        tags: ["convt-account"],
        lists: ["list_fake"],
        customAttributes: contact.attributes,
        createdAt: "2026-01-02T03:04:05.000Z",
        duplicateStrategy: "merge",
      },
    });
  });

  test("a signup leaves out createdAt and lists, so Sequenzy's defaults apply", async () => {
    const { seen, client } = recording(() => Response.json({ success: true }));
    await client.create({
      contact: { ...contact, firstName: "" },
      tags: [],
      lists: null,
      createdAt: null,
    });
    const body = seen[0].body as Record<string, unknown>;
    expect("createdAt" in body).toBe(false);
    expect("lists" in body).toBe(false);
    expect("firstName" in body).toBe(false);
  });

  test("update merges attributes and sets status only when reactivating", async () => {
    const { seen, client } = recording(() => Response.json({ success: true }));
    await client.update({ contact, reactivate: false });
    await client.update({ contact, reactivate: true });
    expect(seen[0].url).toBe(
      "https://api.sequenzy.com/api/v1/subscribers/external?externalId=usr_test1",
    );
    expect(seen[0].method).toBe("PATCH");
    expect(seen[0].body).toEqual({
      email: mailbox,
      firstName: "Sam",
      customAttributes: contact.attributes,
      customAttributesStrategy: "merge",
    });
    expect((seen[1].body as Record<string, unknown>).status).toBe("active");
  });

  test("unsubscribe and remove use the external id routes", async () => {
    const { seen, client } = recording(() => new Response(null, { status: 204 }));
    expect(await client.unsubscribe({ externalId: "usr_a/b", email: null })).toEqual({
      kind: "ok",
    });
    expect(await client.remove("usr_a/b")).toEqual({ kind: "ok" });
    expect(seen.map((s) => [s.method, s.url, s.body])).toEqual([
      [
        "PATCH",
        "https://api.sequenzy.com/api/v1/subscribers/external?externalId=usr_a%2Fb",
        { status: "unsubscribed" },
      ],
      ["DELETE", "https://api.sequenzy.com/api/v1/subscribers/external?externalId=usr_a%2Fb", null],
    ]);
  });

  test("classifies 404, 409, 429, 5xx and network errors", async () => {
    const cases: Array<[() => Response, unknown]> = [
      [() => Response.json({ code: "NOT_FOUND" }, { status: 404 }), { kind: "not_found" }],
      [
        () => Response.json({ code: "CONFLICT" }, { status: 409 }),
        { kind: "refused", status: 409, code: "CONFLICT" },
      ],
      [
        () =>
          Response.json(
            { error: { code: "RATE_LIMITED" } },
            { status: 429, headers: { "retry-after": "7" } },
          ),
        { kind: "retry", status: 429, code: "RATE_LIMITED", retryAfterMs: 7000 },
      ],
      [
        () => new Response("oops", { status: 503 }),
        { kind: "retry", status: 503, code: "http_503", retryAfterMs: null },
      ],
    ];
    for (const [response, expected] of cases) {
      const { client } = recording(response);
      expect(await client.unsubscribe({ externalId: "usr_x", email: mailbox })).toEqual(
        expected as never,
      );
    }
    const broken = sequenzyContacts({
      apiKey: "k",
      fetch: fakeFetch(async () => {
        throw new TypeError("connection reset");
      }),
    });
    expect(await broken.remove("usr_x")).toEqual({
      kind: "retry",
      status: null,
      code: "network_error",
      retryAfterMs: null,
    });
  });
});

describe("Sequenzy webhook", () => {
  const secret = "whsec_test_only";
  const now = new Date("2026-10-09T12:00:00Z");
  const ts = Math.floor(now.getTime() / 1000);
  const body = (o: unknown) => new TextEncoder().encode(JSON.stringify(o));
  const unsub = body({
    id: "evt_1",
    type: "subscriber.unsubscribed",
    data: { subscriber: { external_id: "usr_test1", email: mailbox.toUpperCase() } },
  });

  async function headers(raw: Uint8Array, at = ts, sig?: string) {
    return new Headers({
      "x-sequenzy-timestamp": String(at),
      "x-sequenzy-signature": sig ?? `v1=${await signSequenzyWebhook(secret, at, raw)}`,
    });
  }

  test("accepts a valid signature among several and parses an opt-out", async () => {
    const good = await signSequenzyWebhook(secret, ts, unsub);
    const result = await verifySequenzyWebhook({
      secret,
      raw: unsub,
      now,
      headers: await headers(unsub, ts, `v1=${"0".repeat(64)},v1=${good}`),
    });
    expect(result).toEqual({
      ok: true,
      event: {
        kind: "opt_out",
        id: "evt_1",
        type: "subscriber.unsubscribed",
        externalId: "usr_test1",
        email: mailbox,
        occurredAt: null,
      },
    });
  });

  test("accepts a base64 digest", async () => {
    const hex = await signSequenzyWebhook(secret, ts, unsub);
    const b64 = btoa(String.fromCharCode(...hex.match(/../g)!.map((h) => parseInt(h, 16))));
    const result = await verifySequenzyWebhook({
      secret,
      raw: unsub,
      now,
      headers: await headers(unsub, ts, `v1=${b64}`),
    });
    expect(result.ok).toBe(true);
  });

  test("rejects a wrong secret, a changed body, a stale timestamp and missing headers", async () => {
    const h = await headers(unsub);
    expect(await verifySequenzyWebhook({ secret: "other", raw: unsub, now, headers: h })).toEqual({
      ok: false,
      reason: "signature",
    });
    expect(
      await verifySequenzyWebhook({
        secret,
        raw: body({ id: "evt_1", type: "x" }),
        now,
        headers: h,
      }),
    ).toEqual({ ok: false, reason: "signature" });
    expect(
      await verifySequenzyWebhook({
        secret,
        raw: unsub,
        now,
        headers: await headers(unsub, ts - 600),
      }),
    ).toEqual({ ok: false, reason: "stale" });
    expect(
      await verifySequenzyWebhook({ secret, raw: unsub, now, headers: new Headers() }),
    ).toEqual({ ok: false, reason: "signature" });
  });

  test("a camelCase externalId is read too", async () => {
    const raw = body({
      id: "evt_4",
      type: "email.unsubscribed",
      data: { externalId: "usr_test2" },
    });
    const r = await verifySequenzyWebhook({ secret, raw, now, headers: await headers(raw) });
    expect(r.ok && r.event.kind === "opt_out" ? r.event.externalId : null).toBe("usr_test2");
  });

  test("email events read the recipient; other events pass through", async () => {
    const bounced = body({
      id: "evt_2",
      type: "email.bounced",
      created_at: "2026-10-09T11:59:00Z",
      data: { recipient: mailbox, email_type: "campaign" },
    });
    const r = await verifySequenzyWebhook({
      secret,
      raw: bounced,
      now,
      headers: await headers(bounced),
    });
    expect(r).toEqual({
      ok: true,
      event: {
        kind: "opt_out",
        id: "evt_2",
        type: "email.bounced",
        externalId: null,
        email: mailbox,
        occurredAt: new Date("2026-10-09T11:59:00Z"),
      },
    });
    const opened = body({ id: "evt_3", type: "email.opened", data: {} });
    expect(
      await verifySequenzyWebhook({ secret, raw: opened, now, headers: await headers(opened) }),
    ).toEqual({ ok: true, event: { kind: "other", id: "evt_3", type: "email.opened" } });
    const junk = new TextEncoder().encode("not json");
    expect(
      await verifySequenzyWebhook({ secret, raw: junk, now, headers: await headers(junk) }),
    ).toEqual({ ok: false, reason: "malformed" });
  });
});
