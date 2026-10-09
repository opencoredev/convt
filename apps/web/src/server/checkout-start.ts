// The checkout start routes. convt-billing records the checkout and its nonce
// before calling the provider; this answers 303 to the provider's page and sets the
// nonce cookie that later releases the key on the success page.

import type { CatalogProduct } from "@convt/billing/rpc";

import { billing, allowCheckout, checkoutCookie, productForPro } from "./billing";
import { createAuth } from "./auth";
import { requestContext } from "./context";

const nonceMaxAge = 2 * 60 * 60;

function to(url: string, cookie?: string) {
  const headers = new Headers({ location: url, "cache-control": "private, no-store" });
  if (cookie) headers.append("set-cookie", cookie);
  return new Response(null, { status: 303, headers });
}

export async function startCheckout(request: Request, context: unknown, kind: "desktop" | "pro") {
  const { scope, appEnv } = requestContext(context);
  const url = new URL(request.url);
  const session = await createAuth(scope, appEnv).api.getSession({ headers: request.headers });
  const user = session?.user.emailVerified
    ? { id: session.user.id, email: session.user.email }
    : null;
  // The desktop app's Start free trial opens /checkout/pro?from=app (convt-billing's
  // checkout_url), so the success page can send the buyer back to the app.
  const fromApp = url.searchParams.get("from") === "app";
  let product: CatalogProduct = "desktop";
  if (kind === "pro") {
    const p = productForPro(url.searchParams.get("interval") ?? "month");
    if (!p) return new Response("Unknown interval", { status: 400 });
    product = p;
    if (!user) {
      const back = `/checkout/pro?interval=${p === "pro_year" ? "year" : "month"}${fromApp ? "&from=app" : ""}`;
      return to(`/sign-in?redirect=${encodeURIComponent(back)}`);
    }
  }
  const ip = request.headers.get("cf-connecting-ip")?.trim() || "unknown";
  if (!(await allowCheckout(scope.db, ip, user?.id ?? null)))
    return to("/checkout/success?error=rate_limited");
  const created = await billing().createCheckout({ product, user, fromApp });
  if (!created.ok) {
    if (created.refusal === "already_pro") return to("/dashboard/billing");
    return to(`/checkout/success?error=${created.refusal}`);
  }
  return to(created.url, checkoutCookie(appEnv, created.cookieValue, nonceMaxAge));
}
