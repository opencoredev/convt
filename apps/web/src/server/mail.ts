import { codeEmail as renderCodeEmail, sequenzyTransport } from "@convt/mail";

// Outgoing email. `mailpit` posts to a local Mailpit's send API (its web UI shows
// the messages), `log` prints a redacted line, and the provider transports send
// transactional mail. The dev transports are refused in production by readEnv.

import type { AppEnv } from "./env";
import { redactText } from "./redact";

export type MailMessage = {
  to: string;
  subject: string;
  text: string;
  html?: string;
  idempotencyKey: string;
};

export async function sendMail(config: AppEnv["mail"], message: MailMessage): Promise<void> {
  if (config.transport === "log") {
    console.info(`[mail] to ${message.to}: ${redactText(message.subject)}`);
    return;
  }
  if (config.transport === "mailpit") {
    const res = await fetch(`${config.url}/api/v1/send`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        From: parseAddress(config.from),
        To: [{ Email: message.to }],
        Subject: message.subject,
        Text: message.text,
        HTML: message.html,
      }),
    });
    if (!res.ok) throw new Error(`mailpit answered ${res.status}`);
    return;
  }
  if (config.transport === "sequenzy") {
    const result = await sequenzyTransport({ apiKey: config.apiKey }).send(
      {
        ...message,
        from: config.from,
        html: message.html ?? `<pre>${escapeHtml(message.text)}</pre>`,
      },
      message.idempotencyKey,
    );
    if (!result.ok) throw new Error(`sequenzy answered ${result.status ?? result.code}`);
    return;
  }
  if (!config.apiKey) throw new Error("RESEND_API_KEY is not set");
  const res = await fetch("https://api.resend.com/emails", {
    method: "POST",
    headers: { authorization: `Bearer ${config.apiKey}`, "content-type": "application/json" },
    body: JSON.stringify({
      from: config.from,
      to: [message.to],
      subject: message.subject,
      text: message.text,
      html: message.html,
    }),
  });
  if (!res.ok) throw new Error(`resend answered ${res.status}`);
}

function escapeHtml(text: string): string {
  return text.replace(/[&<>"']/g, (c) => {
    switch (c) {
      case "&":
        return "&amp;";
      case "<":
        return "&lt;";
      case ">":
        return "&gt;";
      case '"':
        return "&quot;";
      default:
        return "&#39;";
    }
  });
}

function parseAddress(text: string): { Email: string; Name?: string } {
  const m = text.match(/^\s*(.*?)\s*<([^>]+)>\s*$/);
  return m ? { Name: m[1] || undefined, Email: m[2] } : { Email: text.trim() };
}

export type CodeKind = "sign-in" | "email-verification" | "change-email" | "forget-password";

/** The sign-in email: the code, and a link whose fragment carries the code. Text and HTML. */
export function codeEmail(
  kind: CodeKind,
  email: string,
  code: string,
  siteUrl: string,
  minutes: number,
): MailMessage {
  const link = `${siteUrl}/sign-in/verify#${new URLSearchParams({ email, code })}`;
  const rendered = renderCodeEmail({
    kind: kind === "sign-in" ? "sign-in" : kind === "change-email" ? "change-email" : "confirm",
    email,
    code,
    link,
    minutes,
  });
  return { to: email, ...rendered, idempotencyKey: crypto.randomUUID() };
}
