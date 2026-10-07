// The Worker's settings. Production values are Worker vars and secrets; local
// development gets them from apps/web/.dev.vars (written by scripts/dev-web.sh).
// A dev mail sink or the OAuth mock is refused unless ENV is not production and
// its server-side URL is loopback.

export type AppEnv = {
  env: "production" | "staging" | "development" | "test";
  sales: "desktop" | "all";
  authUrl: string;
  authSecret: string;
  mail:
    | { transport: "mailpit"; url: string; from: string }
    | { transport: "log"; from: string }
    | { transport: "sequenzy"; apiKey: string; from: string }
    | { transport: "resend"; apiKey: string; from: string };
  oauthMock: { url: string; publicUrl: string } | null;
  github: { clientId: string; clientSecret: string } | null;
  google: { clientId: string; clientSecret: string } | null;
  /** Public PostHog project key + host for the marketing site. Null when unset. */
  posthog: { key: string; host: string } | null;
};

export type RawEnv = Record<string, unknown>;

function isLoopback(url: string): boolean {
  try {
    const host = new URL(url).hostname;
    return host === "127.0.0.1" || host === "localhost" || host === "[::1]";
  } catch {
    return false;
  }
}

const str = (raw: RawEnv, key: string) => {
  const v = raw[key];
  return typeof v === "string" && v.trim() !== "" ? v.trim() : undefined;
};

export function readEnv(raw: RawEnv): AppEnv {
  const envName = str(raw, "ENV") ?? "production";
  if (
    envName !== "production" &&
    envName !== "staging" &&
    envName !== "development" &&
    envName !== "test"
  )
    throw new Error(`ENV must be production, staging, development or test, not ${envName}`);
  const production = envName === "production" || envName === "staging";
  const sales = str(raw, "SALES") ?? (production ? "desktop" : "all");
  if (sales !== "desktop" && sales !== "all") throw new Error("SALES must be desktop or all");
  const authUrl = str(raw, "BETTER_AUTH_URL");
  const authSecret = str(raw, "BETTER_AUTH_SECRET");
  if (!authUrl) throw new Error("BETTER_AUTH_URL is not set");
  if (!authSecret || authSecret.length < 32)
    throw new Error("BETTER_AUTH_SECRET is missing or shorter than 32 characters");
  if (production && !authUrl.startsWith("https://"))
    throw new Error("BETTER_AUTH_URL must be https in production");

  const from = str(raw, "MAIL_FROM") ?? "convt <hello@convt.app>";
  const transport = str(raw, "MAIL_TRANSPORT") ?? (production ? "resend" : "log");
  let mail: AppEnv["mail"];
  if (transport === "resend") {
    mail = { transport, apiKey: str(raw, "RESEND_API_KEY") ?? "", from };
  } else if (transport === "sequenzy") {
    const apiKey = str(raw, "SEQUENZY_API_KEY");
    if (!apiKey) throw new Error("SEQUENZY_API_KEY is not set");
    mail = { transport, apiKey, from };
  } else if (transport === "mailpit" || transport === "log") {
    if (production) throw new Error(`MAIL_TRANSPORT=${transport} is refused in production`);
    if (transport === "mailpit") {
      const url = str(raw, "MAILPIT_URL");
      if (!url || !isLoopback(url)) throw new Error("MAILPIT_URL must be a loopback URL");
      mail = { transport, url, from };
    } else {
      mail = { transport, from };
    }
  } else {
    throw new Error(`unknown MAIL_TRANSPORT ${transport}`);
  }

  const mockUrl = str(raw, "OAUTH_MOCK_URL");
  let oauthMock: AppEnv["oauthMock"] = null;
  if (mockUrl) {
    if (production) throw new Error("OAUTH_MOCK_URL is refused in production");
    if (!isLoopback(mockUrl)) throw new Error("OAUTH_MOCK_URL must be a loopback URL");
    oauthMock = {
      url: mockUrl.replace(/\/$/, ""),
      publicUrl: (str(raw, "OAUTH_MOCK_PUBLIC_URL") ?? mockUrl).replace(/\/$/, ""),
    };
  }
  const pair = (id: string, secret: string) => {
    const clientId = str(raw, id);
    const clientSecret = str(raw, secret);
    return clientId && clientSecret ? { clientId, clientSecret } : null;
  };
  const posthogKey = str(raw, "POSTHOG_KEY");
  const posthogHost = str(raw, "POSTHOG_HOST") ?? "https://us.i.posthog.com";
  const posthog = posthogKey ? { key: posthogKey, host: posthogHost.replace(/\/$/, "") } : null;

  return {
    env: envName,
    sales,
    authUrl: authUrl.replace(/\/$/, ""),
    authSecret,
    mail,
    oauthMock,
    github: pair("GITHUB_CLIENT_ID", "GITHUB_CLIENT_SECRET"),
    google: pair("GOOGLE_CLIENT_ID", "GOOGLE_CLIENT_SECRET"),
    posthog,
  };
}

/** Matches createAuth's real providers and its all-or-nothing local OAuth mock. */
export function availableProviders(env: AppEnv) {
  const mock = env.oauthMock !== null && env.github === null && env.google === null;
  return { github: env.github !== null || mock, google: env.google !== null || mock };
}
