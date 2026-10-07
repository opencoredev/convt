import { expect, test } from "bun:test";

import { PRO_CLOUD_SOFT_BUDGET_CENTS, proCloudSoftBudgetExceeded } from "../../src/queries/cloud";

test("Pro cloud soft budget is $8 and hidden from callers until cost is recorded", () => {
  expect(PRO_CLOUD_SOFT_BUDGET_CENTS).toBe(800);
  expect(proCloudSoftBudgetExceeded(0)).toBe(false);
  expect(proCloudSoftBudgetExceeded(799)).toBe(false);
  expect(proCloudSoftBudgetExceeded(800)).toBe(true);
});
