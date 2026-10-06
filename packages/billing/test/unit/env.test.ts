import { describe, expect, test } from "bun:test";
import { base64urlEncode, importSigningKey, publicKeyOf } from "@convt/license";

import { loadSigningKey, readBillingEnv } from "../../src/env";

const seed = base64urlEncode(crypto.getRandomValues(new Uint8Array(32)));
const prod = {
  ENV: "production",
  SITE_URL: "https://convt.app",
  POLAR_ACCESS_TOKEN: "polar_oat_x",
  POLAR_WEBHOOK_SECRET: "whsec_x",
  RESEND_API_KEY: "re_x",
  LICENSE_SIGNING_KEY: seed,
  DEV_LICENSE_PUBKEYS: "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
};

describe("production guards", () => {
  test("a loopback provider or mail URL is refused in production", () => {
    expect(() => readBillingEnv({ ...prod, POLAR_API_URL: "http://127.0.0.1:4000" })).toThrow(
      /POLAR_API_URL/,
    );
    expect(() => readBillingEnv({ ...prod, RESEND_API_URL: "http://localhost:4000" })).toThrow(
      /RESEND_API_URL/,
    );
    expect(() => readBillingEnv({ ...prod, POLAR_PORTAL_ORIGIN: "http://127.0.0.1:4000" })).toThrow(
      /POLAR_PORTAL_ORIGIN/,
    );
    expect(() => readBillingEnv({ ...prod, MAIL_TRANSPORT: "log" })).toThrow(
      /refused in production/,
    );
    expect(() => readBillingEnv({ ...prod, BILLING_CATALOG: "local" })).toThrow(
      /production catalog/,
    );
    expect(() => readBillingEnv({ ...prod, SITE_URL: "http://convt.app" })).toThrow(/https/);
    const { DEV_LICENSE_PUBKEYS: _, ...withoutList } = prod;
    expect(() => readBillingEnv(withoutList)).toThrow(/DEV_LICENSE_PUBKEYS must list/);
    expect(() => readBillingEnv({ ...prod, DEV_LICENSE_PUBKEYS: "none" })).toThrow(
      /DEV_LICENSE_PUBKEYS must list/,
    );
    const e = readBillingEnv(prod);
    expect(e.polar.apiUrl).toBe("https://api.polar.sh");
  });

  test("the signing key must be well formed, match LICENSE_PUBLIC_KEY, and not be a dev key", async () => {
    const pub = await publicKeyOf(
      await importSigningKey(new Uint8Array(Buffer.from(seed, "base64url"))),
    );
    await expect(
      loadSigningKey(readBillingEnv({ ...prod, LICENSE_SIGNING_KEY: "short" })),
    ).rejects.toThrow(/malformed/);
    await expect(loadSigningKey(readBillingEnv(prod))).rejects.toThrow(
      /LICENSE_PUBLIC_KEY is required/,
    );
    await expect(
      loadSigningKey(readBillingEnv({ ...prod, LICENSE_PUBLIC_KEY: "AAAA" })),
    ).rejects.toThrow(/does not match/);
    await expect(
      loadSigningKey(
        readBillingEnv({
          ...prod,
          LICENSE_PUBLIC_KEY: pub,
          DEV_LICENSE_PUBKEYS: `${prod.DEV_LICENSE_PUBKEYS},${pub}`,
        }),
      ),
    ).rejects.toThrow(/dev signing key/);
    await loadSigningKey(readBillingEnv({ ...prod, LICENSE_PUBLIC_KEY: pub }));
  });

  test("development accepts the loopback mock; the local catalog refuses a real provider", () => {
    const dev = {
      ...prod,
      ENV: "development",
      SITE_URL: "http://localhost:3000",
      POLAR_API_URL: "http://127.0.0.1:4000",
      RESEND_API_URL: "http://127.0.0.1:4000",
      POLAR_PORTAL_ORIGIN: "http://127.0.0.1:4000",
    };
    expect(readBillingEnv(dev).catalogEnv).toBe("local");
    expect(() => readBillingEnv({ ...dev, POLAR_API_URL: "https://api.polar.sh" })).toThrow(
      /loopback/,
    );
  });
});

test("staging uses sandbox with public URLs and a separate non-dev signing key", async () => {
  const staging = {
    ...prod,
    ENV: "staging",
    SITE_URL: "https://convt-web-staging.example.workers.dev",
  };
  const e = readBillingEnv(staging);
  expect(e.catalogEnv).toBe("sandbox");
  expect(e.polar.apiUrl).toBe("https://sandbox-api.polar.sh");
  expect(() => readBillingEnv({ ...staging, BILLING_CATALOG: "production" })).toThrow(
    /sandbox catalog/,
  );
  expect(() => readBillingEnv({ ...staging, BILLING_CATALOG: "local" })).toThrow(/sandbox catalog/);
  expect(() => readBillingEnv({ ...staging, RESEND_API_URL: "http://localhost:4000" })).toThrow(
    /https/,
  );
  await expect(loadSigningKey(e)).rejects.toThrow(/LICENSE_PUBLIC_KEY/);
  const pub = await publicKeyOf(
    await importSigningKey(new Uint8Array(Buffer.from(seed, "base64url"))),
  );
  await loadSigningKey(readBillingEnv({ ...staging, LICENSE_PUBLIC_KEY: pub }));
  await expect(
    loadSigningKey(
      readBillingEnv({ ...staging, LICENSE_PUBLIC_KEY: pub, DEV_LICENSE_PUBKEYS: pub }),
    ),
  ).rejects.toThrow(/dev signing key/);
});

test("license mail defaults to the site's download page and allows an override", () => {
  expect(readBillingEnv(prod).downloadUrl).toBe("https://convt.app/download");
  expect(
    readBillingEnv({ ...prod, DOWNLOAD_URL: "https://downloads.convt.app/release" }).downloadUrl,
  ).toBe("https://downloads.convt.app/release");
});
