import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { renderToStaticMarkup } from "react-dom/server";

import { GoogleSignIn, OtherSignIn } from "../../src/components/app/social-sign-in";
import { checkoutAside, downloadAction } from "../../src/lib/checkout-copy";
import {
  downloadCtaLabel,
  downloadHref,
  downloadStepLabel,
  osFromUserAgent,
} from "../../src/lib/platform";
import { availableProviders, readEnv } from "../../src/server/env";
import { visibleMethods } from "../../src/server/views";
import type { SignInMethod } from "../../src/lib/types";

const env = readEnv({
  ENV: "test",
  BETTER_AUTH_URL: "http://localhost:3000",
  BETTER_AUTH_SECRET: "x".repeat(64),
  GITHUB_CLIENT_ID: "id",
  GITHUB_CLIENT_SECRET: "secret",
  GOOGLE_CLIENT_ID: "id",
  GOOGLE_CLIENT_SECRET: "secret",
});
const noop = () => {};

const both = (available: Parameters<typeof OtherSignIn>[0]["available"]) =>
  renderToStaticMarkup(
    <>
      <GoogleSignIn available={available} onSelect={noop} />
      <OtherSignIn available={available} onSelect={noop} />
    </>,
  );

test("sign-in shows no Apple button while Apple is not configured", () => {
  const html = both(availableProviders(env));
  expect(html).toContain("Continue with Google</button>");
  expect(html).toContain("GitHub</button>");
  expect(html).not.toContain("Apple");
});

test("sign-in renders the Apple button once it is configured", () => {
  const html = both({ github: false, google: false, apple: true });
  expect(html).toContain(">Apple</button>");
  expect(html).not.toContain("GitHub");
  expect(html).not.toContain("Google");
});

test("sign-in drops Google, the 'or' divider and the provider links when none is configured", () => {
  expect(both({ github: false, google: false, apple: false })).toBe("");
});

const method = (id: SignInMethod["id"], accountId: string | null = null): SignInMethod => ({
  id,
  label: id,
  identity: accountId ? `ID ${accountId}` : null,
  removable: id !== "email",
  accountId,
});

test("settings lists no Apple method while Apple is not configured", () => {
  const methods = [method("email"), method("github"), method("google"), method("apple")];
  expect(visibleMethods(methods, availableProviders(env)).map((m) => m.id)).toEqual([
    "email",
    "github",
    "google",
  ]);
  expect(
    visibleMethods(methods, { github: false, google: false, apple: true }).map((m) => m.id),
  ).toEqual(["email", "apple"]);
});

test("settings keeps an already linked provider so it can be removed", () => {
  const methods = [method("email"), method("github", "acc_1"), method("google")];
  expect(
    visibleMethods(methods, { github: false, google: false, apple: false }).map((m) => m.id),
  ).toEqual(["email", "github"]);
});

const agents = {
  mac: "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 Version/18.0 Safari/605.1.15",
  windows:
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/140.0 Safari/537.36",
  linux: "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 Chrome/140.0 Safari/537.36",
  iphone: "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) Version/18.0 Mobile Safari/604.1",
};

test("checkout success names the visitor's OS, with a neutral fallback", () => {
  const copy = (ua: string) => {
    const os = osFromUserAgent(ua);
    return [downloadStepLabel(os), downloadCtaLabel(os), downloadHref(os)];
  };
  expect(copy(agents.mac)).toEqual([
    "Download convt for macOS",
    "Download for macOS",
    "/download?os=macos",
  ]);
  expect(copy(agents.windows)).toEqual([
    "Download convt for Windows",
    "Download for Windows",
    "/download?os=windows",
  ]);
  expect(copy(agents.linux)).toEqual([
    "Download convt for Linux",
    "Download for Linux",
    "/download?os=linux",
  ]);
  expect(copy(agents.iphone)).toEqual(["Download convt", "Download", "/download"]);
});

test("checkout success panel and button follow the visitor's OS", () => {
  const windows = osFromUserAgent(agents.windows);
  expect(checkoutAside("key", windows).items[0]).toBe("Download convt for Windows");
  expect(checkoutAside("trial", osFromUserAgent(agents.linux)).items[0]).toBe(
    "Download convt for Linux",
  );
  expect(checkoutAside("key", osFromUserAgent(agents.mac)).items[0]).toBe(
    "Download convt for macOS",
  );
  expect(checkoutAside("trial", null).items[0]).toBe("Download convt");
  expect(downloadAction(windows)).toEqual({
    href: "/download?os=windows",
    label: "Download for Windows",
  });
  expect(downloadAction(null)).toEqual({ href: "/download", label: "Download" });
});

test("account pages no longer say Mac where any computer is meant", () => {
  const files = [
    "../../src/routes/_app/checkout/success.tsx",
    "../../src/components/app/mac-list.tsx",
    "../../src/components/app/auth-layout.tsx",
    "../../src/routes/_app/_shell/dashboard/index.tsx",
    "../../src/routes/_app/_shell/dashboard/licenses.tsx",
    "../../src/routes/_app/_shell/account.tsx",
    "../../src/server/views.ts",
  ];
  for (const file of files) {
    const source = readFileSync(new URL(file, import.meta.url), "utf8");
    expect(source).not.toMatch(/your Mac|\bMacs\b|any Mac\b|every Mac\b|a Mac\b|"Mac"|Mac app/);
  }
});
