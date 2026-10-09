// Campaign and lifecycle email for Sequenzy. These are not the transactional
// templates in templates.ts: those go out through the outbox, one per event, and
// never carry an unsubscribe footer. These are pasted into Sequenzy campaigns and
// sequences (`bun run --cwd packages/mail export:marketing`), and every one ends
// with the footer below, which uses two Sequenzy merge tags:
//
//   {{unsubscribeUrl}}  Sequenzy's own one-click unsubscribe link
//   {{preferencesUrl}}  convt.app's signed preferences page, a custom attribute
//                       convt-billing sets on each contact
//
// Colors and type follow apps/web/src/styles.css: light by default, the site's
// dark palette for clients that honor prefers-color-scheme.

import { escapeHtml, type Rendered } from "./templates";

/** Bump when any marketing template's output changes. */
export const marketingTemplateVersion = 1;

export const unsubscribeTag = "{{unsubscribeUrl}}";
export const preferencesTag = "{{preferencesUrl}}";

const color = {
  page: "#f7f8f7",
  card: "#ffffff",
  line: "#e6e8e7",
  ink: "#0a0a0a",
  ink2: "#6b6f6d",
  ink3: "#6c716e",
  green: "#127a47",
  greenTint: "#eef7f2",
  greenLine: "#cfe6d9",
  button: "#16935a",
  code: "#0f1211",
  codeInk: "#e6e8e7",
};

const darkCss =
  "@media (prefers-color-scheme: dark){" +
  ".page{background:#0a0b0b !important}" +
  ".card{background:#111312 !important;border-color:#232726 !important}" +
  ".ink{color:#edefee !important}.ink2{color:#a1a6a3 !important}.ink3{color:#868b88 !important}" +
  ".note{background:#12261b !important;border-color:#1d3a2a !important}" +
  ".rule{border-color:#232726 !important}a.link{color:#3fcb84 !important}" +
  ".check{color:#3fcb84 !important}}" +
  "@media (max-width:600px){.card{border-radius:0 !important;border-left:0 !important;border-right:0 !important}" +
  ".pad{padding:28px 22px !important}.h1{font-size:24px !important;line-height:30px !important}}";

const font = "-apple-system,BlinkMacSystemFont,'Segoe UI',Helvetica,Arial,sans-serif";
const mono = "ui-monospace,SFMono-Regular,Menlo,Consolas,monospace";

export type MarketingBlock =
  | { kind: "p"; text: string }
  | { kind: "button"; href: string; label: string }
  | { kind: "checks"; items: string[] }
  | { kind: "steps"; items: Array<{ title: string; body: string }> }
  | { kind: "note"; title: string; body: string }
  | { kind: "code"; text: string }
  | { kind: "link"; href: string; label: string };

export type MarketingEmail = {
  subject: string;
  /** The inbox preview line. Hidden in the body. */
  preheader: string;
  heading: string;
  blocks: MarketingBlock[];
};

export type FooterInput = {
  siteUrl: string;
  /**
   * The sender's postal address. US law (CAN-SPAM) requires one in commercial
   * email; convt.app publishes none yet, so the export leaves a marker to fill in.
   */
  postalAddress: string;
};

export type MarketingRendered = Rendered & { preheader: string };

function blockHtml(b: MarketingBlock): string {
  const p = `margin:0 0 16px;font-size:15px;line-height:24px;color:${color.ink2}`;
  switch (b.kind) {
    case "p":
      return `<p class="ink2" style="${p}">${escapeHtml(b.text)}</p>`;
    case "button":
      return (
        `<table role="presentation" cellpadding="0" cellspacing="0" border="0" style="margin:8px 0 24px"><tr>` +
        `<td style="border-radius:10px;background:${color.button};background-image:linear-gradient(180deg,#1fb36c,#14864f)">` +
        `<a href="${escapeHtml(b.href)}" style="display:inline-block;padding:11px 18px;font-size:15px;line-height:18px;font-weight:600;color:#ffffff;text-decoration:none;border-radius:10px">${escapeHtml(b.label)}</a>` +
        `</td></tr></table>`
      );
    case "checks":
      return (
        `<table role="presentation" cellpadding="0" cellspacing="0" border="0" style="margin:0 0 20px">` +
        b.items
          .map(
            (item) =>
              `<tr><td class="check" valign="top" style="padding:0 10px 10px 0;font-size:15px;line-height:22px;color:${color.green};font-weight:600">&#10003;</td>` +
              `<td class="ink" style="padding:0 0 10px;font-size:15px;line-height:22px;color:${color.ink}">${escapeHtml(item)}</td></tr>`,
          )
          .join("") +
        `</table>`
      );
    case "steps":
      return (
        `<table role="presentation" cellpadding="0" cellspacing="0" border="0" style="margin:4px 0 8px;width:100%">` +
        b.items
          .map(
            (step, i) =>
              `<tr><td valign="top" style="padding:0 14px 16px 0;width:26px">` +
              `<div class="note" style="width:26px;height:26px;border-radius:13px;background:${color.greenTint};border:1px solid ${color.greenLine};font-size:13px;line-height:26px;text-align:center;font-weight:600;color:${color.green}">${i + 1}</div></td>` +
              `<td valign="top" style="padding:0 0 16px">` +
              `<p class="ink" style="margin:2px 0 2px;font-size:15px;line-height:22px;font-weight:600;color:${color.ink}">${escapeHtml(step.title)}</p>` +
              `<p class="ink2" style="margin:0;font-size:14px;line-height:21px;color:${color.ink2}">${escapeHtml(step.body)}</p></td></tr>`,
          )
          .join("") +
        `</table>`
      );
    case "note":
      return (
        `<div class="note" style="margin:4px 0 20px;padding:16px 18px;border-radius:12px;background:${color.greenTint};border:1px solid ${color.greenLine}">` +
        `<p class="ink" style="margin:0 0 4px;font-size:15px;line-height:22px;font-weight:600;color:${color.ink}">${escapeHtml(b.title)}</p>` +
        `<p class="ink2" style="margin:0;font-size:14px;line-height:21px;color:${color.ink2}">${escapeHtml(b.body)}</p></div>`
      );
    case "code":
      return `<p style="margin:0 0 20px;padding:12px 14px;border-radius:10px;background:${color.code};color:${color.codeInk};font-family:${mono};font-size:13px;line-height:20px">${escapeHtml(b.text)}</p>`;
    case "link":
      return `<p style="margin:0 0 16px;font-size:15px;line-height:24px"><a class="link" href="${escapeHtml(b.href)}" style="color:${color.green};font-weight:600;text-decoration:none">${escapeHtml(b.label)} &rarr;</a></p>`;
    default: {
      const _exhaustive: never = b;
      return _exhaustive;
    }
  }
}

function blockText(b: MarketingBlock): string {
  switch (b.kind) {
    case "p":
      return b.text;
    case "button":
    case "link":
      return `${b.label}: ${b.href}`;
    case "checks":
      return b.items.map((i) => `- ${i}`).join("\n");
    case "steps":
      return b.items.map((s, i) => `${i + 1}. ${s.title}\n   ${s.body}`).join("\n");
    case "note":
      return `${b.title}\n${b.body}`;
    case "code":
      return `    ${b.text}`;
    default: {
      const _exhaustive: never = b;
      return _exhaustive;
    }
  }
}

const footerReason =
  "You're getting this because you have a convt account. Account and license emails, like sign-in codes and keys, still arrive if you unsubscribe.";

/** Every marketing email goes through here, so none can leave out the footer. */
export function renderMarketing(email: MarketingEmail, footer: FooterInput): MarketingRendered {
  const site = footer.siteUrl.replace(/\/$/, "");
  const html =
    `<!doctype html><html lang="en"><head><meta charset="utf-8">` +
    `<meta name="viewport" content="width=device-width,initial-scale=1">` +
    `<meta name="color-scheme" content="light dark"><meta name="supported-color-schemes" content="light dark">` +
    `<title>${escapeHtml(email.subject)}</title><style>${darkCss}</style></head>` +
    `<body class="page" style="margin:0;padding:0;background:${color.page};font-family:${font};-webkit-font-smoothing:antialiased">` +
    `<div style="display:none;max-height:0;overflow:hidden;opacity:0">${escapeHtml(email.preheader)}&#8199;&#65279;&#847;&#8199;&#65279;&#847;&#8199;&#65279;&#847;</div>` +
    `<table role="presentation" class="page" width="100%" cellpadding="0" cellspacing="0" border="0" style="background:${color.page}"><tr><td align="center" style="padding:32px 12px">` +
    `<table role="presentation" width="100%" cellpadding="0" cellspacing="0" border="0" style="max-width:560px">` +
    `<tr><td style="padding:0 4px 18px">` +
    `<a href="${escapeHtml(site)}" style="text-decoration:none"><img src="${escapeHtml(site)}/brand/convt-app-icon.png" width="32" height="32" alt="" style="display:inline-block;vertical-align:middle;border:0">` +
    `<span class="ink" style="display:inline-block;vertical-align:middle;margin-left:6px;font-size:18px;line-height:32px;font-weight:600;letter-spacing:-0.03em;color:${color.ink}">convt</span></a></td></tr>` +
    `<tr><td class="card pad" style="background:${color.card};border:1px solid ${color.line};border-radius:16px;padding:36px 36px 20px">` +
    `<h1 class="ink h1" style="margin:0 0 14px;font-size:28px;line-height:34px;font-weight:600;letter-spacing:-0.035em;color:${color.ink}">${escapeHtml(email.heading)}</h1>` +
    email.blocks.map(blockHtml).join("") +
    `</td></tr>` +
    `<tr><td style="padding:22px 8px 0">` +
    `<p class="ink3" style="margin:0 0 10px;font-size:12px;line-height:18px;color:${color.ink3}">${escapeHtml(footerReason)}</p>` +
    `<p class="ink3" style="margin:0 0 10px;font-size:12px;line-height:18px;color:${color.ink3}">` +
    `<a class="link" href="${preferencesTag}" style="color:${color.ink3};text-decoration:underline">Email preferences</a> &nbsp;&middot;&nbsp; ` +
    `<a class="link" href="${unsubscribeTag}" style="color:${color.ink3};text-decoration:underline">Unsubscribe</a> &nbsp;&middot;&nbsp; ` +
    `<a class="link" href="${escapeHtml(site)}/privacy" style="color:${color.ink3};text-decoration:underline">Privacy</a></p>` +
    `<p class="ink3" style="margin:0;font-size:12px;line-height:18px;color:${color.ink3}">convt &middot; ${escapeHtml(footer.postalAddress)}</p>` +
    `</td></tr></table></td></tr></table></body></html>`;
  const text =
    [email.heading, ...email.blocks.map(blockText)].join("\n\n") +
    `\n\n--\n${footerReason}\nEmail preferences: ${preferencesTag}\nUnsubscribe: ${unsubscribeTag}\nconvt · ${footer.postalAddress}\n`;
  return { subject: email.subject, preheader: email.preheader, text, html };
}

/** Sent once by the signup sequence. */
export function welcomeEmail(siteUrl: string): MarketingEmail {
  const site = siteUrl.replace(/\/$/, "");
  return {
    subject: "Welcome to convt",
    preheader: "Right-click a file, pick a format. Here's how to start.",
    heading: "Convert any file with a right-click.",
    blocks: [
      {
        kind: "p",
        text: "Thanks for making a convt account. convt converts images, video, audio and documents on your own computer, so your files never get uploaded.",
      },
      {
        kind: "steps",
        items: [
          {
            title: "Download the app",
            body: "convt runs on macOS, Windows and Linux. Every format works offline.",
          },
          {
            title: "Right-click any file",
            body: "Choose Convert with convt and pick a format. A HEIC photo becomes a JPG, a MOV becomes an MP4.",
          },
          {
            title: "Or use the command line",
            body: "The same engine, for scripts and whole folders.",
          },
        ],
      },
      { kind: "code", text: "convt photos/ --to jpeg -r" },
      { kind: "button", href: `${site}/download`, label: "Download convt" },
      {
        kind: "p",
        text: "The app is free for 7 days from your first conversion. After that, Desktop is a one-time purchase with every future desktop update, or Pro adds cloud conversions from your phone or browser.",
      },
      { kind: "link", href: `${site}/formats`, label: "See every format" },
    ],
  };
}

/** For accounts that bought Desktop before it became lifetime. */
export function lifetimeDesktopEmail(siteUrl: string): MarketingEmail {
  const site = siteUrl.replace(/\/$/, "");
  return {
    subject: "Your convt Desktop license is now lifetime",
    preheader: "Every future desktop update, no renewal. Nothing for you to do.",
    heading: "Your Desktop license is now lifetime.",
    blocks: [
      {
        kind: "p",
        text: "When you bought convt Desktop, it came with a year of updates. We've changed that for everyone who paid: a Desktop license now includes every future desktop update, with no renewal.",
      },
      {
        kind: "note",
        title: "You don't need to do anything.",
        body: "Your current license key keeps working and now counts as lifetime. Every future desktop update works with it.",
      },
      {
        kind: "checks",
        items: [
          "Every format, offline",
          "Every future desktop update",
          "Batch folders and presets",
          "No subscription for the desktop app",
        ],
      },
      { kind: "button", href: `${site}/dashboard/licenses`, label: "View your license" },
      {
        kind: "p",
        text: "Thank you for buying convt early. If anything about your license looks wrong, tell us and we'll fix it.",
      },
      { kind: "link", href: `${site}/contact`, label: "Contact support" },
    ],
  };
}

/** The first email of the Pro trial sequence. The trial-ending notice stays transactional. */
export function proTrialEmail(siteUrl: string): MarketingEmail {
  const site = siteUrl.replace(/\/$/, "");
  return {
    subject: "Getting started with convt Pro",
    preheader: "How to start, and what comes with Pro after the trial.",
    heading: "Your Pro trial has started.",
    blocks: [
      {
        kind: "p",
        text: "For the next 7 days the convt app is yours to use. Here's how to start.",
      },
      {
        kind: "steps",
        items: [
          {
            title: "Sign in on your computer",
            body: "Open Settings in the convt app and choose Sign in with convt.app. Pro turns on for that computer.",
          },
          {
            title: "Right-click any file",
            body: "Choose Convert with convt and pick a format. Everything runs on your computer.",
          },
        ],
      },
      {
        kind: "note",
        title: "Cloud conversions start when Pro is paid",
        body: "Once your trial becomes a paid plan, you can convert from your phone or browser and send heavy video to the cloud.",
      },
      { kind: "button", href: `${site}/dashboard`, label: "Open your dashboard" },
      {
        kind: "p",
        text: "We'll email you before the trial ends. You can cancel any time from Billing, and nothing is charged until then.",
      },
      { kind: "link", href: `${site}/dashboard/billing`, label: "Manage your plan" },
    ],
  };
}

export type ProductUpdateInput = {
  siteUrl: string;
  subject: string;
  preheader: string;
  heading: string;
  intro: string;
  items: Array<{ title: string; body: string }>;
  cta: { href: string; label: string };
};

/** News and release notes. The content is the campaign's; the frame is shared. */
export function productUpdateEmail(input: ProductUpdateInput): MarketingEmail {
  return {
    subject: input.subject,
    preheader: input.preheader,
    heading: input.heading,
    blocks: [
      { kind: "p", text: input.intro },
      ...input.items.map((item): MarketingBlock => ({
        kind: "note",
        title: item.title,
        body: item.body,
      })),
      { kind: "button", href: input.cta.href, label: input.cta.label },
      {
        kind: "link",
        href: `${input.siteUrl.replace(/\/$/, "")}/changelog`,
        label: "Read the full changelog",
      },
    ],
  };
}

/** The example product update the export and the previews use. */
export function sampleProductUpdate(siteUrl: string): MarketingEmail {
  const site = siteUrl.replace(/\/$/, "");
  return productUpdateEmail({
    siteUrl: site,
    subject: "What's new in convt",
    preheader: "Lifetime Desktop licenses and a browser extension.",
    heading: "What's new in convt",
    intro: "A short roundup of what changed since the last update.",
    items: [
      {
        title: "Desktop is now a lifetime license",
        body: "Buy it once and get every future desktop update. Existing Desktop keys count as lifetime too.",
      },
      {
        title: "Save web images as PNG, JPG or WebP",
        body: "The convt extension for Chrome and Edge adds Convert with convt to the image right-click menu. It converts in your browser.",
      },
    ],
    cta: { href: `${site}/download`, label: "Get the latest version" },
  });
}

export type MarketingTemplateName = "welcome" | "lifetime-desktop" | "pro-trial" | "product-update";

/** Every template, rendered for export and previews. */
export function marketingTemplates(
  footer: FooterInput,
): Record<MarketingTemplateName, MarketingRendered> {
  return {
    welcome: renderMarketing(welcomeEmail(footer.siteUrl), footer),
    "lifetime-desktop": renderMarketing(lifetimeDesktopEmail(footer.siteUrl), footer),
    "pro-trial": renderMarketing(proTrialEmail(footer.siteUrl), footer),
    "product-update": renderMarketing(sampleProductUpdate(footer.siteUrl), footer),
  };
}
