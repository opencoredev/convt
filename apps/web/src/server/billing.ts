// The site's side of billing. Every write goes to convt-billing through the BILLING
// service binding; this Worker holds no billing secret and has no write grant on a
// billing table. Account actions take the user id from the verified session.

import { consumeSendBucket, type Db } from "@convt/db";
import type { BillingRpc, CatalogProduct } from "@convt/billing/rpc";
import { env as rawEnv } from "cloudflare:workers";

import type { AppEnv } from "./env";

export function billing(): BillingRpc {
  return rawEnv.BILLING;
}

/** `__Host-convt_checkout` in production (Secure, no Domain); `convt_checkout` on local http. */
export function checkoutCookieName(appEnv: AppEnv): string {
  return appEnv.env === "production" ? "__Host-convt_checkout" : "convt_checkout";
}

export function checkoutCookie(appEnv: AppEnv, value: string, maxAgeSeconds: number): string {
  const parts = [
    `${checkoutCookieName(appEnv)}=${value}`,
    "Path=/",
    "HttpOnly",
    // Lax, so it rides the top-level return from the provider's checkout.
    "SameSite=Lax",
    `Max-Age=${maxAgeSeconds}`,
  ];
  if (appEnv.env === "production") parts.push("Secure");
  return parts.join("; ");
}

export function readCookie(header: string | null, name: string): string | null {
  if (!header) return null;
  for (const part of header.split(";")) {
    const [k, ...v] = part.trim().split("=");
    if (k === name) return v.join("=");
  }
  return null;
}

export const checkoutLimits = {
  ip: { max: 10, windowMs: 60 * 60 * 1000 },
  user: { max: 5, windowMs: 60 * 60 * 1000 },
};

/** The P6 send-bucket function with `checkout:` keys. True when allowed. */
export async function allowCheckout(db: Db, ip: string, userId: string | null): Promise<boolean> {
  const now = new Date();
  const byIp = await consumeSendBucket(db, `checkout:ip:${ip}`, checkoutLimits.ip.windowMs, now);
  if (byIp > checkoutLimits.ip.max) return false;
  if (userId) {
    const byUser = await consumeSendBucket(
      db,
      `checkout:user:${userId}`,
      checkoutLimits.user.windowMs,
      now,
    );
    if (byUser > checkoutLimits.user.max) return false;
  }
  return true;
}

export const productForPro = (interval: string | null): CatalogProduct | null =>
  interval === "month" ? "pro_month" : interval === "year" ? "pro_year" : null;

/** Whole cents from a dollar amount typed by the user, or null. */
export function parseCapDollars(text: string): number | null {
  const t = text.trim().replace(/^\$/, "").replace(/,/g, "");
  if (!/^\d{1,5}(\.\d{1,2})?$/.test(t)) return null;
  const [whole, frac = ""] = t.split(".");
  const cents = Number(whole) * 100 + Number(frac.padEnd(2, "0"));
  return cents >= 100 && cents <= 1_000_000 ? cents : null;
}
