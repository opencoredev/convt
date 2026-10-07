import { cloudAllowance, createApiKey, revokeApiKey } from "@convt/db/queries";
import { createServerFn } from "@tanstack/react-start";
import { env } from "cloudflare:workers";
import { authed } from "./session";

export const addApiKey = createServerFn({ method: "POST" })
  .middleware([authed])
  .validator((data: { name: string }) => {
    const name = typeof data?.name === "string" ? data.name.trim() : "";
    if (!name || name.length > 80) throw new Error("Name your key using 1 to 80 characters.");
    return { name };
  })
  .handler(async ({ data, context: { db, userId } }) => createApiKey(db, userId, data.name));
export const removeApiKey = createServerFn({ method: "POST" })
  .middleware([authed])
  .validator((data: { id: string }) => {
    if (typeof data?.id !== "string" || data.id.length > 64) throw new Error("Invalid key.");
    return data;
  })
  .handler(async ({ data, context: { db, userId } }) => ({
    ok: await revokeApiKey(db, userId, data.id),
  }));
export const fetchApiSpend = createServerFn({ method: "GET" })
  .middleware([authed])
  .handler(async ({ context: { db, userId } }) => cloudAllowance(db, userId, "api"));
export const fetchCloudAccess = createServerFn({ method: "GET" })
  .middleware([authed])
  .handler(async ({ context: { db, userId } }) => {
    const allowance = await cloudAllowance(db, userId, "pro");
    return {
      allowed: allowance.allowed,
      state: allowance.state,
      configured:
        typeof env.CONVT_API_URL === "string" && typeof env.CONVT_WEB_TOKEN_SECRET === "string",
    };
  });
export const fetchCloudCredential = createServerFn({ method: "POST" })
  .middleware([authed])
  .handler(async ({ context: { db, userId } }) => {
    if (!(await cloudAllowance(db, userId, "pro")).allowed)
      throw new Error("Cloud conversion requires Pro or a Pro trial.");
    const secret = env.CONVT_WEB_TOKEN_SECRET;
    const baseUrl = env.CONVT_API_URL;
    if (typeof secret !== "string" || secret.length < 32 || typeof baseUrl !== "string")
      throw new Error("Cloud conversion is not configured yet.");
    const body = btoa(
      JSON.stringify({
        sub: userId,
        exp: Math.floor(Date.now() / 1000) + 300,
        aud: "convt-cloud-web",
      }),
    )
      .replace(/=/g, "")
      .replace(/\+/g, "-")
      .replace(/\//g, "_");
    const key = await crypto.subtle.importKey(
      "raw",
      new TextEncoder().encode(secret),
      { name: "HMAC", hash: "SHA-256" },
      false,
      ["sign"],
    );
    const signature = new Uint8Array(
      await crypto.subtle.sign("HMAC", key, new TextEncoder().encode(body)),
    );
    return {
      baseUrl,
      token: `cvt_web_${body}.${Array.from(signature, (n) => n.toString(16).padStart(2, "0")).join("")}`,
    };
  });
