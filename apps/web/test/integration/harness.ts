// In-process Better Auth for integration tests: each request gets a fresh auth
// instance and its own connection as convt_web, like a Worker request. Codes are
// captured instead of mailed, and the OAuth mock runs on a random local port.

import { createDb, type Db } from "@convt/db";
import { freshDatabase, type TestDatabase } from "@convt/db/testing";
import { createMock } from "@convt/oauth-mock";
import pg from "pg";

import type { AnalyticsEvent } from "../../src/server/analytics";
import { createAuth, type RequestScope } from "../../src/server/auth";
import { readEnv, type AppEnv } from "../../src/server/env";
import type { MailMessage } from "../../src/server/mail";

export const siteUrl = "http://localhost:3999";

export type Harness = {
  tdb: TestDatabase;
  owner: Db;
  env: AppEnv;
  mail: MailMessage[];
  analytics: AnalyticsEvent[];
  mockUrl: string;
  close: () => Promise<void>;
  /** One auth request as a browser on `ip` with its cookie jar. */
  request: (
    path: string,
    init?: {
      method?: string;
      body?: unknown;
      jar?: Jar;
      ip?: string;
      headers?: Record<string, string>;
    },
  ) => Promise<Response>;
  codeFor: (email: string) => string;
  /** Runs `fn` with one request's auth instance, for calls no endpoint exposes. */
  withAuth: <T>(fn: (auth: ReturnType<typeof createAuth>) => Promise<T>) => Promise<T>;
};

export class Jar {
  cookies = new Map<string, string>();

  header(): string {
    return [...this.cookies].map(([k, v]) => `${k}=${v}`).join("; ");
  }

  take(res: Response) {
    for (const line of res.headers.getSetCookie()) {
      const [pair] = line.split(";");
      const eq = pair.indexOf("=");
      const name = pair.slice(0, eq).trim();
      const value = pair.slice(eq + 1).trim();
      if (/max-age=0/i.test(line) || value === "") this.cookies.delete(name);
      else this.cookies.set(name, value);
    }
  }

  get session(): string | undefined {
    return this.cookies.get("convt.session_token");
  }
}

export async function startHarness(options: { production?: boolean } = {}): Promise<Harness> {
  const tdb = await freshDatabase();
  const ownerConn = await tdb.open("owner");
  const server = Bun.serve({
    hostname: "127.0.0.1",
    port: 0,
    fetch: () => new Response("starting"),
  });
  const mockUrl = `http://127.0.0.1:${server.port}`;
  const mock = await createMock({ internalUrl: mockUrl });
  server.reload({ fetch: mock.fetch });
  const env = readEnv({
    ENV: options.production ? "production" : "test",
    BETTER_AUTH_URL: options.production ? "https://convt.test" : siteUrl,
    BETTER_AUTH_SECRET: "test-secret-".padEnd(64, "x"),
    ...(options.production
      ? { RESEND_API_KEY: "unused" }
      : { MAIL_TRANSPORT: "log", OAUTH_MOCK_URL: mockUrl }),
  });
  const pool = new pg.Pool({ connectionString: tdb.webUrl, max: 30 });
  const mail: MailMessage[] = [];
  const analytics: AnalyticsEvent[] = [];

  async function request(
    path: string,
    init: {
      method?: string;
      body?: unknown;
      jar?: Jar;
      ip?: string;
      headers?: Record<string, string>;
    } = {},
  ): Promise<Response> {
    const client = await pool.connect();
    const pending: Promise<unknown>[] = [];
    const scope: RequestScope = { db: createDb(client), background: (p) => void pending.push(p) };
    try {
      const auth = createAuth(scope, env, {
        startCookies: false,
        sendMail: async (m) => void mail.push(m),
        captureAnalytics: async (event) => {
          analytics.push(event);
        },
      });
      const url = path.startsWith("http") ? path : `${env.authUrl}/api/auth${path}`;
      const headers: Record<string, string> = {
        origin: new URL(env.authUrl).origin,
        "cf-connecting-ip": init.ip ?? "203.0.113.1",
        "user-agent": "convt-tests",
        ...init.headers,
      };
      if (init.jar?.cookies.size) headers.cookie = init.jar.header();
      let body: string | undefined;
      if (init.body !== undefined) {
        headers["content-type"] = "application/json";
        body = JSON.stringify(init.body);
      }
      const res = await auth.handler(
        new Request(url, { method: init.method ?? (body ? "POST" : "GET"), headers, body }),
      );
      await Promise.allSettled(pending);
      init.jar?.take(res);
      return res;
    } finally {
      client.release();
    }
  }

  function codeFor(email: string): string {
    const m = [...mail].reverse().find((x) => x.to === email);
    if (!m) throw new Error(`no code mailed to ${email}`);
    return m.subject.match(/\d{6}/)![0];
  }

  async function withAuth<T>(fn: (auth: ReturnType<typeof createAuth>) => Promise<T>): Promise<T> {
    const client = await pool.connect();
    try {
      return await fn(
        createAuth({ db: createDb(client), background: () => {} }, env, {
          startCookies: false,
          captureAnalytics: async (event) => {
            analytics.push(event);
          },
        }),
      );
    } finally {
      client.release();
    }
  }

  return {
    withAuth,
    tdb,
    owner: ownerConn.db,
    env,
    mail,
    analytics,
    mockUrl,
    request,
    codeFor,
    async close() {
      server.stop(true);
      await pool.end();
      await tdb.drop();
    },
  };
}

/** Sends a sign-in code and signs in with it. */
export async function signInWithCode(h: Harness, email: string, jar = new Jar(), ip?: string) {
  const sent = await h.request("/email-otp/send-verification-otp", {
    body: { email, type: "sign-in" },
    jar,
    ip,
  });
  if (sent.status !== 200) throw new Error(`send failed: ${sent.status} ${await sent.text()}`);
  const res = await h.request("/sign-in/email-otp", {
    body: { email, otp: h.codeFor(email) },
    jar,
    ip,
  });
  if (res.status !== 200) throw new Error(`sign-in failed: ${res.status} ${await res.text()}`);
  return jar;
}

/**
 * Runs an OAuth round trip through the mock: start (sign-in or link), pick the
 * identity on the authorize page, follow the callback. Returns the final redirect.
 */
export async function oauth(
  h: Harness,
  kind: "sign-in" | "link",
  provider: "github" | "google",
  identity: string,
  jar: Jar,
  opts: { email?: string; beforeCallback?: () => Promise<void> } = {},
): Promise<{ location: string; status: number }> {
  const start =
    kind === "sign-in"
      ? await h.request("/sign-in/social", {
          body: {
            provider,
            callbackURL: "/dashboard",
            errorCallbackURL: "/sign-in",
            disableRedirect: true,
          },
          jar,
        })
      : await h.request("/link-social", {
          body: { provider, callbackURL: "/account", disableRedirect: true },
          jar,
        });
  if (start.status !== 200) return { location: "", status: start.status };
  const { url } = (await start.json()) as { url: string };
  const authorize = new URL(url);
  authorize.searchParams.set("identity", identity);
  if (opts.email) authorize.searchParams.set("email", opts.email);
  const toCallback = await fetch(authorize, { redirect: "manual" });
  const callback = toCallback.headers.get("location")!;
  await opts.beforeCallback?.();
  const res = await h.request(callback, { jar });
  return { location: res.headers.get("location") ?? "", status: res.status };
}
