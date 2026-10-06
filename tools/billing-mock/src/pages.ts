// The mock's hosted pages: Polar's checkout (Pay, Decline, Abandon) and customer
// portal. Plain HTML; they stand in for Polar's own pages and are never shown on
// convt.app.

const escape = (s: string) => s.replace(/[&<>"']/g, (c) => `&#${c.charCodeAt(0)};`);

const shell = (title: string, body: string) =>
  new Response(
    `<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>${escape(title)}</title>
<style>
:root{color-scheme:light dark;--bg:#f6f6f4;--card:#fff;--ink:#18181b;--ink2:#6b6b70;--line:#e2e2df;--accent:#2a5bd7}
@media (prefers-color-scheme:dark){:root{--bg:#111113;--card:#1b1b1e;--ink:#ededef;--ink2:#9a9aa2;--line:#2c2c31;--accent:#7aa2ff}}
*{box-sizing:border-box}body{margin:0;background:var(--bg);color:var(--ink);font:14px/20px -apple-system,"Segoe UI",Helvetica,Arial,sans-serif}
main{max-width:440px;margin:40px auto;padding:0 16px}.card{background:var(--card);border:1px solid var(--line);border-radius:12px;padding:24px}
h1{font-size:18px;margin:0 0 4px}.muted{color:var(--ink2)}.price{font-size:28px;font-weight:600;margin:16px 0 4px}
label{display:block;margin:16px 0 6px;font-weight:500}input,select{width:100%;padding:9px 10px;border:1px solid var(--line);border-radius:8px;background:transparent;color:inherit;font:inherit}
.row{display:flex;gap:8px;margin-top:20px;flex-wrap:wrap}button,.btn{flex:1;padding:10px 12px;border-radius:8px;border:1px solid var(--line);background:transparent;color:inherit;font:inherit;cursor:pointer;text-align:center;text-decoration:none}
button.primary{background:var(--accent);border-color:var(--accent);color:#fff}.note{margin-top:14px;padding:10px 12px;border-radius:8px;border:1px solid var(--line)}
.badge{display:inline-block;font-size:11px;letter-spacing:.04em;padding:1px 6px;border:1px solid var(--line);border-radius:6px;margin-left:6px}
</style></head><body><main><p class="muted" style="margin:0 0 12px">Polar (local billing mock)</p>${body}</main></body></html>`,
    { headers: { "content-type": "text/html; charset=utf-8", "cache-control": "no-store" } },
  );

export type CheckoutView = {
  secret: string;
  productName: string;
  priceLabel: string;
  trialLabel: string | null;
  email: string;
  emailLocked: boolean;
  status: string;
  message: string | null;
  metered: boolean;
};

export function checkoutPage(v: CheckoutView): Response {
  if (v.status !== "open") {
    return shell(
      "Checkout",
      `<div class="card"><h1>This checkout is ${escape(v.status)}</h1><p class="muted">Start again from convt.</p></div>`,
    );
  }
  return shell(
    `Checkout: ${v.productName}`,
    `<div class="card"><h1>${escape(v.productName)}</h1>
<p class="price">${escape(v.priceLabel)}</p>
${v.trialLabel ? `<p class="muted">${escape(v.trialLabel)}</p>` : ""}
${v.metered ? `<p class="muted">No charge today. Usage is billed monthly.</p>` : ""}
${v.message ? `<p class="note" role="alert">${escape(v.message)}</p>` : ""}
<form method="post" action="/checkout/${escape(v.secret)}/confirm">
<label for="email">Email</label>
<input id="email" name="email" type="email" required value="${escape(v.email)}" ${v.emailLocked ? "readonly" : ""}>
<label for="card">Card</label>
<select id="card" name="card"><option value="4242">Visa ending 4242 (succeeds)</option><option value="0002">Visa ending 0002 (declined)</option></select>
<div class="row">
<button class="primary" type="submit" name="action" value="pay">${v.trialLabel ? "Start trial" : v.metered ? "Save card" : "Pay"}</button>
<button type="submit" name="action" value="decline">Decline</button>
<button type="submit" name="action" value="abandon" formnovalidate>Abandon</button>
</div></form></div>`,
  );
}

export function abandonedPage(): Response {
  return shell(
    "Checkout abandoned",
    `<div class="card"><h1>Checkout abandoned</h1><p class="muted">Nothing was charged. Close this tab or go back to convt.</p></div>`,
  );
}

export function failedPage(message: string): Response {
  return shell(
    "Checkout failed",
    `<div class="card"><h1>Checkout failed</h1><p class="note" role="alert">${escape(message)}</p></div>`,
  );
}

export type PortalView = {
  token: string;
  email: string;
  card: string | null;
  subscriptions: Array<{ name: string; status: string; renews: string }>;
  returnUrl: string | null;
  message: string | null;
};

export function portalPage(v: PortalView): Response {
  return shell(
    "Customer portal",
    `<div class="card"><h1>Customer portal</h1><p class="muted">${escape(v.email)}</p>
${v.message ? `<p class="note" role="status">${escape(v.message)}</p>` : ""}
<label>Payment method</label><p>${v.card ? escape(v.card) : "No card on file"}</p>
<form method="post" action="/portal/${escape(v.token)}/card"><div class="row"><button class="primary" type="submit">Update card</button></div></form>
<label>Subscriptions</label>
${v.subscriptions.length ? v.subscriptions.map((s) => `<p>${escape(s.name)}<span class="badge">${escape(s.status)}</span><br><span class="muted">${escape(s.renews)}</span></p>`).join("") : `<p class="muted">None</p>`}
${v.returnUrl ? `<div class="row"><a class="btn" href="${escape(v.returnUrl)}">Back to convt</a></div>` : ""}
</div>`,
  );
}
