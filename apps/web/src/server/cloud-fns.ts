import { cloudAllowance, createApiKey, revokeApiKey } from "@convt/db/queries";
import { createServerFn } from "@tanstack/react-start";
import { env } from "cloudflare:workers";

import { cloudConfig, mintCloudCredential } from "./cloud-credential";
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
  .handler(async ({ context: { db, userId } }) => ({
    ...(await cloudAllowance(db, userId, "pro")),
    configured: cloudConfig(env).kind === "configured",
  }));
export const fetchCloudCredential = createServerFn({ method: "POST" })
  .middleware([authed])
  .handler(async ({ context: { db, userId } }) => {
    if (!(await cloudAllowance(db, userId, "pro")).allowed)
      throw new Error("Cloud conversion requires an active paid Pro subscription.");
    const config = cloudConfig(env);
    if (config.kind === "missing") throw new Error("Cloud conversion is not configured yet.");
    return mintCloudCredential({ config, userId, now: new Date() });
  });
