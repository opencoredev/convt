import { describe, expect, test } from "bun:test";
import { localPriceIds } from "@convt/db/seed";
import { localProducts } from "@convt/billing-mock";

import {
  complimentaryOrder,
  discountProblem,
  listPriceOf,
  loadCatalog,
  meteredPriceProblem,
  placeholderIds,
  productByPriceId,
  productByProviderId,
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
    for (const k of ["desktop", "bundle", "pro_month", "pro_year", "api"] as const) {
      expect(c.products[k].priceId).toBe(localPriceIds[k]);
      expect(c.products[k].productId).toBe(localProducts[k].productId);
      expect(c.products[k].priceId).toBe(localProducts[k].priceId);
      expect(c.products[k].amountCents).toBe(localProducts[k].amount);
    }
    expect(c.switchPolicy).toEqual({ pro_month: "invoice", pro_year: "invoice" });
  });

  test("sandbox and production have distinct ids and the same prices as local", () => {
    const local = loadCatalog("local");
    for (const env of ["sandbox", "production"] as const) {
      const c = loadCatalog(env);
      // The only problem allowed is ids still waiting for the provider, and only for
      // the products CNV-101 adds; the deploy refuses them (scripts/deploy-worker.sh).
      expect(validateCatalog(c).filter((p) => !p.includes("placeholders"))).toEqual([]);
      for (const id of placeholderIds(c))
        expect(id).toMatch(/^PLACEHOLDER_[A-Z]+_(DESKTOP_39_PRICE|BUNDLE_PRODUCT|BUNDLE_PRICE)$/);
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
      expect(c.legacyPrices.map((l) => [l.product, l.amountCents])).toEqual([["desktop", 2900]]);
    }
    expect(loadCatalog("sandbox").products.desktop.productId).not.toBe(
      loadCatalog("production").products.desktop.productId,
    );
    // The production $29 price stays recognized for every order sold at it.
    const prod = loadCatalog("production");
    expect(productByPriceId(prod, "a68e7412-964d-4dfc-bd9b-7aa6353ae37f")).toBe("desktop");
    expect(productByProviderId(prod, "9d3028c1-1e4f-4d12-9641-59960eb58679")).toBe("desktop");
  });

  test("Desktop is $39 and Desktop + Cloud $69; the old $29 price is legacy at its own amount", () => {
    const c = loadCatalog("local");
    expect(c.products.desktop.amountCents).toBe(3900);
    expect(c.products.bundle.amountCents).toBe(6900);
    expect(listPriceOf(c, "desktop", "price_local_desktop_39")).toBe(3900);
    expect(listPriceOf(c, "desktop", "price_local_desktop")).toBe(2900);
    expect(listPriceOf(c, "bundle", "price_local_bundle")).toBe(6900);
    // A price belongs to one product only.
    expect(listPriceOf(c, "bundle", "price_local_desktop")).toBeNull();
    expect(listPriceOf(c, "desktop", "price_local_bundle")).toBeNull();
    expect(listPriceOf(c, "desktop", "price_unknown")).toBeNull();
    expect(productByPriceId(c, "price_local_desktop")).toBe("desktop");
    expect(productByProviderId(c, "prod_local_bundle")).toBe("bundle");
    const broken = {
      ...c,
      legacyPrices: [{ ...c.legacyPrices[0], priceId: "price_local_desktop_39" }],
    };
    expect(validateCatalog(broken).join()).toMatch(/still a current price/);
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
    expect(family).toEqual({ code: "FAMILY", basisPoints: 10000, products: ["pro_month"] });
    expect(zortos).toEqual({
      code: "ZORTOS_DISCORD",
      basisPoints: 10000,
      products: ["desktop"],
    });

    expect(discountProblem(c, "17bb47c4-8b7b-4fb5-b013-31ad52a0e909", "pro_month")).toBeNull();
    expect(
      discountProblem(c, "17bb47c4-8b7b-4fb5-b013-31ad52a0e909", "pro_month", {
        netCents: 0,
        subtotalCents: 1200,
        discountCents: 1198,
        items: [{ priceId: c.products.pro_month.priceId, amountCents: 1200 }],
      }),
    ).toMatch(/the order took 1198/);
    const complimentaryAmounts = {
      netCents: 1,
      subtotalCents: 2900,
      discountCents: 2900,
      items: [{ priceId: c.products.desktop.priceId, amountCents: 2900 }],
    };
    expect(
      discountProblem(c, "17bb47c4-8b7b-4fb5-b013-31ad52a0e909", "desktop", complimentaryAmounts),
    ).toMatch(/does not apply/);
    expect(
      discountProblem(c, "8a134038-3319-4905-a893-8635b0fd7cd7", "desktop", complimentaryAmounts),
    ).toBeNull();
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
    const price = "price_local_desktop"; // The legacy $29 price, at its own amount.
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
    expect(complimentaryOrder(c, "desktop", full)).toBe(true);
    expect(complimentaryOrder(c, "desktop", zeroed)).toBe(true);
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
      complimentaryOrder(c, "desktop", {
        netCents: 2030,
        subtotalCents: 2900,
        discountCents: 870,
        items: [{ priceId: price, amountCents: 2900 }],
      }),
    ).toBe(false);
  });

  test("complimentary one-time orders: both Polar shapes, at the current prices only", () => {
    const c = loadCatalog("local");
    const shapes = (priceId: string, amount: number) => ({
      full: {
        netCents: 0,
        subtotalCents: amount,
        discountCents: amount,
        items: [{ priceId, amountCents: amount }],
      },
      zeroed: {
        netCents: 0,
        subtotalCents: 0,
        discountCents: 0,
        items: [{ priceId, amountCents: 0 }],
      },
    });
    for (const [product, priceId, amount] of [
      ["desktop", "price_local_desktop_39", 3900],
      ["bundle", "price_local_bundle", 6900],
    ] as const) {
      const { full, zeroed } = shapes(priceId, amount);
      expect(complimentaryOrder(c, product, full)).toBe(true);
      expect(complimentaryOrder(c, product, zeroed)).toBe(true);
      expect(discountProblem(c, "disc_giveaway", product, full)).toBeNull();
      // $29 on the $39 price is neither the list price nor zero.
      expect(complimentaryOrder(c, product, shapes(priceId, 2900).full)).toBe(false);
    }
    // The bundle's price is not the Desktop product's, and the reverse.
    expect(complimentaryOrder(c, "desktop", shapes("price_local_bundle", 6900).full)).toBe(false);
    // PRODUCTHUNT never applied to Desktop + Cloud.
    expect(
      discountProblem(c, "disc_local_producthunt", "bundle", {
        netCents: 4830,
        subtotalCents: 6900,
        discountCents: 2070,
        items: [{ priceId: "price_local_bundle", amountCents: 6900 }],
      }),
    ).toMatch(/does not apply to bundle/);
  });

  test("updates_until is the same day a year later; 29 February becomes 28 February", () => {
    expect(addYears("2026-10-05", 1)).toBe("2027-10-05");
    expect(addYears("2028-02-29", 1)).toBe("2029-02-28");
    expect(addYears("2027-12-31", 1)).toBe("2028-12-31");
  });
});
