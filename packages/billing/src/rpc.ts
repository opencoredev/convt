// What the site Worker may ask convt-billing through its service binding. Every
// method that touches an account takes the user id the site read from its own
// verified session; convt-billing never sees cookies other than the checkout
// nonce, and the site never sees a billing secret.

import type { CatalogProduct } from "./catalog";
import type { CheckoutRefusal, CheckoutResult } from "./checkout";

export type ActionReason =
  | "not_found"
  | "declined"
  | "provider_error"
  | "bad_cap"
  | "trial"
  | "discount";

export interface BillingRpc {
  health(): Promise<{ ok: boolean; problems: string[] }>;
  createCheckout(input: {
    product: CatalogProduct;
    user: { id: string; email: string } | null;
    spendCapCents?: number | null;
  }): Promise<
    | { ok: true; url: string; checkoutId: string; cookieValue: string }
    | { ok: false; refusal: CheckoutRefusal }
  >;
  checkoutResult(input: {
    providerCheckoutId: string;
    cookie: string | null;
    sessionUserId: string | null;
    sync: boolean;
  }): Promise<{ result: CheckoutResult; setCookie: string | null }>;
  switchInterval(
    userId: string,
    to: "month" | "year",
  ): Promise<{ ok: true } | { ok: false; reason: ActionReason }>;
  setCancel(
    userId: string,
    kind: "pro" | "api",
    cancel: boolean,
  ): Promise<{ ok: true } | { ok: false; reason: ActionReason }>;
  portalUrl(userId: string): Promise<string | null>;
  receiptUrl(userId: string, invoiceId: string): Promise<string | null>;
  card(userId: string): Promise<{ brand: string; last4: string; expires: string } | null>;
  setSpendCap(
    userId: string,
    cents: number,
  ): Promise<{ ok: true } | { ok: false; reason: ActionReason }>;
  multipleSubscriptionsAllowed(): Promise<boolean | null>;
  requestDeletion(userId: string): Promise<{ id: string; status: string }>;
  /**
   * P8 renewal: the account's newest unrevoked paid Pro key, or null. The site calls
   * it for a signed-in desktop device, with the user id from the device token.
   */
  currentProKey(userId: string): Promise<{ key: string; updatesUntil: string } | null>;
  /**
   * CNV-56: the account's desktop trial, started at `now` if it has none.
   * `storedDeviceHash` is the site's peppered hash (64 lowercase hex), never the
   * app's own. `device_used`: this computer started another account's trial.
   */
  startTrial(
    userId: string,
    storedDeviceHash: string,
    now: Date,
  ): Promise<{ ok: true; key: string; endsAt: Date } | { ok: false; reason: "device_used" }>;
  /**
   * CNV-56: the key the app should hold. The best unrevoked paid key of either plan,
   * else a trial token for a Pro subscription in its trial, else null.
   */
  currentKey(userId: string, now: Date): Promise<{ key: string; updatesUntil: string } | null>;
}

export type { CheckoutRefusal, CheckoutResult, CatalogProduct };
