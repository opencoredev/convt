// The desktop app's Start free trial opens /checkout/pro?from=app, the one way Pro
// is still started (CNV-101). It may only ever start a trial: an account that
// can't (it had Pro, or has Pro or a Desktop + Cloud year now) gets no checkout.

import type { BillingRpc } from "@convt/billing/rpc";

export type ProAccess = Awaited<ReturnType<BillingRpc["currentProAccess"]>>;

/** Where the app's trial URL goes for an account: on to checkout, or a page. */
export function proTrialEntry(access: ProAccess): "checkout" | "/dashboard/billing" | "/#pricing" {
  switch (access.kind) {
    case "can_start_trial":
      return "checkout";
    case "pro":
    case "trial":
      // Already covered: billing shows the subscription, trial or cloud year.
      return "/dashboard/billing";
    case "lapsed":
      return "/#pricing";
    default: {
      const _exhaustive: never = access;
      return _exhaustive;
    }
  }
}
