import { describe, expect, test } from "bun:test";
import { localPriceIds } from "@convt/db/seed";
import { localProducts } from "@convt/billing-mock";

import {
  complimentaryDesktop,
  discountProblem,
  loadCatalog,
  meteredPriceProblem,
  validateCatalog,
} from "../../src/catalog";
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

  test("production lists both Polar ids for the PRODUCTHUNT launch code", () => {
    const c = loadCatalog("production");
    const a = c.discounts["64641dd4-73ae-4704-8fbf-450bed2b2aa4"];
    const b = c.discounts["8d401db5-99d3-44c2-82e9-6483dec9ced7"];
    expect(a).toEqual(b);
    expect(a?.code).toBe("PRODUCTHUNT");
    expect(a?.products).toEqual(["desktop", "pro_month"]);
  });

  test("production accepts the two 100% launch discounts only on their products", () => {
    const c = loadCatalog("production");
    const family = c.discounts["17bb47c4-8b7b-4fb5-b013-31ad52a0e909"];
    const zortos = c.discounts["8a134038-3319-4905-a893-8635b0fd7cd7"];
    expect(family).toEqual({ code: "K0SIYK55", basisPoints: 10000, products: ["pro_month"] });
    expect(zortos).toEqual({ code: "SUIGL3WV", basisPoints: 10000, products: ["desktop"] });

    expect(discountProblem(c, "17bb47c4-8b7b-4fb5-b013-31ad52a0e909", "pro_month")).toBeNull();
    expect(
      discountProblem(c, "17bb47c4-8b7b-4fb5-b013-31ad52a0e909", "pro_month", {
        netCents: 0,
        subtotalCents: 1200,
        discountCents: 1200,
        items: [{ priceId: c.products.pro_month.priceId, amountCents: 1200 }],
      }),
    ).toBeNull();
    expect(
      discountProblem(c, "17bb47c4-8b7b-4fb5-b013-31ad52a0e909", "pro_month", {
        netCents: 0,
        subtotalCents: 1200,
        discountCents: 1198,
        items: [{ priceId: c.products.pro_month.priceId, amountCents: 1200 }],
      }),
    ).toMatch(/the order took 1198/);
    expect(discountProblem(c, "17bb47c4-8b7b-4fb5-b013-31ad52a0e909", "desktop")).toMatch(
      /does not apply/,
    );
    expect(discountProblem(c, "8a134038-3319-4905-a893-8635b0fd7cd7", "desktop")).toBeNull();
    expect(discountProblem(c, "8a134038-3319-4905-a893-8635b0fd7cd7", "pro_month")).toMatch(
      /does not apply/,
    );
    expect(discountProblem(c, "unknown-discount", "pro_month")).toMatch(/unknown discount/);

    // Polar's subscription_create trial has a zero subtotal; its later cycle
    // charges the monthly list price and records the full discount.
    expect(
      discountProblem(c, "17bb47c4-8b7b-4fb5-b013-31ad52a0e909", "pro_month", {
        netCents: 0,
        subtotalCents: 0,
        discountCents: 0,
        items: [],
      }),
    ).toBeNull();
    expect(
      discountProblem(c, "17bb47c4-8b7b-4fb5-b013-31ad52a0e909", "pro_month", {
        netCents: 0,
        subtotalCents: 1200,
        discountCents: 1200,
        items: [{ priceId: c.products.pro_month.priceId, amountCents: 1200 }],
      }),
    ).toBeNull();
  });

  test("a 100% Polar Desktop write-off is complimentary; a partial unknown discount is not", () => {
    const c = loadCatalog("local");
    const price = c.products.desktop.priceId;
    const full = {
      netCents: 0,
      subtotalCents: 2900,
      discountCents: 2900,
      items: [{ priceId: price, amountCents: 2900 }],
    };
    const zeroed = {
      netCents: 0,
      subtotalCents: 0,
      discountCents: 0,
      items: [{ priceId: price, amountCents: 0 }],
    };
    expect(complimentaryDesktop(c, full)).toBe(true);
    expect(complimentaryDesktop(c, zeroed)).toBe(true);
    expect(discountProblem(c, "disc_giveaway", "desktop", full)).toBeNull();
    expect(discountProblem(c, null, "desktop", full)).toBeNull();
    expect(discountProblem(c, "disc_giveaway", "desktop", zeroed)).toBeNull();
    expect(
      discountProblem(c, "disc_other", "desktop", {
        netCents: 2030,
        subtotalCents: 2900,
        discountCents: 870,
        items: [{ priceId: price, amountCents: 2900 }],
      }),
    ).toMatch(/unknown discount/);
    expect(
      complimentaryDesktop(c, {
        netCents: 2030,
        subtotalCents: 2900,
        discountCents: 870,
        items: [{ priceId: price, amountCents: 2900 }],
      }),
    ).toBe(false);
  });

  test("updates_until is the same day a year later; 29 February becomes 28 February", () => {
    expect(addYears("2026-10-05", 1)).toBe("2027-10-05");
    expect(addYears("2028-02-29", 1)).toBe("2029-02-28");
    expect(addYears("2027-12-31", 1)).toBe("2028-12-31");
  });
});
