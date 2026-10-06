// The success page's key release: only to the owning session or the browser with
// the checkout's nonce; rotation on first disclosure; refusals for every forgery.

import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { base64urlEncode } from "@convt/license";
import { sql } from "drizzle-orm";

import { createHarness, type Harness } from "../../src/testing";

let h: Harness;
beforeAll(async () => {
  h = await createHarness();
});
afterAll(async () => h?.close());

const result = (
  providerCheckoutId: string,
  cookie: string | null,
  sessionUserId: string | null = null,
  sync = false,
) => h.service.checkoutResult({ providerCheckoutId, cookie, sessionUserId, sync });

describe("checkout result", () => {
  test("pending until the order lands, then the key once with rotation; a reload within 10 minutes shows it again", async () => {
    const b = await h.buy("desktop", null, { email: "cookie@convt.test" });
    // Webhooks have not arrived: pending, and the first pending asks for a sync.
    expect((await result(b.providerCheckoutId, b.cookieValue)).result.state).toBe("pending");
    const synced = await result(b.providerCheckoutId, b.cookieValue, null, true);
    expect(synced.result.state).toBe("ready");
    expect(synced.setCookie).not.toBeNull();
    expect(synced.setCookie).not.toBe(b.cookieValue);
    if (synced.result.state !== "ready") throw new Error("not ready");
    expect(synced.result.maskedEmail).toBe("c***@convt.test");
    expect(synced.result.token.split(".").length).toBe(2);
    const [row] = await h.q<{ synced_at: string | null; key_disclosed_at: string | null }>(
      sql`select synced_at, key_disclosed_at from checkouts where id = ${b.checkoutId}`,
    );
    expect(row.synced_at).not.toBeNull();
    expect(row.key_disclosed_at).not.toBeNull();
    // The old nonce is now refused: "already shown".
    expect((await result(b.providerCheckoutId, b.cookieValue)).result.state).toBe("shown");
    // The rotated nonce shows the key again (a reload), without rotating again.
    const reload = await result(b.providerCheckoutId, synced.setCookie);
    expect(reload.result.state).toBe("ready");
    expect(reload.setCookie).toBeNull();
    // After 10 minutes the rotated nonce has expired.
    h.mock.advance(11 * 60_000);
    expect((await result(b.providerCheckoutId, synced.setCookie)).result.state).toBe("shown");
    h.mock.takeHeld();
  });

  test("the lost-first-response path shows the 'already shown' page; the key is still on its way by email", async () => {
    const b = await h.buy("desktop", null, { email: "lost@convt.test" });
    await h.deliverAll();
    const first = await result(b.providerCheckoutId, b.cookieValue);
    expect(first.result.state).toBe("ready");
    // The browser never got `first.setCookie`, so it still holds the old nonce.
    expect((await result(b.providerCheckoutId, b.cookieValue)).result.state).toBe("shown");
    const mails = await h.q(
      sql`select 1 from email_outbox where to_email = 'lost@convt.test' and kind = 'license_issued'`,
    );
    expect(mails.length).toBe(1);
  });

  test("an expired nonce, a nonce for another checkout, a forged checkout id and no cookie are refused", async () => {
    const a = await h.buy("desktop", null, { email: "a-cookie@convt.test" });
    const b = await h.buy("desktop", null, { email: "b-cookie@convt.test" });
    await h.deliverAll();
    const nonceA = a.cookieValue.split(".")[1];
    const forged = `${b.checkoutId}.${nonceA}`;
    const random = `${a.checkoutId}.${base64urlEncode(crypto.getRandomValues(new Uint8Array(32)))}`;
    for (const [name, cookie, id] of [
      ["another checkout's cookie", b.cookieValue, a.providerCheckoutId],
      ["an injected cookie with a forged checkout id", forged, b.providerCheckoutId],
      ["a random nonce", random, a.providerCheckoutId],
      ["no cookie", null, a.providerCheckoutId],
      ["garbage", "chk_x.y", a.providerCheckoutId],
    ] as const) {
      const r = await result(id, cookie);
      expect({ name, state: r.result.state }).toEqual({ name, state: "not_found" });
    }
    // A nonce past its 2-hour life is refused, checked on our side.
    const c = await h.buy("desktop", null, { email: "c-cookie@convt.test" });
    await h.deliverAll();
    h.mock.advance(2 * 3600_000 + 60_000);
    expect((await result(c.providerCheckoutId, c.cookieValue)).result.state).toBe("not_found");
    // None of these disclosed anything.
    const disclosed = await h.q(
      sql`select 1 from checkouts where id in (${a.checkoutId}, ${b.checkoutId}, ${c.checkoutId}) and key_disclosed_at is not null`,
    );
    expect(disclosed.length).toBe(0);
  });

  test("another checkout's cookie never reveals that a key was shown", async () => {
    const a = await h.buy("desktop", null, { email: "shown-a@convt.test" });
    const b = await h.buy("desktop", null, { email: "shown-b@convt.test" });
    await h.deliverAll();
    expect((await result(a.providerCheckoutId, a.cookieValue)).result.state).toBe("ready");
    // B's cookie on A's disclosed checkout: not "shown", just nothing.
    expect((await result(a.providerCheckoutId, b.cookieValue)).result.state).toBe("not_found");
  });

  test("the owning session sees the key without the cookie; another user's session does not", async () => {
    const u = await h.user("owner-cookie@convt.test");
    const other = await h.user("other-cookie@convt.test");
    const b = await h.buy("desktop", u);
    await h.deliverAll();
    expect((await result(b.providerCheckoutId, null, u.id)).result.state).toBe("ready");
    expect((await result(b.providerCheckoutId, null, other.id)).result.state).toBe("not_found");
    // The owner's view rotates nothing.
    const [row] = await h.q<{ key_disclosed_at: string | null }>(
      sql`select key_disclosed_at from checkouts where id = ${b.checkoutId}`,
    );
    expect(row.key_disclosed_at).toBeNull();
  });

  test("two concurrent first requests: only one gets the key and a new cookie", async () => {
    const b = await h.buy("desktop", null, { email: "concurrent-cookie@convt.test" });
    await h.deliverAll();
    const rs = await Promise.all(
      Array.from({ length: 6 }, () => result(b.providerCheckoutId, b.cookieValue)),
    );
    const ready = rs.filter((r) => r.result.state === "ready");
    expect(ready.length).toBe(1);
    expect(ready[0].setCookie).not.toBeNull();
  });

  test("a trial and a declined checkout report their states", async () => {
    const u = await h.user("trial-cookie@convt.test");
    const t = await h.buy("pro_month", u);
    await h.deliverAll();
    expect((await result(t.providerCheckoutId, t.cookieValue)).result.state).toBe("trial");
    const d = await h.service.createCheckout({ product: "desktop", user: null });
    if (!d.ok) throw new Error("refused");
    const co = h.mock.checkoutBySecret(d.url.split("/checkout/")[1])!;
    h.mock.completeCheckout(co.id, "0002");
    await h.deliverAll();
    expect((await result(co.id, d.cookieValue)).result.state).toBe("pending");
  });
});
