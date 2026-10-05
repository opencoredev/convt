// Formatting helpers. Dates are ISO strings and are formatted in UTC so the server
// render and the browser agree.

const dateFormat = new Intl.DateTimeFormat("en-US", {
  month: "short",
  day: "numeric",
  year: "numeric",
  timeZone: "UTC",
});

const shortDateFormat = new Intl.DateTimeFormat("en-US", {
  month: "short",
  day: "numeric",
  timeZone: "UTC",
});

const numberFormat = new Intl.NumberFormat("en-US");

const moneyFormat = new Intl.NumberFormat("en-US", { style: "currency", currency: "USD" });

/** "Oct 2, 2027" */
export function formatDate(iso: string) {
  return dateFormat.format(new Date(iso));
}

/** "Oct 1" */
export function formatShortDate(iso: string) {
  return shortDateFormat.format(new Date(iso));
}

/** "SEP 3", for chart axes. */
export function formatAxisDate(iso: string) {
  return formatShortDate(iso).toUpperCase();
}

export function formatNumber(n: number) {
  return numberFormat.format(n);
}

export function formatMoney(cents: number) {
  return moneyFormat.format(cents / 100);
}

export function plural(n: number, one: string, many = `${one}s`) {
  return `${n} ${n === 1 ? one : many}`;
}
