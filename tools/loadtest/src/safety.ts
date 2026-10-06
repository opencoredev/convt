export function localUrl(value: string): string {
  const url = new URL(value);
  if (!["127.0.0.1", "localhost", "[::1]"].includes(url.hostname))
    throw Error("Load test refuses non-loopback targets");
  return value.replace(/\/$/, "");
}

/** Validate every hop, including storage signatures, with one overall timeout. */
export async function localFetch(value: string, init: RequestInit = {}, timeoutMs = 30_000) {
  const signal = AbortSignal.timeout(timeoutMs);
  let url = value;
  let options = { ...init };
  for (let hop = 0; hop <= 5; hop++) {
    localUrl(url);
    if (!["http:", "https:"].includes(new URL(url).protocol)) throw Error("Expected HTTP target");
    const response = await fetch(url, { ...options, signal, redirect: "manual" });
    if (![301, 302, 303, 307, 308].includes(response.status) || !response.headers.has("location"))
      return response;
    await response.body?.cancel();
    const next = new URL(response.headers.get("location")!, url).href;
    localUrl(next);
    const headers = new Headers(options.headers);
    if (new URL(next).origin !== new URL(url).origin) {
      headers.delete("authorization");
      headers.delete("cookie");
    }
    if (
      response.status === 303 ||
      ([301, 302].includes(response.status) && options.method?.toUpperCase() === "POST")
    ) {
      options = { ...options, method: "GET", body: undefined };
      headers.delete("content-length");
      headers.delete("content-type");
    }
    options.headers = headers;
    url = next;
  }
  throw Error("Too many local redirects");
}

export type Receipt = {
  external_id: string;
  external_customer_id: string;
  metadata: { quantity: number };
};
export type Usage = { job_id: string; user_id: string; reported_at: unknown };
export function meterMatches(receipts: Receipt[], usage: Usage[], customers: string[]) {
  // Include unexpected IDs for these customers, so additional charges cannot hide.
  const owned = receipts.filter((r) => customers.includes(r.external_customer_id));
  return (
    owned.length === usage.length &&
    new Set(owned.map((r) => r.external_id)).size === usage.length &&
    owned.every(
      (r) =>
        r.metadata.quantity === 1 &&
        usage.some(
          (u) =>
            u.job_id === r.external_id &&
            u.user_id === r.external_customer_id &&
            Boolean(u.reported_at),
        ),
    )
  );
}
