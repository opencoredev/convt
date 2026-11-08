import { expect, test } from "bun:test";

import { BUNDLE_CLOUD_DAYS } from "../../src/catalog";
import { planCloudYears, type BundleRow } from "../../src/converge";

const day = 86_400_000;
const year = BUNDLE_CLOUD_DAYS * day;
const t0 = Date.UTC(2026, 9, 1);
const at = (days: number) => new Date(t0 + days * day);
const order = (id: string, billedDay: number, v: Partial<BundleRow> = {}): BundleRow => ({
  id,
  status: "paid",
  billed_at: at(billedDay),
  email: "a@convt.test",
  user_id: "usr_a",
  provider_customer_id: null,
  checkout_id: null,
  lost: false,
  sub_id: `sub_${id}`,
  sub_status: "active",
  sub_start: null,
  sub_end: null,
  sub_ended_at: null,
  ...v,
});
const laid = (start: Date) => ({
  sub_start: start,
  sub_end: new Date(start.getTime() + year),
  sub_ended_at: new Date(start.getTime() + year),
});

test("a correct layout plans no writes, so the reconciler leaves the account alone", () => {
  const a = order("a", 0, laid(at(0)));
  const b = order("b", 10, laid(new Date(t0 + year)));
  expect(planCloudYears([a, b], at(40))).toEqual([]);
});

test("a refunded first year frees its days: the second moves to the refund", () => {
  const a = order("a", 0, {
    status: "refunded",
    sub_status: "canceled",
    sub_start: at(0),
    sub_end: at(365),
    sub_ended_at: at(20),
  });
  const b = order("b", 10, laid(at(10)));
  expect(planCloudYears([a, b], at(30))).toEqual([
    { kind: "move", subId: "sub_b", start: at(20), stop: new Date(at(20).getTime() + year) },
  ]);
});

test("overlapping years stack, and a year with no row is inserted", () => {
  const a = order("a", 0, laid(at(0)));
  const b = order("b", 1, laid(at(1)));
  const c = order("c", 2, { sub_id: null, sub_status: null });
  const end = t0 + year;
  expect(planCloudYears([a, b, c], at(5))).toEqual([
    { kind: "move", subId: "sub_b", start: new Date(end), stop: new Date(end + year) },
    { kind: "insert", order: c, start: new Date(end + year), stop: new Date(end + 2 * year) },
  ]);
});
