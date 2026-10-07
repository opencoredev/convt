// The emailed download link for phone visitors. Giving an address also joins the
// launch mailing list (launch_list), which the card says; every email carries an
// unsubscribe link that deletes the row. The send limits key on a SHA-256 of the
// address (otp_send_limits). The address is never logged.

import { downloadLink } from "@convt/mail";

import { activeOffer, type LaunchOffer } from "#/lib/launch-offer";
import { isCaptureSource, normalizeEmail, type CaptureSource } from "#/lib/mobile";

import type { MailMessage } from "./mail";
import { redactText } from "./redact";

export const mobileLinkLimits = {
  /** A second request for the same address inside this window is a double tap: no email. */
  duplicateWindowMs: 60 * 1000,
  email: { max: 3, windowMs: 60 * 60 * 1000 },
  ip: { max: 10, windowMs: 60 * 60 * 1000 },
};

export type MobileLinkInput = { email: string; source: CaptureSource };

export type MobileLinkResult =
  | { ok: true }
  | { ok: false; error: "invalid_email" | "too_many" | "send_failed" };

/** The server function's validator: a string email (checked later) and a known source. */
export function parseMobileLinkInput(data: unknown): MobileLinkInput {
  const d = data as Partial<Record<keyof MobileLinkInput, unknown>> | null;
  if (typeof d?.email !== "string" || d.email.length > 1000 || !isCaptureSource(d.source))
    throw new Error("bad request");
  return { email: d.email, source: d.source };
}

async function sha256Hex(text: string): Promise<string> {
  const digest = new Uint8Array(
    await crypto.subtle.digest("SHA-256", new TextEncoder().encode(text)),
  );
  return [...digest].map((b) => b.toString(16).padStart(2, "0")).join("");
}

/**
 * The address's unsubscribe token: an HMAC of it under the site's auth secret, so it
 * is the same in every email and any of them unsubscribes. 43 base64url characters;
 * the database keeps only its SHA-256.
 */
export async function unsubscribeToken(secret: string, email: string): Promise<string> {
  const key = await crypto.subtle.importKey(
    "raw",
    new TextEncoder().encode(secret),
    { name: "HMAC", hash: "SHA-256" },
    false,
    ["sign"],
  );
  const mac = new Uint8Array(
    await crypto.subtle.sign(
      "HMAC",
      key,
      new TextEncoder().encode(`launch-list-unsubscribe\n${email}`),
    ),
  );
  return btoa(String.fromCharCode(...mac))
    .replace(/\+/g, "-")
    .replace(/\//g, "_")
    .replace(/=+$/, "");
}

export const hashUnsubscribeToken = sha256Hex;

/** The /unsubscribe server function's validator. */
export function parseUnsubscribeInput(data: unknown): { token: string } {
  const token = (data as { token?: unknown } | null)?.token;
  if (typeof token !== "string" || !/^[A-Za-z0-9_-]{43}$/.test(token))
    throw new Error("bad request");
  return { token };
}

/** The token rides in the fragment, so it never reaches a server log or a Referer. */
export const unsubscribeUrl = (siteUrl: string, token: string) =>
  `${siteUrl}/unsubscribe#${new URLSearchParams({ t: token })}`;

export function downloadLinkMessage(
  to: string,
  siteUrl: string,
  offer: LaunchOffer | null,
  unsubscribeToken: string,
  idempotencyKey: string,
): MailMessage {
  const rendered = downloadLink({
    downloadUrl: `${siteUrl}/download`,
    unsubscribeUrl: unsubscribeUrl(siteUrl, unsubscribeToken),
    offer: offer ?? undefined,
  });
  return { to, ...rendered, idempotencyKey };
}

export type MobileLinkDeps = {
  /** consumeSendBucket bound to a database: counts one use and returns the window's count. */
  consume: (key: string, windowMs: number) => Promise<number>;
  /** releaseSendBucket: ends a bucket's window, so a failed send does not count as sent. */
  release: (key: string) => Promise<void>;
  /** joinLaunchList bound to a database. */
  join: (entry: {
    email: string;
    source: CaptureSource;
    unsubscribeTokenHash: string;
  }) => Promise<void>;
  send: (message: MailMessage) => Promise<void>;
  siteUrl: string;
  /** Keys the unsubscribe token (the site's auth secret). */
  unsubscribeSecret: string;
  now: Date;
};

export async function requestMobileLink(
  input: { email: string; source: CaptureSource; ip: string },
  deps: MobileLinkDeps,
): Promise<MobileLinkResult> {
  const email = normalizeEmail(input.email);
  if (!email) return { ok: false, error: "invalid_email" };
  const hash = await sha256Hex(email);
  const limits = mobileLinkLimits;
  // The IP first, so a flood of made-up addresses never reaches the per-email buckets.
  if ((await deps.consume(`mobile-link:ip:${input.ip}`, limits.ip.windowMs)) > limits.ip.max)
    return { ok: false, error: "too_many" };
  // A second tap inside the duplicate window was already answered: no email, and it
  // does not use up one of the address's tries. Accepted gap: if two requests race and
  // the first send fails, the second has already answered ok; the first caller sees
  // the error, and its retry sends because a failure releases this bucket.
  const recent = `mobile-link:recent:${hash}`;
  if ((await deps.consume(recent, limits.duplicateWindowMs)) > 1) return { ok: true };
  if ((await deps.consume(`mobile-link:email:${hash}`, limits.email.windowMs)) > limits.email.max) {
    await deps.release(recent);
    return { ok: false, error: "too_many" };
  }
  // Stable for the duplicate window, so a provider retry of the same send is dropped too.
  const slot = Math.floor(deps.now.getTime() / limits.duplicateWindowMs);
  const token = await unsubscribeToken(deps.unsubscribeSecret, email);
  const message = downloadLinkMessage(
    email,
    deps.siteUrl,
    activeOffer(deps.now),
    token,
    `mobile-link-${hash.slice(0, 32)}-${slot}`,
  );
  try {
    await deps.join({
      email,
      source: input.source,
      unsubscribeTokenHash: await hashUnsubscribeToken(token),
    });
    await deps.send(message);
    return { ok: true };
  } catch (e) {
    await deps.release(recent).catch(() => {});
    console.error(
      "[mobile-link] send failed",
      redactText(e instanceof Error ? e.message : String(e), false),
    );
    return { ok: false, error: "send_failed" };
  }
}
