import { expect, test } from "bun:test";
import { codeEmail, sendMail } from "../../src/server/mail";

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
    const message = codeEmail("sign-in", "a@convt.test", "123456", "https://convt.app", 15);
    message.text += " <script>";
    const config = {
      transport: "sequenzy",
      from: "convt <hello@convt.app>",
      apiKey: "sq_fake",
    } satisfies Parameters<typeof sendMail>[0];
    await sendMail(config, message);
    await sendMail(config, message);
    expect(keys[0]).toBe(message.idempotencyKey);
    expect(keys[1]).toBe(keys[0]);
    expect(bodies[0]).toBe(bodies[1]);
    expect(
      codeEmail("sign-in", "a@convt.test", "123456", "https://convt.app", 15).idempotencyKey,
    ).not.toBe(keys[0]);
  } finally {
    globalThis.fetch = originalFetch;
  }
});

test("the log transport never prints the recipient's address", async () => {
  const lines: string[] = [];
  const info = console.info;
  console.info = (line: string) => void lines.push(line);
  try {
    await sendMail(
      { transport: "log", from: "convt <hello@convt.test>" },
      { to: "someone@convt.test", subject: "Hi", text: "Hi", idempotencyKey: "k" },
    );
  } finally {
    console.info = info;
  }
  expect(lines.join("\n")).not.toContain("someone");
  expect(lines.join("\n")).toContain("***@convt.test");
});
