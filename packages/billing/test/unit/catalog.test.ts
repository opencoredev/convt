import { describe, expect, test } from "bun:test";
import { localPriceIds } from "@convt/db/seed";
import { localProducts } from "@convt/billing-mock";

import { loadCatalog, meteredPriceProblem, validateCatalog } from "../../src/catalog";
import { addYears } from "../../src/context";

describe("catalog", () => {
  test("a metered price that is not a whole number of cents of at least 1 is refused", () => {
    expect(meteredPriceProblem("1")).toBeNull();
    expect(meteredPriceProblem("25")).toBeNull();
    expect(meteredPriceProblem("1.00")).toBeNull();
    for (const bad of ["0.5", "0", "1.25", "abc", null])
      expect(meteredPriceProblem(bad)).not.toBeNull();
    const c = loadCatalog("local");
    const broken = {
      ...c,
      products: { ...c.products, api: { ...c.products.api, unitAmount: "0.5" } },
    };
    expect(validateCatalog(broken).join()).toMatch(/whole number of cents/);
  });

  test("the local catalog, the seed and the mock agree on ids and prices", () => {
    const c = loadCatalog("local");
    for (const k of ["desktop", "pro_month", "pro_year", "api"] as const) {
      expect(c.products[k].priceId).toBe(localPriceIds[k]);
      expect(c.products[k].productId).toBe(localProducts[k].productId);
      expect(c.products[k].priceId).toBe(localProducts[k].priceId);
      expect(c.products[k].amountCents).toBe(localProducts[k].amount);
    }
    expect(c.switchPolicy).toEqual({ pro_month: "invoice", pro_year: "invoice" });
  });

  test("sandbox and production have real, distinct ids and the same prices as local", () => {
    const local = loadCatalog("local");
    for (const env of ["sandbox", "production"] as const) {
      const c = loadCatalog(env);
      expect(validateCatalog(c)).toEqual([]);
      const ids = Object.values(c.products).flatMap((e) => [e.productId, e.priceId]);
      expect(new Set(ids).size).toBe(ids.length);
      for (const [name, e] of Object.entries(c.products)) {
        const { productId: _p, priceId: _q, ...terms } = e;
        const {
          productId: _lp,
          priceId: _lq,
          ...localTerms
        } = local.products[name as keyof typeof local.products];
        expect(terms).toEqual(localTerms);
      }
    }
    expect(loadCatalog("sandbox").products.desktop.productId).not.toBe(
      loadCatalog("production").products.desktop.productId,
    );
  });

  test("updates_until is the same day a year later; 29 February becomes 28 February", () => {
    expect(addYears("2026-10-05", 1)).toBe("2027-10-05");
    expect(addYears("2028-02-29", 1)).toBe("2029-02-28");
    expect(addYears("2027-12-31", 1)).toBe("2028-12-31");
  });
});
