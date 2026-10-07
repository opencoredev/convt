// The discount the mobile download-link email offers. The only place its code, terms
// and end date live: change the offer here. The code must be one convt-billing accepts
// (packages/billing/src/catalog.ts), or checkout rejects the order; a unit test checks.

export type LaunchOffer = {
  code: string;
  /** One sentence for the email, after "Use code X at checkout for". */
  terms: string;
  /** The first moment the code no longer works. */
  endsAt: Date;
  /** The end date as the email states it. */
  endsLabel: string;
};

export const launchOffer: LaunchOffer = {
  code: "PRODUCTHUNT",
  terms: "30% off convt Desktop, or off your first 3 months of Pro monthly",
  // Through 31 October 2026, Pacific time (PDT, UTC-7).
  endsAt: new Date("2026-11-01T00:00:00-07:00"),
  endsLabel: "31 October 2026 (Pacific time)",
};

/** The offer while it runs; null after its end date, which hides the incentive. */
export function activeOffer(now: Date, offer: LaunchOffer = launchOffer): LaunchOffer | null {
  return now < offer.endsAt ? offer : null;
}
