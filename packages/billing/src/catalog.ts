// The products convt sells, per environment: the provider's product and price ids,
// amounts and intervals, and the switch policy. Only these products create
// entitlements. The sandbox and production ids come from the "convt" Polar
// organizations (docs/p7-billing-plan.md, "Accounts and secrets").

export type CatalogEnv = "local" | "sandbox" | "production";
export type CatalogProduct = "desktop" | "bundle" | "pro_month" | "pro_year" | "api";
export type ProProduct = "pro_month" | "pro_year";
/** Products sold for one payment: an order, never a subscription. */
export type OneTimeProduct = "desktop" | "bundle";

/** How long the cloud year of one Desktop + Cloud order lasts. */
export const BUNDLE_CLOUD_DAYS = 365;
export type Proration = "invoice" | "next_period";

export type CatalogEntry = {
  productId: string;
  priceId: string;
  /** Fixed price in whole cents; null for the metered API product. */
  amountCents: number | null;
  /** Metered unit price in cents, as the provider's decimal string. */
  unitAmount: string | null;
  interval: "month" | "year" | null;
  trialDays: number | null;
};

/** A percentage discount code billing accepts on the listed products. */
export type CatalogDiscount = {
  code: string;
  basisPoints: number;
  products: CatalogProduct[];
};

export type Catalog = {
  env: CatalogEnv;
  currency: "usd";
  products: Record<CatalogProduct, CatalogEntry>;
  /** By the provider's discount id. An order or subscription with any other discount is rejected. */
  discounts: Record<string, CatalogDiscount>;
  /** Switching between monthly and yearly takes effect at once, both ways. */
  switchPolicy: Record<ProProduct, Proration>;
  meterName: "api_conversion";
  /**
   * Prices no longer sold that past orders still carry, each at the amount it
   * charged. Refunds, disputes and the backfill refetch those orders.
   */
  legacyPrices: LegacyPrice[];
};

export type LegacyPrice = {
  product: OneTimeProduct;
  productId: string;
  priceId: string;
  amountCents: number;
};

// The Product Hunt launch offer: 30% off Desktop, or off the first 3 months of Pro
// monthly, until the end of 31 October 2026 Pacific time (the code's ends_at in Polar).
// It stays listed so the launch orders it discounted still validate when refetched.
const productHunt: CatalogDiscount = {
  code: "PRODUCTHUNT",
  basisPoints: 3000,
  products: ["desktop", "pro_month"],
};

const local: Catalog = {
  env: "local",
  currency: "usd",
  products: {
    desktop: {
      productId: "prod_local_desktop",
      priceId: "price_local_desktop_39",
      amountCents: 3900,
      unitAmount: null,
      interval: null,
      trialDays: null,
    },
    bundle: {
      productId: "prod_local_bundle",
      priceId: "price_local_bundle",
      amountCents: 6900,
      unitAmount: null,
      interval: null,
      trialDays: null,
    },
    pro_month: {
      productId: "prod_local_pro_month",
      priceId: "price_local_pro_month",
      amountCents: 1200,
      unitAmount: null,
      interval: "month",
      trialDays: 7,
    },
    pro_year: {
      productId: "prod_local_pro_year",
      priceId: "price_local_pro_year",
      amountCents: 9600,
      unitAmount: null,
      interval: "year",
      trialDays: 7,
    },
    api: {
      productId: "prod_local_api",
      priceId: "price_local_api",
      amountCents: null,
      // PLACEHOLDER: one cent per conversion until P9 sets a real price.
      unitAmount: "1",
      interval: "month",
      trialDays: null,
    },
  },
  switchPolicy: { pro_month: "invoice", pro_year: "invoice" },
  meterName: "api_conversion",
  discounts: { disc_local_producthunt: productHunt },
  // The $29 Desktop price, sold until CNV-101.
  legacyPrices: [
    {
      product: "desktop",
      productId: "prod_local_desktop",
      priceId: "price_local_desktop",
      amountCents: 2900,
    },
  ],
};

/** The same products in a Polar organization: only the ids differ from `local`. */
const withIds = (
  env: "sandbox" | "production",
  ids: Record<CatalogProduct, { productId: string; priceId: string }>,
  discounts: Catalog["discounts"],
  legacyPrices: LegacyPrice[],
): Catalog => ({
  ...local,
  env,
  discounts,
  legacyPrices,
  products: Object.fromEntries(
    Object.entries(local.products).map(([k, v]) => [k, { ...v, ...ids[k as CatalogProduct] }]),
  ) as Catalog["products"],
});

// The Polar sandbox organization "convt" (c9b2ccbd-28a8-4f07-984b-66b59c06a410).
const sandbox = withIds(
  "sandbox",
  {
    // PLACEHOLDER until the sandbox org has the $39 price and the bundle product
    // (docs/plans/cnv-101-one-time-pricing.md, "Production rollout").
    desktop: {
      productId: "31a1a071-bec9-4b7e-a5dd-e9a52eb7d3ab",
      priceId: "PLACEHOLDER_SANDBOX_DESKTOP_39_PRICE",
    },
    bundle: {
      productId: "PLACEHOLDER_SANDBOX_BUNDLE_PRODUCT",
      priceId: "PLACEHOLDER_SANDBOX_BUNDLE_PRICE",
    },
    pro_month: {
      productId: "274ac0a7-b561-4a2b-a378-cbefc994e558",
      priceId: "4f9c498a-7c19-468e-b40a-99c77a6de2dc",
    },
    pro_year: {
      productId: "b93eb48c-0940-431e-870a-8ab8eff3f1de",
      priceId: "bde32120-f1a6-4f9d-be04-7b358d2f0556",
    },
    api: {
      productId: "1d42d41b-15e6-4b21-81ce-8e4f2d0dfb3d",
      priceId: "4922ee3c-ee15-46fd-9aaa-bc25922fb9a9",
    },
  },
  { "cbc510ac-177c-450b-b5ca-26bcd8a345a7": productHunt },
  [
    {
      product: "desktop",
      productId: "31a1a071-bec9-4b7e-a5dd-e9a52eb7d3ab",
      priceId: "20024212-c2be-4d23-819e-151b704a3a3e",
      amountCents: 2900,
    },
  ],
);

// The production organization "convt" (6098e410-4ea7-48fd-b677-7b261b8e0f7c).
const production = withIds(
  "production",
  {
    // PLACEHOLDER until Leo creates the $39 price and the bundle product in the
    // production org after CNV-101 merges; the billing deploy refuses placeholders.
    desktop: {
      productId: "9d3028c1-1e4f-4d12-9641-59960eb58679",
      priceId: "PLACEHOLDER_PRODUCTION_DESKTOP_39_PRICE",
    },
    bundle: {
      productId: "PLACEHOLDER_PRODUCTION_BUNDLE_PRODUCT",
      priceId: "PLACEHOLDER_PRODUCTION_BUNDLE_PRICE",
    },
    pro_month: {
      productId: "366bad05-07e3-41ca-b620-ce3d1ae5db3c",
      priceId: "ba25c579-8e8d-4e90-9536-02e291822979",
    },
    pro_year: {
      productId: "57e8ab1a-2ec5-4c9c-8f6d-2095bc01855d",
      priceId: "1ea3fe0a-2145-417c-b8ed-347016b0f2a8",
    },
    api: {
      productId: "34e9b296-afb4-4880-a87c-fca1be914f36",
      priceId: "4c5fdab8-63de-400e-a353-efba78c85820",
    },
  },
  {
    // Recorded when the catalog was filled. Polar also issues
    // 8d401db5-… for the same PRODUCTHUNT code; both must be accepted or
    // launch checkouts hang on /checkout/success and never grant Pro.
    "64641dd4-73ae-4704-8fbf-450bed2b2aa4": productHunt,
    "8d401db5-99d3-44c2-82e9-6483dec9ced7": productHunt,
    "17bb47c4-8b7b-4fb5-b013-31ad52a0e909": {
      code: "FAMILY",
      basisPoints: 10000,
      products: ["pro_month"],
    },
    "8a134038-3319-4905-a893-8635b0fd7cd7": {
      code: "ZORTOS_DISCORD",
      basisPoints: 10000,
      products: ["desktop"],
    },
  },
  [
    {
      product: "desktop",
      productId: "9d3028c1-1e4f-4d12-9641-59960eb58679",
      priceId: "a68e7412-964d-4dfc-bd9b-7aa6353ae37f",
      amountCents: 2900,
    },
  ],
);

const catalogs: Record<CatalogEnv, Catalog> = {
  local,
  sandbox,
  production,
};

/**
 * A metered price must be a whole number of cents of at least 1 until P9 defines
 * sub-cent precision and rounding. Returns the problem, or null.
 */
export function meteredPriceProblem(unitAmount: string | null): string | null {
  if (unitAmount === null) return "the metered price has no unit amount";
  if (!/^\d+(\.0+)?$/.test(unitAmount.trim()))
    return `unit amount ${unitAmount} is not a whole number of cents`;
  if (Number(unitAmount) < 1) return `unit amount ${unitAmount} is below one cent`;
  return null;
}

export function validateCatalog(c: Catalog): string[] {
  const problems: string[] = [];
  for (const [name, e] of Object.entries(c.products)) {
    if (name === "api") {
      const p = meteredPriceProblem(e.unitAmount);
      if (p) problems.push(`api: ${p}`);
    } else if (!Number.isInteger(e.amountCents) || (e.amountCents ?? 0) <= 0) {
      problems.push(`${name}: the price must be a positive whole number of cents`);
    }
  }
  for (const [id, d] of Object.entries(c.discounts)) {
    if (!Number.isInteger(d.basisPoints) || d.basisPoints <= 0 || d.basisPoints > 10000)
      problems.push(`discount ${id}: basis points must be between 1 and 10000`);
    if (d.products.includes("api")) problems.push(`discount ${id}: the API is never discounted`);
  }
  const current = new Set(Object.values(c.products).map((e) => e.priceId));
  for (const l of c.legacyPrices) {
    if (!Number.isInteger(l.amountCents) || l.amountCents <= 0)
      problems.push(`legacy ${l.priceId}: the price must be a positive whole number of cents`);
    if (current.has(l.priceId)) problems.push(`legacy ${l.priceId}: still a current price`);
  }
  if (c.env !== "local" && placeholderIds(c).length)
    problems.push(`${c.env}: product ids are placeholders: ${placeholderIds(c).join(", ")}`);
  return problems;
}

/** Ids not yet filled in from the provider. A deploy with any of them is refused. */
export function placeholderIds(c: Catalog): string[] {
  return Object.values(c.products)
    .flatMap((e) => [e.productId, e.priceId])
    .filter((id) => id.startsWith("PLACEHOLDER"));
}

/**
 * Loads the catalog for an environment, refusing one with an invalid price.
 * Placeholders are allowed here so tests and local tools load every catalog;
 * `scripts/deploy-worker.sh` refuses to deploy one.
 */
export function loadCatalog(env: CatalogEnv): Catalog {
  const c = catalogs[env];
  const problems = validateCatalog(c).filter((p) => !p.includes("placeholders"));
  if (problems.length) throw new Error(`the ${env} catalog is invalid: ${problems.join("; ")}`);
  return c;
}

export function productByProviderId(c: Catalog, productId: string | null): CatalogProduct | null {
  if (!productId) return null;
  for (const [k, v] of Object.entries(c.products))
    if (v.productId === productId) return k as CatalogProduct;
  return c.legacyPrices.find((p) => p.productId === productId)?.product ?? null;
}

export function productByPriceId(c: Catalog, priceId: string | null): CatalogProduct | null {
  if (!priceId) return null;
  for (const [k, v] of Object.entries(c.products))
    if (v.priceId === priceId) return k as CatalogProduct;
  return c.legacyPrices.find((p) => p.priceId === priceId)?.product ?? null;
}

export const isPro = (p: CatalogProduct | null): p is ProProduct =>
  p === "pro_month" || p === "pro_year";

export const isOneTime = (p: CatalogProduct | null): p is OneTimeProduct =>
  p === "desktop" || p === "bundle";

/**
 * The list price `priceId` charges for `product`: the current price or a legacy
 * one at its own recorded amount. Null when the price is not one of the product's.
 */
export function listPriceOf(c: Catalog, product: OneTimeProduct, priceId: string | null) {
  if (!priceId) return null;
  const current = c.products[product];
  if (current.priceId === priceId) return current.amountCents;
  return (
    c.legacyPrices.find((p) => p.product === product && p.priceId === priceId)?.amountCents ?? null
  );
}

/** Polar line items used when deciding whether a one-time order charged nothing. */
export type ComplimentaryAmounts = {
  netCents: number;
  subtotalCents: number;
  discountCents: number;
  appliedBalanceCents?: number;
  items: Array<{ priceId: string | null; amountCents: number }>;
};

/**
 * A Polar-signed one-time order that took no money: a 100% code, Polar zeroing the
 * line, or store credit covering the list price. Partial unknown discounts are not.
 */
export function complimentaryOrder(
  c: Catalog,
  product: OneTimeProduct,
  o: ComplimentaryAmounts,
): boolean {
  if (o.netCents !== 0 || o.items.length !== 1) return false;
  const price = listPriceOf(c, product, o.items[0].priceId);
  if (price === null) return false;
  const item = o.items[0].amountCents;
  if (item !== price && item !== 0) return false;
  if (o.subtotalCents !== price && o.subtotalCents !== 0) return false;
  if (o.subtotalCents === 0) return o.discountCents === 0;
  return o.discountCents + (o.appliedBalanceCents ?? 0) >= o.subtotalCents;
}

/**
 * Why a discount on `product` is not acceptable, or null. `discountCents` is checked
 * against the code's percentage of `subtotalCents` when given (orders only).
 * A complimentary one-time write-off is accepted even when Polar's discount id is
 * not in the catalog (giveaway codes, 100% coupons).
 */
export function discountProblem(
  c: Catalog,
  discountId: string | null,
  product: CatalogProduct,
  amounts?: ComplimentaryAmounts,
): string | null {
  if (isOneTime(product) && amounts && complimentaryOrder(c, product, amounts)) return null;
  if (!discountId)
    return amounts && amounts.discountCents !== 0 ? "a discount without a code" : null;
  const d = c.discounts[discountId];
  if (!d) return `unknown discount ${discountId}`;
  if (!d.products.includes(product)) return `${d.code} does not apply to ${product}`;
  if (amounts) {
    const expected = Math.round((amounts.subtotalCents * d.basisPoints) / 10000);
    if (Math.abs(amounts.discountCents - expected) > 1)
      return `${d.code} on ${amounts.subtotalCents} is ${expected}, the order took ${amounts.discountCents}`;
  }
  return null;
}
