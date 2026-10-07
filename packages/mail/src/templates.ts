// Transactional email templates. Each takes frozen inputs and returns the subject,
// a plain-text body and minimal escaped HTML. Polar sends receipts as merchant of
// record, so these cover only what Polar does not: the key, the trial ending, a
// failed renewal, and Leo's daily digest. The outbox stores the rendered result,
// so a later template change never alters an email that is already queued.

/** Bump when any template's output changes. Stored with each frozen payload. */
export const templateVersion = 2;

export type Rendered = { subject: string; text: string; html: string };

const dateFormat = new Intl.DateTimeFormat("en-US", {
  month: "short",
  day: "numeric",
  year: "numeric",
  timeZone: "UTC",
});

/** "Oct 5, 2026" from an ISO date or timestamp, in UTC. */
export function formatDay(iso: string): string {
  return dateFormat.format(new Date(iso.length === 10 ? `${iso}T00:00:00Z` : iso));
}

export function formatCents(cents: number): string {
  return new Intl.NumberFormat("en-US", { style: "currency", currency: "USD" }).format(cents / 100);
}

export function escapeHtml(text: string): string {
  return text.replace(/[&<>"']/g, (c) => `&#${c.charCodeAt(0)};`);
}

export function activationUrl(token: string): string {
  return `convt://activate?key=${encodeURIComponent(token)}`;
}

/** Mail clients that honor prefers-color-scheme get the site's dark colors. */
const darkCss =
  "@media (prefers-color-scheme: dark){body{background:#141414 !important;color:#ededeb !important}" +
  ".key{border-color:#3a3a38 !important;background:#1d1d1c !important}a{color:#4cc38a !important}}";

type Block =
  | { p: string }
  | { key: string }
  | { link: { href: string; label: string } }
  | { list: string[] };

/** Text and HTML from the same blocks, so the two never drift. */
function render(subject: string, blocks: Block[]): Rendered {
  const text: string[] = [];
  const html: string[] = [];
  for (const b of blocks) {
    if ("p" in b) {
      text.push(b.p);
      html.push(`<p style="margin:0 0 14px">${escapeHtml(b.p)}</p>`);
    } else if ("key" in b) {
      text.push(b.key);
      html.push(
        `<p class="key" style="margin:0 0 14px;padding:12px;border:1px solid #d9d9d6;border-radius:8px;font-family:ui-monospace,Menlo,monospace;font-size:12px;word-break:break-all">${escapeHtml(b.key)}</p>`,
      );
    } else if ("link" in b) {
      text.push(`${b.link.label}: ${b.link.href}`);
      html.push(
        `<p style="margin:0 0 14px"><a href="${escapeHtml(b.link.href)}" style="color:#1a7f4b;font-weight:600">${escapeHtml(b.link.label)}</a></p>`,
      );
    } else {
      text.push(b.list.map((l) => `- ${l}`).join("\n"));
      html.push(
        `<ul style="margin:0 0 14px;padding-left:18px">${b.list.map((l) => `<li>${escapeHtml(l)}</li>`).join("")}</ul>`,
      );
    }
  }
  return {
    subject,
    text: `${text.join("\n\n")}\n`,
    html: `<!doctype html><html><head><meta name="color-scheme" content="light dark"><meta name="supported-color-schemes" content="light dark"><style>${darkCss}</style></head><body style="margin:0;padding:24px;font-family:-apple-system,'Segoe UI',Helvetica,Arial,sans-serif;font-size:14px;line-height:20px;color:#1c1c1a;background:#ffffff"><div style="max-width:520px"><p style="margin:0 0 20px;font-size:17px;font-weight:600;letter-spacing:-0.02em">convt</p>${html.join("")}</div></body></html>`,
  };
}

export type LicenseIssuedInput = {
  product: "desktop" | "pro";
  token: string;
  /** `YYYY-MM-DD`. */
  updatesUntil: string;
  siteUrl: string;
  downloadUrl: string;
};

export function licenseIssued(input: LicenseIssuedInput): Rendered {
  const product = input.product === "pro" ? "convt Pro" : "convt Desktop";
  const until = formatDay(input.updatesUntil);
  const window =
    input.product === "pro"
      ? `This key covers every build released up to ${until}. While you're subscribed, a new key appears on your dashboard each billing period.`
      : `This key works forever with every build released up to ${until}, and includes 12 months of updates.`;
  return render(`Your ${product} license key`, [
    { p: `Thanks for buying ${product}. Here is your license key:` },
    { key: input.token },
    { link: { href: activationUrl(input.token), label: "Open in convt" } },
    {
      p: "That link opens the convt app and asks you to confirm. You can also paste the key in the app under Settings, or run convt license activate with it.",
    },
    { p: window },
    { link: { href: input.downloadUrl, label: "Download convt" } },
    { link: { href: `${input.siteUrl}/dashboard/licenses`, label: "Your licenses" } },
    { p: "Keep this email. Anyone with the key can activate convt." },
  ]);
}

export type TrialEndingInput = {
  /** ISO timestamp. */
  trialEndsAt: string;
  amountCents: number;
  interval: "month" | "year";
  siteUrl: string;
};

export function trialEnding(input: TrialEndingInput): Rendered {
  const when = formatDay(input.trialEndsAt);
  const price = `${formatCents(input.amountCents)} a ${input.interval}`;
  return render(`Your convt Pro trial ends ${when}`, [
    { p: `Your free trial of convt Pro ends on ${when}.` },
    {
      p: `Then your card is charged ${price}, and you get your Pro license key for the desktop app.`,
    },
    {
      p: "If you don't want to continue, cancel before then. Nothing is charged and the trial simply ends.",
    },
    { link: { href: `${input.siteUrl}/dashboard/billing`, label: "Manage your plan" } },
  ]);
}

export type RenewalFailedInput = {
  kind: "pro" | "api";
  /** ISO timestamp of the period whose payment failed. */
  periodStart: string;
  siteUrl: string;
};

export function renewalFailed(input: RenewalFailedInput): Rendered {
  const what = input.kind === "pro" ? "your convt Pro renewal" : "your convt API usage";
  const stops =
    input.kind === "pro"
      ? "Your current license key keeps working for every build it covers, but you won't get a new key until the payment goes through."
      : "New API conversions are refused until the payment goes through. Your API keys stay as they are.";
  return render(`We couldn't charge your card for convt`, [
    { p: `The payment for ${what} didn't go through. We'll try again over the next few days.` },
    { p: stops },
    { p: "Update your card to fix it. Open Billing and choose Manage billing." },
    { link: { href: `${input.siteUrl}/dashboard/billing`, label: "Billing" } },
  ]);
}

export type DownloadLinkInput = {
  downloadUrl: string;
  /** The launch discount while it runs; omitted once it has ended. */
  offer?: { code: string; terms: string; endsLabel: string };
};

/** Sent when a phone visitor asks for the download link (not through the outbox). */
export function downloadLink(input: DownloadLinkInput): Rendered {
  const offer: Block[] = input.offer
    ? [
        {
          p: `Use code ${input.offer.code} at checkout for ${input.offer.terms}. The code works through ${input.offer.endsLabel}.`,
        },
      ]
    : [];
  return render("Your convt download link", [
    { p: "Here's the download link you asked for. Open it on your computer:" },
    { link: { href: input.downloadUrl, label: "Download convt" } },
    {
      p: "convt converts images, video, audio and documents on your own computer, with a right-click. Every download starts a 7-day free trial.",
    },
    ...offer,
    {
      p: "You got this because someone entered this address on convt.app. We won't email you again about it.",
    },
  ]);
}

export type AlertDigestInput = {
  /** `YYYY-MM-DD`. */
  date: string;
  counts: Record<string, number>;
  items: Array<{ kind: string; subject: string; detail: string }>;
};

export function alertDigest(input: AlertDigestInput): Rendered {
  const counts = Object.entries(input.counts)
    .filter(([, n]) => n > 0)
    .map(([k, n]) => `${k}: ${n}`);
  return render(`convt billing: ${input.items.length} to look at (${input.date})`, [
    { p: `The billing reconciler found these on ${formatDay(input.date)}.` },
    { list: counts.length ? counts : ["nothing counted"] },
    { list: input.items.slice(0, 200).map((i) => `${i.kind} ${i.subject}: ${i.detail}`) },
    {
      p: "Ids only: look them up in the database or in Polar and Resend by id. No keys or bodies are included.",
    },
  ]);
}
