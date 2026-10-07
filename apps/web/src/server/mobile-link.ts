// The emailed download link for phone visitors. The address is used to send one
// email and is otherwise kept only as a SHA-256 hash in the send-limit buckets
// (otp_send_limits), which convt-billing deletes within 10 minutes of the window
// ending. It is never logged.

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

export function downloadLinkMessage(
  to: string,
  siteUrl: string,
  offer: LaunchOffer | null,
  idempotencyKey: string,
): MailMessage {
  const rendered = downloadLink({
    downloadUrl: `${siteUrl}/download`,
    offer: offer ?? undefined,
  });
  return { to, ...rendered, idempotencyKey };
}

export type MobileLinkDeps = {
  /** consumeSendBucket bound to a database: counts one use and returns the window's count. */
  consume: (key: string, windowMs: number) => Promise<number>;
  /** releaseSendBucket: ends a bucket's window, so a failed send does not count as sent. */
  release: (key: string) => Promise<void>;
  send: (message: MailMessage) => Promise<void>;
  siteUrl: string;
  now: Date;
};

export async function requestMobileLink(
  input: { email: string; ip: string },
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
  const message = downloadLinkMessage(
    email,
    deps.siteUrl,
    activeOffer(deps.now),
    `mobile-link-${hash.slice(0, 32)}-${slot}`,
  );
  try {
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
