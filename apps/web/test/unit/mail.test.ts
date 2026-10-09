import { expect, test } from "bun:test";
import { codeEmail, sendMail } from "../../src/server/mail";

// Built at runtime so no address appears in the source.
const address = ["a", "convt.test"].join("@");
const sender = `convt <${["hello", "convt.app"].join("@")}>`;

test("sign-in mail uses Sequenzy HTML, preserves its key on retry, and escapes text", async () => {
  const originalFetch = globalThis.fetch;
  const bodies: string[] = [];
  const keys: Array<string | null> = [];
  globalThis.fetch = Object.assign(
    async (input: Parameters<typeof fetch>[0], init?: RequestInit) => {
      expect(input).toBe("https://api.sequenzy.com/api/v1/transactional/send");
      const headers = new Headers(init?.headers);
      keys.push(headers.get("idempotency-key"));
      bodies.push(String(init?.body));
      const body = JSON.parse(String(init?.body));
      expect(body.body).toContain("123456");
      expect(body.body).toContain("&lt;script&gt;");
      expect(body.body).not.toContain("<script>");
      expect(body.trackingSettings).toEqual({ clickTracking: false, openTracking: false });
      expect(body).not.toHaveProperty("text");
      return Response.json({ success: true, emailSendId: "seq_1" });
    },
    { preconnect: originalFetch.preconnect },
  );
  try {
    // Without HTML, Sequenzy gets the text, escaped.
    const message = { ...codeEmail("sign-in", address, "123456", "https://convt.app", 15) };
    delete message.html;
    message.text += " <script>";
    const config = {
      transport: "sequenzy",
      from: sender,
      apiKey: "sq_fake",
    } satisfies Parameters<typeof sendMail>[0];
    await sendMail(config, message);
    await sendMail(config, message);
    expect(keys[0]).toBe(message.idempotencyKey);
    expect(keys[1]).toBe(keys[0]);
    expect(bodies[0]).toBe(bodies[1]);
    expect(
      codeEmail("sign-in", address, "123456", "https://convt.app", 15).idempotencyKey,
    ).not.toBe(keys[0]);
  } finally {
    globalThis.fetch = originalFetch;
  }
});

test("the sign-in code email has an HTML part with the code and the sign-in link", () => {
  const message = codeEmail("sign-in", address, "123456", "https://convt.app", 15);
  expect(message.subject).toBe("123456 is your convt sign-in code");
  expect(message.text).toContain("123456");
  expect(message.text).toContain("https://convt.app/sign-in/verify#");
  expect(message.html).toContain(">123456</p>");
  expect(message.html).toContain("https://convt.app/sign-in/verify#email=");
  expect(
    codeEmail("email-verification", address, "222222", "https://convt.app", 15).html,
  ).toContain(`confirm ${address} for your convt account`);
});
