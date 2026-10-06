// Server functions for checkout results, billing actions, API enrollment and
// account deletion. Account actions run `authed`, so the user id comes from the
// verified session; convt-billing does the work through the service binding.

import { revokeOtherSessions } from "@convt/db/queries";
import { createServerFn } from "@tanstack/react-start";
import { getRequestHeader, setResponseHeader } from "@tanstack/react-start/server";

import { freshAgeSeconds } from "./auth";
import {
  billing,
  checkoutCookie,
  checkoutCookieName,
  parseCapDollars,
  readCookie,
} from "./billing";
import { loadSession } from "./context";
import { authed } from "./session";
import { checkoutView, type CheckoutView } from "./views";

const disclosedMaxAge = 10 * 60;

/**
 * The success page's poll. Released to the session user who owns the checkout or
 * to the browser holding the checkout's nonce cookie. Never in page HTML: only in
 * this response, which is private and not cached.
 */
export const fetchCheckoutResult = createServerFn({ method: "POST" })
  .validator((data: { checkoutId: string; sync: boolean }) => {
    if (
      typeof data?.checkoutId !== "string" ||
      data.checkoutId.length > 100 ||
      typeof data.sync !== "boolean"
    )
      throw new Error("bad checkout id");
    return data;
  })
  .handler(async ({ data, context }): Promise<CheckoutView> => {
    setResponseHeader("cache-control", "private, no-store");
    setResponseHeader("referrer-policy", "no-referrer");
    const { appEnv, session } = await loadSession(context);
    const cookie = readCookie(getRequestHeader("cookie") ?? null, checkoutCookieName(appEnv));
    const r = await billing().checkoutResult({
      providerCheckoutId: data.checkoutId,
      cookie,
      sessionUserId: session?.user.emailVerified ? session.user.id : null,
      sync: data.sync,
    });
    if (r.setCookie)
      setResponseHeader("set-cookie", checkoutCookie(appEnv, r.setCookie, disclosedMaxAge));
    return checkoutView(r.result);
  });

type ActionResult = { ok: true } | { ok: false; message: string };

const messages: Record<string, string> = {
  declined: "Your card was declined. Nothing changed.",
  not_found: "There's no plan to change.",
  provider_error: "The payment provider didn't answer. Try again in a minute.",
  bad_cap: "Enter a whole amount of cents between $1 and $10,000.",
};

const result = (r: { ok: true } | { ok: false; reason: string }): ActionResult =>
  r.ok ? r : { ok: false, message: messages[r.reason] ?? "Something went wrong. Try again." };

export const switchPlanInterval = createServerFn({ method: "POST" })
  .middleware([authed])
  .validator((data: { to: "month" | "year" }) => {
    if (data?.to !== "month" && data?.to !== "year") throw new Error("bad interval");
    return data;
  })
  .handler(async ({ data, context: { userId } }) =>
    result(await billing().switchInterval(userId, data.to)),
  );

export const setPlanCancel = createServerFn({ method: "POST" })
  .middleware([authed])
  .validator((data: { kind: "pro" | "api"; cancel: boolean }) => {
    if ((data?.kind !== "pro" && data?.kind !== "api") || typeof data.cancel !== "boolean")
      throw new Error("bad request");
    return data;
  })
  .handler(async ({ data, context: { userId } }) =>
    result(await billing().setCancel(userId, data.kind, data.cancel)),
  );

/** Polar's customer portal for card, receipt email, invoices and payment retries. */
export const openPortal = createServerFn({ method: "POST" })
  .middleware([authed])
  .handler(async ({ context: { userId } }) => ({ url: await billing().portalUrl(userId) }));

export const openReceipt = createServerFn({ method: "POST" })
  .middleware([authed])
  .validator((data: { id: string }) => {
    if (typeof data?.id !== "string" || data.id.length > 64) throw new Error("bad invoice id");
    return data;
  })
  .handler(async ({ data, context: { userId } }) => ({
    url: await billing().receiptUrl(userId, data.id),
  }));

/** Starts API enrollment: a checkout that saves a card, with the spend cap stored first. */
export const enrollApi = createServerFn({ method: "POST" })
  .middleware([authed])
  .validator((data: { cap: string }) => {
    if (typeof data?.cap !== "string" || data.cap.length > 20) throw new Error("bad cap");
    return data;
  })
  .handler(async ({ data, context: { userId, user, appEnv } }) => {
    if (appEnv.sales !== "all")
      return { ok: false as const, message: "API billing is coming soon." };
    const cents = parseCapDollars(data.cap);
    if (cents === null) return { ok: false as const, message: messages.bad_cap };
    const created = await billing().createCheckout({
      product: "api",
      user: { id: userId, email: user.email },
      spendCapCents: cents,
    });
    if (!created.ok) {
      const why: Record<string, string> = {
        already_enrolled: "This account already has API billing.",
        needs_multiple_subscriptions:
          "API billing can't start yet. Our payment provider needs a setting turned on first.",
        bad_cap: messages.bad_cap,
        provider_error: messages.provider_error,
      };
      return { ok: false as const, message: why[created.refusal] ?? messages.provider_error };
    }
    setResponseHeader("set-cookie", checkoutCookie(appEnv, created.cookieValue, 2 * 60 * 60));
    return { ok: true as const, url: created.url };
  });

export const saveSpendCap = createServerFn({ method: "POST" })
  .middleware([authed])
  .validator((data: { cap: string }) => {
    if (typeof data?.cap !== "string" || data.cap.length > 20) throw new Error("bad cap");
    return data;
  })
  .handler(async ({ data, context: { userId } }) => {
    const cents = parseCapDollars(data.cap);
    if (cents === null) return { ok: false as const, message: messages.bad_cap };
    return result(await billing().setSpendCap(userId, cents));
  });

/**
 * Deletes the account: a fresh session (signed in within the last hour) and the
 * email typed to confirm. convt-billing ends subscriptions first, then deletes.
 */
export const deleteAccount = createServerFn({ method: "POST" })
  .middleware([authed])
  .validator((data: { email: string }) => {
    if (typeof data?.email !== "string" || data.email.length > 320) throw new Error("bad email");
    return data;
  })
  .handler(async ({ data, context: { userId, user, sessionId, sessionCreatedAt, db } }) => {
    if (Date.now() - sessionCreatedAt.getTime() > freshAgeSeconds * 1000)
      return {
        ok: false as const,
        code: "SESSION_NOT_FRESH",
        message: "Sign in again to delete your account.",
      };
    if (data.email.trim().toLowerCase() !== user.email.toLowerCase())
      return {
        ok: false as const,
        code: "EMAIL_MISMATCH",
        message: "Type your account's email exactly to confirm.",
      };
    const d = await billing().requestDeletion(userId);
    // Every other browser and every Mac is signed out now; this session ends with the account.
    await revokeOtherSessions(db, userId, sessionId, new Date());
    return { ok: true as const, status: d.status };
  });
