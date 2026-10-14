// bun run dev:sign-in against a disposable Postgres: the token it hands a dev
// build is an ordinary device token, so Pro renewal and the cloud credential
// answer exactly as they would after a browser sign-in. See scripts/dev-sign-in.ts.

import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import {
  chmodSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { currentProKey } from "@convt/billing";
import type { Db } from "@convt/db";
import { activeDevices } from "@convt/db/queries";
import * as t from "@convt/db/schema";
import { runSeed } from "@convt/db/seed";
import { importSigningKey, newId } from "@convt/license";
import { eq } from "drizzle-orm";

import {
  devDeviceSignIn,
  revokeDevSession,
  signInToConfig,
  writeAccountFile,
} from "../../scripts/dev-sign-in";
import type { CloudConfig } from "../../src/server/cloud-credential";
import { deviceCloudCredential, renewDevice, tokenHash } from "../../src/server/device-auth";
import { startHarness, type Harness } from "./harness";

// Fixture addresses are built at runtime: the repo is public and holds no address literals.
const fixture = (name: string) => [name, "convt.test"].join("@");

let h: Harness;
let web: Db;
let billingDb: Db;
let ipCounter = 10;
const nextIp = () => `198.51.100.${ipCounter++}`;
const cloud: CloudConfig = {
  kind: "configured",
  secret: "s".repeat(40),
  baseUrl: "http://127.0.0.1:9",
};

beforeAll(async () => {
  h = await startHarness();
  await runSeed(
    h.owner,
    new Date(),
    await importSigningKey(crypto.getRandomValues(new Uint8Array(32))),
  );
  web = (await h.tdb.open("web")).db;
  billingDb = (await h.tdb.open("billing")).db;
});
afterAll(async () => h?.close());

const proKey = (id: string) => currentProKey(billingDb, id);
const userId = async (email: string) =>
  (await h.owner.select().from(t.users).where(eq(t.users.email, email)))[0].id;

describe("dev:sign-in", () => {
  test("signs the Pro fixture in: renewal returns its Pro key and the cloud credential is issued", async () => {
    const session = await devDeviceSignIn(web, fixture("pro"), new Date());
    expect(session.email).toBe(fixture("pro"));
    expect(session.token).toStartWith("cvd_");

    // An ordinary device row, holding only the token's hash.
    const [device] = await h.owner
      .select()
      .from(t.devices)
      .where(eq(t.devices.tokenHash, await tokenHash(session.token)));
    expect(device.userId).toBe(await userId(fixture("pro")));
    expect(device.name).toEndWith("(dev:sign-in)");
    expect(JSON.stringify(await h.owner.select().from(t.devices))).not.toContain(session.token);
    expect((await activeDevices(web, device.userId)).map((d) => d.id)).toContain(device.id);

    const renewed = await renewDevice(web, proKey, session.token, {}, nextIp(), new Date());
    expect(renewed.status).toBe(200);
    // The Pro key itself; `access` comes from convt-billing, which the route wires in.
    expect(renewed.body.key).toBeString();

    const credential = await deviceCloudCredential(web, cloud, session.token, nextIp(), new Date());
    expect(credential.status).toBe(200);
    expect(credential.body.expiresIn).toBeNumber();
  });

  test("a fixture without Pro signs in but gets no Pro key and no cloud credential", async () => {
    const session = await devDeviceSignIn(web, fixture("new"), new Date());
    const renewed = await renewDevice(web, proKey, session.token, {}, nextIp(), new Date());
    expect(renewed.status).toBe(200);
    expect(renewed.body.key).toBeNull();
    const credential = await deviceCloudCredential(web, cloud, session.token, nextIp(), new Date());
    expect(credential).toEqual({ status: 403, body: { error: "not_pro" } });
  });

  test("a lapsed Pro fixture gets no cloud credential", async () => {
    const session = await devDeviceSignIn(web, fixture("lapsed"), new Date());
    const credential = await deviceCloudCredential(web, cloud, session.token, nextIp(), new Date());
    expect(credential).toEqual({ status: 403, body: { error: "not_pro" } });
  });

  test("refuses addresses that aren't seeded fixtures, and accounts that don't exist", async () => {
    const before = (await h.owner.select().from(t.devices)).length;
    for (const address of ["someone@example.com", fixture("someone"), fixture("")])
      await expect(devDeviceSignIn(web, address, new Date())).rejects.toThrow(
        "accounts `bun run db:seed` writes",
      );
    // A fixture address the seed leaves without an account.
    await expect(devDeviceSignIn(web, fixture("unclaimed"), new Date())).rejects.toThrow(
      "isn't seeded",
    );
    expect((await h.owner.select().from(t.devices)).length).toBe(before);
  });

  test("refuses an account that took a fixture's address but isn't the seeded one", async () => {
    const now = new Date();
    await h.owner.insert(t.users).values({
      id: newId("usr"),
      name: "Not the fixture",
      email: fixture("unclaimed"),
      emailVerified: true,
      createdAt: now,
      updatedAt: now,
    });
    await expect(devDeviceSignIn(web, fixture("unclaimed"), now)).rejects.toThrow("isn't seeded");
  });

  test("revokeDevSession turns the token off", async () => {
    const session = await devDeviceSignIn(web, fixture("desktop"), new Date());
    await revokeDevSession(web, session.token, new Date());
    expect((await renewDevice(web, proKey, session.token, {}, nextIp(), new Date())).status).toBe(
      401,
    );
  });

  test("--replace revokes the sign-in it replaces", async () => {
    const dir = mkdtempSync(join(tmpdir(), "convt-dev-sign-in-"));
    try {
      const first = await signInToConfig(web, fixture("pro"), dir, false, new Date());
      const second = await signInToConfig(web, fixture("pro"), dir, true, new Date());
      const renew = async (token: string) =>
        (await renewDevice(web, proKey, token, {}, nextIp(), new Date())).status;
      expect(await renew(first.session.token)).toBe(401);
      expect(await renew(second.session.token)).toBe(200);
      expect(JSON.parse(readFileSync(second.file, "utf8")).token).toBe(second.session.token);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  test("a sign-in whose file can't be written leaves no new device and the old sign-in whole", async () => {
    const dir = mkdtempSync(join(tmpdir(), "convt-dev-sign-in-"));
    try {
      const first = await signInToConfig(web, fixture("pro"), dir, false, new Date());
      const before = readFileSync(first.file, "utf8");
      const active = async () => (await activeDevices(web, await userId(fixture("pro")))).length;
      const count = await active();
      chmodSync(dir, 0o500);
      await expect(signInToConfig(web, fixture("pro"), dir, true, new Date())).rejects.toThrow();
      chmodSync(dir, 0o700);
      expect(await active()).toBe(count);
      expect(readFileSync(first.file, "utf8")).toBe(before);
      expect(
        (await renewDevice(web, proKey, first.session.token, {}, nextIp(), new Date())).status,
      ).toBe(200);
    } finally {
      chmodSync(dir, 0o700);
      rmSync(dir, { recursive: true, force: true });
    }
  });

  test("refuses an account whose email isn't verified", async () => {
    await h.owner
      .update(t.users)
      .set({ emailVerified: false })
      .where(eq(t.users.email, fixture("api")));
    await expect(devDeviceSignIn(web, fixture("api"), new Date())).rejects.toThrow(
      "no verified email",
    );
  });
});

describe("account.json", () => {
  test("is owner-only and holds the email and token the app reads", () => {
    const dir = mkdtempSync(join(tmpdir(), "convt-dev-sign-in-"));
    try {
      const file = writeAccountFile(join(dir, "config"), { email: "a", token: "cvd_x" }, false);
      expect(statSync(file).mode & 0o777).toBe(0o600);
      expect(JSON.parse(readFileSync(file, "utf8"))).toEqual({ email: "a", token: "cvd_x" });
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  test("is replaced only when asked", () => {
    const dir = mkdtempSync(join(tmpdir(), "convt-dev-sign-in-"));
    try {
      writeFileSync(join(dir, "account.json"), "old");
      expect(() => writeAccountFile(dir, { email: "a", token: "cvd_x" }, false)).toThrow(
        "pass --replace",
      );
      expect(readFileSync(join(dir, "account.json"), "utf8")).toBe("old");
      writeAccountFile(dir, { email: "a", token: "cvd_y" }, true);
      expect(JSON.parse(readFileSync(join(dir, "account.json"), "utf8")).token).toBe("cvd_y");
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  test("a replacement is owner-only even when the old file wasn't", () => {
    const dir = mkdtempSync(join(tmpdir(), "convt-dev-sign-in-"));
    try {
      writeFileSync(join(dir, "account.json"), "old", { mode: 0o644 });
      chmodSync(join(dir, "account.json"), 0o644);
      const file = writeAccountFile(dir, { email: "a", token: "cvd_y" }, true);
      expect(statSync(file).mode & 0o777).toBe(0o600);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  test("a replacement that fails leaves the old file whole", () => {
    const dir = mkdtempSync(join(tmpdir(), "convt-dev-sign-in-"));
    try {
      writeFileSync(join(dir, "account.json"), "old");
      // Something already holds the temporary name, so the new file can't be made.
      mkdirSync(join(dir, `.account.json.${process.pid}.tmp`));
      expect(() => writeAccountFile(dir, { email: "a", token: "cvd_y" }, true)).toThrow();
      expect(readFileSync(join(dir, "account.json"), "utf8")).toBe("old");
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
});
