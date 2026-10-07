import { formatDate, formatMoney } from "./format";
import type { ApiEnrollment, Billing } from "./types";

const liveApiStates = new Set<ApiEnrollment["state"]>(["pending", "enrolled", "payment_failed"]);

/** True only when there is no Pro (trial or paid) and no live API enrollment. */
export function billingHasNoPlan(billing: Pick<Billing, "plan" | "api">): boolean {
  return billing.plan === null && !liveApiStates.has(billing.api.state);
}

/** Spend-cap line for the billing card, plus an end date when the API plan is set to stop. */
export function apiSpendLine(api: ApiEnrollment): string {
  const cap =
    api.spendCapCents !== null
      ? `Spend cap ${formatMoney(api.spendCapCents)} a month`
      : "No spend cap";
  return api.endsOn ? `${cap}. Ends ${formatDate(api.endsOn)}` : cap;
}
