import { formatDate, formatMoney } from "./format";
import type { ApiEnrollment, Billing } from "./types";

const liveApiStates = new Set<ApiEnrollment["state"]>(["pending", "enrolled", "payment_failed"]);

/** True only when there is no Pro, no Desktop, and no live API enrollment. */
export function billingHasNoPlan(billing: Pick<Billing, "plan" | "api">): boolean {
  return billing.plan === null && !liveApiStates.has(billing.api.state);
}

/** Hide the Desktop checkout once the account already owns a live Desktop license. */
export function showGetDesktop(billing: Pick<Billing, "ownsDesktop">): boolean {
  return !billing.ownsDesktop;
}

/**
 * Copy for the payment-method panel when Polar has no card. A $0 or one-time
 * Desktop order never stores a card; "No card on file" reads like a broken panel.
 */
export function billingCardCopy(
  billing: Pick<Billing, "card" | "plan" | "ownsDesktop">,
): string | null {
  if (billing.card) return null;
  if (billing.plan?.kind === "desktop" || (billing.ownsDesktop && billing.plan === null))
    return "No card needed.";
  return "No card on file.";
}

/** Spend-cap line for the billing card, plus an end date when the API plan is set to stop. */
export function apiSpendLine(api: ApiEnrollment): string {
  const cap =
    api.spendCapCents !== null
      ? `Spend cap ${formatMoney(api.spendCapCents)} a month`
      : "No spend cap";
  return api.endsOn ? `${cap}. Ends ${formatDate(api.endsOn)}` : cap;
}
