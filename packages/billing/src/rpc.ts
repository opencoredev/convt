// What the site Worker may ask convt-billing through its service binding. Every
// method that touches an account takes the user id the site read from its own
// verified session; convt-billing never sees cookies other than the checkout
// nonce, and the site never sees a billing secret.

import type { CatalogProduct } from "./catalog";
import type { CheckoutRefusal, CheckoutResult } from "./checkout";
import type { MarketingPreference, TokenPreference } from "./marketing";

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
    /** The desktop app opened this checkout; the success page then sends the buyer back to it. */
    fromApp?: boolean;
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
  currentProAccess(
    userId: string,
  ): Promise<
    | { kind: "pro" }
    | { kind: "trial"; endsOn: string; endsAt: string }
    | { kind: "can_start_trial"; checkoutUrl: string }
    | { kind: "lapsed" }
  >;
  /** Whether the signed-in account gets campaign email. */
  marketingPreference(userId: string): Promise<MarketingPreference>;
  setMarketingPreference(userId: string, subscribed: boolean): Promise<MarketingPreference>;
  /**
   * The same for a preferences link from an email, without signing in. Null when
   * the token is not one convt-billing signed.
   */
  preferenceByToken(token: string): Promise<TokenPreference | null>;
  setPreferenceByToken(token: string, subscribed: boolean): Promise<TokenPreference | null>;
}

export type {
  CheckoutRefusal,
  CheckoutResult,
  CatalogProduct,
  MarketingPreference,
  TokenPreference,
};
