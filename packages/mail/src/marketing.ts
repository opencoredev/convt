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
export const marketingTemplateVersion = 6;

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
  const letter = letterKinds.find((kind) => letters[kind].subject === email.subject);
  if (letter) return renderLetter(letters[letter]);
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

const letterKinds = ["welcome-free", "welcome-paid", "lifetime", "lifetime-free"] as const;
type ApprovedLifecycle = (typeof letterKinds)[number];
const lifecycleCss = `<style>:root{color-scheme:light dark;supported-color-schemes:light dark}@media (max-width:620px){.wrap{width:100%!important;max-width:100%!important}.pad{padding-left:24px!important;padding-right:24px!important}}@media (prefers-color-scheme: dark){.bg{background:#0a0b0b!important}.ink{color:#edefee!important}.ink2{color:#a1a6a3!important}.ink3{color:#868b88!important}.brand,.link{color:#3fcb84!important}.rule{border-color:#232726!important}.lt{display:none!important}.dk{display:block!important;max-height:none!important}}[data-ogsc] .ink{color:#edefee!important}[data-ogsc] .ink2{color:#a1a6a3!important}[data-ogsc] .brand,[data-ogsc] .link{color:#3fcb84!important}</style>`;
const lifecycleLogo = `<img class="lt" src="https://convt.app/brand/convt-lockup-on-light.png" width="89" height="22" alt="convt" style="display:block;width:89px;height:22px;border:0"><!--[if !mso]><!--><img class="dk" src="https://convt.app/brand/convt-lockup-on-dark.png" width="89" height="22" alt="convt" style="display:none;width:89px;height:22px;border:0;max-height:0;overflow:hidden;mso-hide:all"><!--<![endif]-->`;
const lifecycleFooter = `{{postalAddress}} &middot; <a class="ink3" href="${unsubscribeTag}" style="color:#6c716e;text-decoration:underline">unsubscribe</a> &middot; <a class="ink3" href="${preferencesTag}" style="color:#6c716e;text-decoration:underline">preferences</a>`;
const letterFont = "Inter,-apple-system,BlinkMacSystemFont,'Segoe UI',Helvetica,Arial,sans-serif";
const greeting = "hey {{firstName|there}},";
const downloadUrl = "https://convt.app/download";

/** One paragraph, a button, or a link in a letter-style email. */
type LetterBlock =
  | { kind: "p"; text: string }
  | { kind: "item"; title: string; body: string }
  | { kind: "button"; href: string; label: string }
  | { kind: "link"; href: string; label: string };

/** A letter-style email. `blocks` is everything between the greeting and the signature. */
type Letter = { subject: string; preheader: string; title: string; blocks: LetterBlock[] };

// Each letter closes with its own invitation to reply, so the line doesn't read
// as pasted into every email.
const letters: Record<ApprovedLifecycle, Letter> = {
  "welcome-free": {
    subject: "welcome to convt",
    preheader: "welcome to convt",
    title: "welcome to convt",
    blocks: [
      { kind: "p", text: "i'm leo, i'm making convt. thanks for giving it a try." },
      {
        kind: "p",
        text: "you've got 7 days free. download it, right-click any file, pick a format. that's it, and it all stays on your computer.",
      },
      { kind: "button", href: downloadUrl, label: "download convt" },
      {
        kind: "p",
        text: "if something breaks, feels off, or you wish it did something it doesn't, hit reply and tell me.",
      },
    ],
  },
  "welcome-paid": {
    subject: "thanks for buying convt",
    preheader: "thanks for buying convt",
    title: "thank you",
    blocks: [
      {
        kind: "p",
        text: "i'm leo, i'm making convt. thank you for buying it. that genuinely means a lot.",
      },
      {
        kind: "p",
        text: "if you haven't grabbed it yet, here it is. right-click any file, pick a format, done.",
      },
      { kind: "button", href: downloadUrl, label: "download convt" },
      {
        kind: "p",
        text: "got an idea, found a bug, or stuck on something? just reply. it goes straight to me, not a support queue.",
      },
    ],
  },
  // For Desktop buyers.
  lifetime: {
    subject: "your convt license is now lifetime",
    preheader: "your convt license is now lifetime",
    title: "your convt license is now lifetime",
    blocks: [
      {
        kind: "p",
        text: "when i launched convt desktop, i made it a yearly license. that was the wrong call on my part, and i'm sorry. you paid for an app, not a subscription.",
      },
      {
        kind: "p",
        text: "so i've fixed it. desktop licenses are now lifetime, including yours. same key, no renewal, nothing to do.",
      },
      {
        kind: "p",
        text: "thank you for backing convt this early. it means more than you'd think.",
      },
      {
        kind: "p",
        text: "and if there's something you want convt to do next, or something that's bugging you, i'd love to hear it. just reply.",
      },
    ],
  },
  // For accounts that never bought Desktop. The 30% is a code Leo sends by hand
  // to whoever replies; there is no automatic discount behind it.
  "lifetime-free": {
    subject: "convt desktop is lifetime now",
    preheader: "it used to be yearly. that was my mistake.",
    title: "convt desktop is lifetime now",
    blocks: [
      {
        kind: "p",
        text: "when i launched convt desktop, i made it a yearly license. that was a mistake on my part. you should be able to buy an app once and keep it, not rent it.",
      },
      {
        kind: "p",
        text: "so i've changed it. desktop is a lifetime license now: pay once, keep it, and get every future update.",
      },
      {
        kind: "p",
        text: "and since the yearly thing was my fault, i'd like to make it up to you. if you want convt desktop, reply to this email and i'll send you a code for 30% off.",
      },
      { kind: "button", href: "https://convt.app/#pricing", label: "see convt desktop" },
      {
        kind: "p",
        text: "and if something else is holding you back, tell me that too.",
      },
    ],
  },
};

function letterBlockHtml(b: LetterBlock): string {
  switch (b.kind) {
    case "p":
      return `<p style="margin:0 0 16px 0">${escapeHtml(b.text)}</p>`;
    case "item":
      return `<p style="margin:0 0 4px 0;font-weight:600">${escapeHtml(b.title)}</p><p style="margin:0 0 16px 0">${escapeHtml(b.body)}</p>`;
    case "button":
      return `<table role="presentation" cellpadding="0" cellspacing="0" border="0" style="margin:8px 0 24px 0"><tr><td bgcolor="#127a47" style="background:#127a47;border-radius:8px"><a href="${escapeHtml(b.href)}" style="display:inline-block;padding:11px 20px;font-family:${letterFont};font-size:15px;font-weight:600;color:#ffffff;text-decoration:none">${escapeHtml(b.label)}</a></td></tr></table>`;
    case "link":
      return `<p style="margin:0 0 16px 0"><a class="link" href="${escapeHtml(b.href)}" style="color:#127a47;font-weight:600;text-decoration:none">${escapeHtml(b.label)} &rarr;</a></p>`;
    default: {
      const _exhaustive: never = b;
      return _exhaustive;
    }
  }
}

function letterBlockText(b: LetterBlock): string {
  switch (b.kind) {
    case "p":
      return b.text;
    case "item":
      return `${b.title}\n${b.body}`;
    case "button":
    case "link":
      return `${b.label}: ${b.href}`;
    default: {
      const _exhaustive: never = b;
      return _exhaustive;
    }
  }
}

function letterText(letter: Letter): string {
  const body = [greeting, ...letter.blocks.map(letterBlockText), "leo"];
  return `${body.join("\n\n")}\n\n--\nEmail preferences: ${preferencesTag}\nUnsubscribe: ${unsubscribeTag}\n{{postalAddress}}\n`;
}

function letterHtml(letter: Letter): string {
  const copy = letter.blocks.map(letterBlockHtml).join("");
  return `<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><meta name="color-scheme" content="light dark"><meta name="supported-color-schemes" content="light dark"><title>${escapeHtml(letter.title)}</title>${lifecycleCss}</head><body class="bg" style="margin:0;padding:0;background:#ffffff"><table role="presentation" width="100%" cellpadding="0" cellspacing="0" border="0" class="bg" bgcolor="#ffffff"><tr><td align="center"><table class="wrap" role="presentation" width="100%" cellpadding="0" cellspacing="0" border="0" style="width:100%;max-width:600px"><tr><td class="pad" style="padding:48px 48px 0 48px">${lifecycleLogo}</td></tr><tr><td class="ink pad" style="padding:36px 48px 0 48px;font-family:${letterFont};font-size:16px;line-height:26px;color:#0a0a0a"><p style="margin:0 0 16px 0">${greeting}</p>${copy}<p class="ink" style="margin:12px 0 0 0">leo</p></td></tr><tr><td class="pad" style="padding:40px 48px 0 48px"><table role="presentation" width="100%" cellpadding="0" cellspacing="0" border="0"><tr><td class="rule" style="border-top:1px solid #e6e8e7;font-size:0;line-height:0">&nbsp;</td></tr></table></td></tr><tr><td class="ink3 pad" style="padding:14px 48px 40px 48px;font-family:${letterFont};font-size:12px;line-height:18px;color:#6c716e">${lifecycleFooter}</td></tr></table></td></tr></table></body></html>`;
}

function renderLetter(letter: Letter): MarketingRendered {
  return {
    subject: letter.subject,
    preheader: letter.preheader,
    text: letterText(letter),
    html: letterHtml(letter),
  };
}

function lifecycleEmail(kind: ApprovedLifecycle): MarketingEmail {
  const { subject, preheader } = letters[kind];
  return { subject, preheader, heading: subject, blocks: [] };
}
export function welcomeFreeEmail(_siteUrl: string): MarketingEmail {
  return lifecycleEmail("welcome-free");
}
export function welcomePaidEmail(_siteUrl: string): MarketingEmail {
  return lifecycleEmail("welcome-paid");
}
export function welcomeEmailForSegment(_siteUrl: string, segment: "paid" | "free"): MarketingEmail {
  return segment === "paid" ? welcomePaidEmail(_siteUrl) : welcomeFreeEmail(_siteUrl);
}
export function lifetimeEmail(_siteUrl: string): MarketingEmail {
  return lifecycleEmail("lifetime");
}
/** The lifetime announcement for accounts that never bought Desktop, with the 30% offer. */
export function lifetimeFreeEmail(_siteUrl: string): MarketingEmail {
  return lifecycleEmail("lifetime-free");
}
export function welcomeEmail(siteUrl: string): MarketingEmail {
  return welcomeFreeEmail(siteUrl);
}
export function lifetimeDesktopEmail(siteUrl: string): MarketingEmail {
  return lifetimeEmail(siteUrl);
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
  intro: string;
  items: Array<{ title: string; body: string }>;
  cta: { href: string; label: string };
  /** The closing line that asks for a reply. */
  outro: string;
};

/**
 * News and release notes, written as a letter from Leo like the lifecycle
 * emails. The content is the campaign's; the frame is shared.
 */
export function renderProductUpdate(input: ProductUpdateInput): MarketingRendered {
  return renderLetter({
    subject: input.subject,
    preheader: input.preheader,
    title: input.subject,
    blocks: [
      { kind: "p", text: input.intro },
      ...input.items.map((item): LetterBlock => ({ kind: "item", ...item })),
      { kind: "button", href: input.cta.href, label: input.cta.label },
      {
        kind: "link",
        href: `${input.siteUrl.replace(/\/$/, "")}/changelog`,
        label: "full changelog",
      },
      { kind: "p", text: input.outro },
    ],
  });
}

/** The example product update the export and the previews use. */
export function sampleProductUpdate(siteUrl: string): ProductUpdateInput {
  const site = siteUrl.replace(/\/$/, "");
  return {
    siteUrl: site,
    subject: "what's new in convt",
    preheader: "desktop licenses are lifetime now, and there's a browser extension.",
    intro: "a quick roundup of what changed since the last update.",
    items: [
      {
        title: "desktop is a lifetime license now",
        body: "buy it once and get every future desktop update. existing desktop keys count as lifetime too.",
      },
      {
        title: "save web images as png, jpg or webp",
        body: "the convt extension for chrome and edge adds convert with convt to the image right-click menu. it converts in your browser.",
      },
    ],
    cta: { href: `${site}/download`, label: "get the latest version" },
    outro: "what should i build next? reply and let me know.",
  };
}

export type MarketingTemplateName =
  | "welcome-free"
  | "welcome-paid"
  | "lifetime"
  | "lifetime-free"
  | "pro-trial"
  | "product-update";

/** Every template, rendered for export and previews. */
export function marketingTemplates(
  footer: FooterInput,
): Record<MarketingTemplateName, MarketingRendered> {
  return {
    "welcome-free": renderMarketing(welcomeFreeEmail(footer.siteUrl), footer),
    "welcome-paid": renderMarketing(welcomePaidEmail(footer.siteUrl), footer),
    lifetime: renderMarketing(lifetimeEmail(footer.siteUrl), footer),
    "lifetime-free": renderMarketing(lifetimeFreeEmail(footer.siteUrl), footer),
    "pro-trial": renderMarketing(proTrialEmail(footer.siteUrl), footer),
    "product-update": renderProductUpdate(sampleProductUpdate(footer.siteUrl)),
  };
}
