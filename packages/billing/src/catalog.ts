// The products convt sells, per environment: the provider's product and price ids,
// amounts and intervals, and the switch policy. Only these products create
// entitlements. The sandbox and production ids come from the "convt" Polar
// organizations (docs/p7-billing-plan.md, "Accounts and secrets").

export type CatalogEnv = "local" | "sandbox" | "production";
export type CatalogProduct = "desktop" | "pro_month" | "pro_year" | "api";
export type ProProduct = "pro_month" | "pro_year";
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
  legacyDesktop?: DesktopProductIds[];
};

export type DesktopProductIds = { productId: string; priceId: string };

// The Product Hunt launch offer: 30% off Desktop, or off the first 3 months of Pro
// monthly, until the end of 31 October 2026 Pacific time (the code's ends_at in Polar).
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
      priceId: "price_local_desktop",
      amountCents: 2900,
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
};

/** The same products in a Polar organization: only the ids differ from `local`. */
const withIds = (
  env: "sandbox" | "production",
  ids: Record<CatalogProduct, { productId: string; priceId: string }>,
  discounts: Catalog["discounts"],
): Catalog => ({
  ...local,
  env,
  discounts,
  products: Object.fromEntries(
    Object.entries(local.products).map(([k, v]) => [k, { ...v, ...ids[k as CatalogProduct] }]),
  ) as Catalog["products"],
});

// The Polar sandbox organization "convt" (c9b2ccbd-28a8-4f07-984b-66b59c06a410).
const sandbox = withIds(
  "sandbox",
  {
    desktop: {
      productId: "31a1a071-bec9-4b7e-a5dd-e9a52eb7d3ab",
      priceId: "20024212-c2be-4d23-819e-151b704a3a3e",
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
);

// The production organization "convt" (6098e410-4ea7-48fd-b677-7b261b8e0f7c).
const production = withIds(
  "production",
  {
    desktop: {
      productId: "9d3028c1-1e4f-4d12-9641-59960eb58679",
      priceId: "a68e7412-964d-4dfc-bd9b-7aa6353ae37f",
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
  if (
    c.env !== "local" &&
    Object.values(c.products).some((e) => e.productId.startsWith("PLACEHOLDER"))
  )
    problems.push(`${c.env}: product ids are placeholders`);
  return problems;
}

/** Loads the catalog for an environment, refusing one with an invalid price. */
export function loadCatalog(env: CatalogEnv, desktop?: DesktopProductIds): Catalog {
  const previous = catalogs[env].products.desktop;
  const c = desktop
    ? {
        ...catalogs[env],
        legacyDesktop: [{ productId: previous.productId, priceId: previous.priceId }],
        products: {
          ...catalogs[env].products,
          desktop: { ...catalogs[env].products.desktop, ...desktop },
        },
      }
    : catalogs[env];
  const problems = validateCatalog(c).filter((p) => !p.includes("placeholders"));
  if (problems.length) throw new Error(`the ${env} catalog is invalid: ${problems.join("; ")}`);
  return c;
}

export function productByProviderId(c: Catalog, productId: string | null): CatalogProduct | null {
  if (!productId) return null;
  for (const [k, v] of Object.entries(c.products))
    if (v.productId === productId) return k as CatalogProduct;
  if (c.legacyDesktop?.some((p) => p.productId === productId)) return "desktop";
  return null;
}

export function productByPriceId(c: Catalog, priceId: string | null): CatalogProduct | null {
  if (!priceId) return null;
  for (const [k, v] of Object.entries(c.products))
    if (v.priceId === priceId) return k as CatalogProduct;
  if (c.legacyDesktop?.some((p) => p.priceId === priceId)) return "desktop";
  return null;
}

export const isPro = (p: CatalogProduct | null): p is ProProduct =>
  p === "pro_month" || p === "pro_year";

/** Polar line items used when deciding whether a Desktop order charged nothing. */
export type ComplimentaryDesktopAmounts = {
  netCents: number;
  subtotalCents: number;
  discountCents: number;
  appliedBalanceCents?: number;
  items: Array<{ priceId: string | null; amountCents: number }>;
};

/**
 * A Polar-signed Desktop order that took no money: a 100% code, Polar zeroing the
 * line, or store credit covering the list price. Partial unknown discounts are not.
 */
export function complimentaryDesktop(c: Catalog, o: ComplimentaryDesktopAmounts): boolean {
  const price = c.products.desktop;
  if (o.netCents !== 0) return false;
  const acceptedPrices = new Set([price.priceId, ...(c.legacyDesktop ?? []).map((p) => p.priceId)]);
  if (o.items.length !== 1 || !acceptedPrices.has(o.items[0].priceId ?? "")) return false;
  const item = o.items[0].amountCents;
  if (item !== price.amountCents && item !== 0) return false;
  if (o.subtotalCents !== price.amountCents && o.subtotalCents !== 0) return false;
  if (o.subtotalCents === 0) return o.discountCents === 0;
  return o.discountCents + (o.appliedBalanceCents ?? 0) >= o.subtotalCents;
}

/**
 * Why a discount on `product` is not acceptable, or null. `discountCents` is checked
 * against the code's percentage of `subtotalCents` when given (orders only).
 * A complimentary Desktop write-off is accepted even when Polar's discount id is
 * not in the catalog (giveaway codes, 100% coupons).
 */
export function discountProblem(
  c: Catalog,
  discountId: string | null,
  product: CatalogProduct,
  amounts?: ComplimentaryDesktopAmounts,
): string | null {
  if (product === "desktop" && amounts && complimentaryDesktop(c, amounts)) return null;
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
