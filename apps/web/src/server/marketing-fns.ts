// Campaign email preferences. convt-billing owns the consent rows and the
// Sequenzy push; these server functions only pass the user id from the verified
// session, or a signed link token, through the BILLING service binding.

import { createServerFn } from "@tanstack/react-start";
import { setResponseHeader } from "@tanstack/react-start/server";

import { billing } from "./billing";
import { authed } from "./session";

/** Null when convt-billing cannot answer; the page then says so instead of guessing. */
export const fetchMarketingPreference = createServerFn({ method: "GET" })
  .middleware([authed])
  .handler(async ({ context: { userId } }) =>
    billing()
      .marketingPreference(userId)
      .catch(() => null),
  );

const parseSubscribed = (data: unknown): { subscribed: boolean } => {
  if (typeof data !== "object" || data === null || !("subscribed" in data))
    throw new Error("bad preference");
  if (typeof data.subscribed !== "boolean") throw new Error("bad preference");
  return { subscribed: data.subscribed };
};

export const saveMarketingPreference = createServerFn({ method: "POST" })
  .middleware([authed])
  .validator(parseSubscribed)
  .handler(async ({ data, context: { userId } }) =>
    billing().setMarketingPreference(userId, data.subscribed),
  );

/** A token is `<user id>.<signature>`; anything else is not worth a round trip. */
export function parseToken(value: unknown): string | null {
  return typeof value === "string" && /^usr_[0-9A-Za-z]{1,40}\.[A-Za-z0-9_-]{43}$/.test(value)
    ? value
    : null;
}

export type LinkPreference =
  | { state: "ok"; subscribed: boolean; maskedEmail: string }
  | { state: "invalid" }
  | { state: "unavailable" };

export const fetchLinkPreference = createServerFn({ method: "GET" })
  .validator((data: unknown) => ({
    token:
      typeof data === "object" && data !== null && "token" in data ? parseToken(data.token) : null,
  }))
  .handler(async ({ data }): Promise<LinkPreference> => {
    setResponseHeader("cache-control", "private, no-store");
    if (!data.token) return { state: "invalid" };
    try {
      const pref = await billing().preferenceByToken(data.token);
      return pref ? { state: "ok", ...pref } : { state: "invalid" };
    } catch {
      return { state: "unavailable" };
    }
  });

export const saveLinkPreference = createServerFn({ method: "POST" })
  .validator((data: unknown) => {
    const { subscribed } = parseSubscribed(data);
    const token =
      typeof data === "object" && data !== null && "token" in data ? parseToken(data.token) : null;
    return { token, subscribed };
  })
  .handler(async ({ data }): Promise<LinkPreference> => {
    setResponseHeader("cache-control", "private, no-store");
    if (!data.token) return { state: "invalid" };
    try {
      const pref = await billing().setPreferenceByToken(data.token, data.subscribed);
      return pref ? { state: "ok", ...pref } : { state: "invalid" };
    } catch {
      return { state: "unavailable" };
    }
  });
