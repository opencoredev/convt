// The five-minute `cvt_web_` credential convt-server accepts for paid Pro
// conversions. The dashboard converter (cloud-fns.ts) and the desktop app
// (/api/device/cloud) both get theirs here; the caller checks Pro first.
// convt-server verifies it in crates/convt-server/src/tokens.rs.

/** How long a credential lasts. convt-server refuses anything longer. */
export const cloudCredentialSeconds = 300;

export type CloudConfig =
  | { kind: "configured"; secret: string; baseUrl: string }
  | { kind: "missing" };

/** The API address and shared secret, if this Worker has both. */
export function cloudConfig(env: Record<string, unknown>): CloudConfig {
  const secret = env.CONVT_WEB_TOKEN_SECRET;
  const baseUrl = env.CONVT_API_URL;
  if (typeof secret !== "string" || secret.length < 32 || typeof baseUrl !== "string" || !baseUrl)
    return { kind: "missing" };
  return { kind: "configured", secret, baseUrl };
}

export type CloudCredential = { baseUrl: string; token: string };

const base64url = (text: string) =>
  btoa(text).replace(/=/g, "").replace(/\+/g, "-").replace(/\//g, "_");

/** Signs a credential for `userId`, valid from `now` for five minutes. */
export async function mintCloudCredential(args: {
  config: Extract<CloudConfig, { kind: "configured" }>;
  userId: string;
  now: Date;
}): Promise<CloudCredential> {
  const body = base64url(
    JSON.stringify({
      sub: args.userId,
      exp: Math.floor(args.now.getTime() / 1000) + cloudCredentialSeconds,
      aud: "convt-cloud-web",
    }),
  );
  const key = await crypto.subtle.importKey(
    "raw",
    new TextEncoder().encode(args.config.secret),
    { name: "HMAC", hash: "SHA-256" },
    false,
    ["sign"],
  );
  const signature = new Uint8Array(
    await crypto.subtle.sign("HMAC", key, new TextEncoder().encode(body)),
  );
  const hex = Array.from(signature, (n) => n.toString(16).padStart(2, "0")).join("");
  return { baseUrl: args.config.baseUrl, token: `cvt_web_${body}.${hex}` };
}
