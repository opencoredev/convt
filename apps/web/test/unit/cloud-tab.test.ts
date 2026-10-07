import { expect, test } from "bun:test";

const cloud = await Bun.file(
  new URL("../../src/routes/_app/_shell/dashboard/cloud.tsx", import.meta.url),
).text();
const api = await Bun.file(
  new URL("../../src/routes/_app/_shell/dashboard/api.tsx", import.meta.url),
).text();
const redirect = await Bun.file(
  new URL("../../src/routes/_app/_shell/dashboard/api_.convert.tsx", import.meta.url),
).text();

test("Cloud page is the upload UI, not a 50 GB meter or paid-only gate", () => {
  expect(cloud).toContain("Upload and convert");
  expect(cloud).toContain("Cloud convert is included with Pro");
  expect(cloud).not.toContain("50 GB");
  expect(cloud).not.toContain("needs paid Pro");
  expect(cloud).not.toContain("/dashboard/api");
});

test("API tab no longer hosts the converter", () => {
  expect(api).not.toContain("/dashboard/api/convert");
  expect(api).not.toContain("Convert in your browser");
  expect(api).not.toContain("Cloud conversions are coming soon");
  expect(redirect).toContain('to: "/dashboard/cloud"');
});
