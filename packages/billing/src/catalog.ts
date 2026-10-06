// The products convt sells, per environment: the provider's product and price ids,
// amounts and intervals, and the switch policy. Only these products create
// entitlements. Leo fills in the sandbox and production ids once the Polar
// organizations exist (docs/p7-billing-plan.md, "Accounts and secrets").

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

/** PLACEHOLDER ids: Leo creates the products in Polar and replaces these. */
const placeholder = (env: "sandbox" | "production"): Catalog => ({
  ...local,
  env,
  products: Object.fromEntries(
    Object.entries(local.products).map(([k, v]) => [
      k,
      {
        ...v,
        productId: `PLACEHOLDER_${env}_${k}_product`,
        priceId: `PLACEHOLDER_${env}_${k}_price`,
      },
    ]),
  ) as Catalog["products"],
});

const catalogs: Record<CatalogEnv, Catalog> = {
  local,
  sandbox: placeholder("sandbox"),
  production: placeholder("production"),
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
