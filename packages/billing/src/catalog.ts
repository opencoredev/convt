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

export type Catalog = {
  env: CatalogEnv;
  currency: "usd";
  products: Record<CatalogProduct, CatalogEntry>;
  /** Switching between monthly and yearly takes effect at once, both ways. */
  switchPolicy: Record<ProProduct, Proration>;
  meterName: "api_conversion";
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
};

/** The same products in a Polar organization: only the ids differ from `local`. */
const withIds = (
  env: "sandbox" | "production",
  ids: Record<CatalogProduct, { productId: string; priceId: string }>,
): Catalog => ({
  ...local,
  env,
  products: Object.fromEntries(
    Object.entries(local.products).map(([k, v]) => [k, { ...v, ...ids[k as CatalogProduct] }]),
  ) as Catalog["products"],
});

// The Polar sandbox organization "convt" (c9b2ccbd-28a8-4f07-984b-66b59c06a410).
const sandbox = withIds("sandbox", {
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
});

// The production organization "convt" (6098e410-4ea7-48fd-b677-7b261b8e0f7c).
const production = withIds("production", {
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
});

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
  if (
    c.env !== "local" &&
    Object.values(c.products).some((e) => e.productId.startsWith("PLACEHOLDER"))
  )
    problems.push(`${c.env}: product ids are placeholders`);
  return problems;
}

/** Loads the catalog for an environment, refusing one with an invalid price. */
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
  return null;
}

export function productByPriceId(c: Catalog, priceId: string | null): CatalogProduct | null {
  if (!priceId) return null;
  for (const [k, v] of Object.entries(c.products))
    if (v.priceId === priceId) return k as CatalogProduct;
  return null;
}

export const isPro = (p: CatalogProduct | null): p is ProProduct =>
  p === "pro_month" || p === "pro_year";
