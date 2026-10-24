import { expect, test } from "bun:test";

import { pollCheckout } from "../../src/lib/checkout-poll";
import type { CheckoutView } from "../../src/server/views";

const timing = { pollMs: 10, giveUpMs: 10_000, callTimeoutMs: 30 };
const pending: CheckoutView = { state: "pending", product: "pro", allowTrial: false };
const failed: CheckoutView = { state: "failed", product: "pro", allowTrial: false };
const wait = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

test("a call that times out is aborted and its late answer is ignored", async () => {
  let calls = 0;
  const signals: AbortSignal[] = [];
  const states: string[] = [];
  const stop = pollCheckout({
    timing,
    onState: (s) => states.push(s.state),
    fetch: ({ signal }) => {
      calls += 1;
      signals.push(signal);
      // The first call ignores the abort and answers "failed" long after its timeout.
      return calls === 1 ? wait(80).then(() => failed) : wait(1).then(() => pending);
    },
  });
  await wait(120);
  stop();
  expect(signals[0]?.aborted).toBe(true);
  expect(calls).toBeGreaterThan(2);
  expect(states).not.toContain("failed");
});

test("the loop never has two calls outstanding at once", async () => {
  let outstanding = 0;
  let maxOutstanding = 0;
  let calls = 0;
  const stop = pollCheckout({
    timing,
    onState: () => {},
    fetch: ({ signal }) => {
      calls += 1;
      outstanding += 1;
      maxOutstanding = Math.max(maxOutstanding, outstanding);
      const slow = calls % 2 === 1;
      return new Promise<CheckoutView>((resolve, reject) => {
        const t = setTimeout(() => resolve(pending), slow ? 60 : 1);
        signal.addEventListener("abort", () => {
          clearTimeout(t);
          reject(new Error("aborted"));
        });
      }).finally(() => {
        outstanding -= 1;
      });
    },
  });
  await wait(200);
  stop();
  expect(calls).toBeGreaterThan(3);
  expect(maxOutstanding).toBe(1);
});

test("stopping aborts the call in flight and drops its answer", async () => {
  let signal: AbortSignal | undefined;
  const states: string[] = [];
  const stop = pollCheckout({
    timing,
    onState: (s) => states.push(s.state),
    fetch: (args) => {
      signal = args.signal;
      return wait(20).then(() => failed);
    },
  });
  stop();
  await wait(40);
  expect(signal?.aborted).toBe(true);
  expect(states).toEqual([]);
});

test("a final answer stops polling", async () => {
  let calls = 0;
  const states: string[] = [];
  pollCheckout({
    timing,
    onState: (s) => states.push(s.state),
    fetch: () => {
      calls += 1;
      return Promise.resolve(calls < 3 ? pending : failed);
    },
  });
  await wait(80);
  expect(calls).toBe(3);
  expect(states).toEqual(["pending", "pending", "failed"]);
});
