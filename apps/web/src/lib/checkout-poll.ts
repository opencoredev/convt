import { checkoutGiveUp, type CheckoutGiveUp, type CheckoutView } from "#/server/views";

// The success page's polling loop, kept out of the component so it can be tested.
// At most one request is in flight: a call that doesn't answer within `callTimeoutMs`
// is aborted before the next one is scheduled, and nothing it returns later counts.

export type CheckoutPollTiming = { pollMs: number; giveUpMs: number; callTimeoutMs: number };

export const checkoutPollTiming: CheckoutPollTiming = {
  pollMs: 2000,
  giveUpMs: 60_000,
  callTimeoutMs: 12_000,
};

export function pollCheckout({
  fetch,
  onState,
  timing = checkoutPollTiming,
}: {
  fetch: (args: { sync: boolean; signal: AbortSignal }) => Promise<CheckoutView>;
  onState: (state: CheckoutView | CheckoutGiveUp) => void;
  timing?: CheckoutPollTiming;
}): () => void {
  let stopped = false;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let inFlight: AbortController | undefined;
  // After the first pending answer, later calls ask convt-billing to sync this
  // checkout from the provider. Keep asking while Polar is still attaching the
  // subscription; a single early GET used to give up and show "key by email".
  let syncNext = false;
  let syncedOnce = false;
  let lastKnown: { product: CheckoutView["product"]; allowTrial: boolean } = {
    product: null,
    allowTrial: false,
  };
  const started = Date.now();
  const expired = () => Date.now() - started > timing.giveUpMs;

  const call = (sync: boolean): Promise<CheckoutView> => {
    const controller = new AbortController();
    inFlight = controller;
    // A call that never answers (a dropped connection) must not stall the page.
    const timeout = setTimeout(() => controller.abort(), timing.callTimeoutMs);
    const aborted = new Promise<never>((_, reject) =>
      controller.signal.addEventListener("abort", () => reject(new Error("aborted")), {
        once: true,
      }),
    );
    return Promise.race([fetch({ sync, signal: controller.signal }), aborted]).finally(() => {
      clearTimeout(timeout);
      if (inFlight === controller) inFlight = undefined;
    });
  };

  const poll = async () => {
    try {
      const sync = syncNext;
      if (sync) syncedOnce = true;
      const r = await call(sync);
      if (stopped) return;
      if (r.state !== "ready") lastKnown = { product: r.product, allowTrial: r.allowTrial };
      if (r.state === "pending") {
        syncNext = true;
        if (expired()) {
          onState(checkoutGiveUp(r.product, r.allowTrial));
          return;
        }
        onState(r);
        timer = setTimeout(poll, syncedOnce ? timing.pollMs : 0);
        return;
      }
      onState(r);
    } catch {
      if (stopped) return;
      if (expired()) {
        onState(checkoutGiveUp(lastKnown.product, lastKnown.allowTrial));
        return;
      }
      timer = setTimeout(poll, timing.pollMs);
    }
  };

  void poll();
  return () => {
    stopped = true;
    clearTimeout(timer);
    inFlight?.abort();
  };
}
