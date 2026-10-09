// The dashboard's API samples and docs links against the sources they describe: the
// server's OpenAPI spec and the Blume site in apps/docs that serves convt.app/docs.

import { describe, expect, test } from "bun:test";
import { existsSync, readFileSync } from "node:fs";

import spec from "../../../../crates/convt-server/openapi.json";
import { quickStart } from "#/lib/api-samples";
import { apiBaseUrl, links } from "#/lib/config";

type Operation = { security?: unknown[] };
const paths = spec.paths as Record<string, Record<string, Operation>>;
const schemas = spec.components.schemas as Record<
  string,
  { required?: string[]; enum?: string[]; "x-enum-descriptions"?: Record<string, string> }
>;
const base = "https://api.example.test";

const specPath = (path: string) =>
  Object.keys(paths).find((p) => new RegExp(`^${p.replace(/\{[^}]+\}/g, "[^/]+")}$`).test(path));

describe("OpenAPI spec", () => {
  test("lists exactly the operations convt-server routes", () => {
    const ops = Object.entries(paths).flatMap(([path, item]) =>
      Object.keys(item).map((method) => `${method.toUpperCase()} ${path}`),
    );
    expect(ops.sort()).toEqual([
      "GET /v1/formats",
      "GET /v1/jobs/{id}",
      "GET /v1/jobs/{id}/download",
      "POST /v1/jobs",
      "POST /v1/jobs/{id}/cancel",
      "POST /v1/jobs/{id}/start",
    ]);
  });

  test("only the formats list is public", () => {
    const open = Object.entries(paths).flatMap(([path, item]) =>
      Object.values(item)
        .filter((op) => (op.security ?? spec.security).length === 0)
        .map(() => path),
    );
    expect(open).toEqual(["/v1/formats"]);
  });

  test("every error code's description starts with its HTTP status", () => {
    const { enum: codes = [], "x-enum-descriptions": text = {} } = schemas.ErrorCode;
    expect(codes.length).toBeGreaterThan(0);
    for (const code of codes) expect(text[code], code).toMatch(/^\d{3}(, \d{3})*\. /);
  });
});

describe("dashboard quick start", () => {
  const samples = quickStart(base).filter((s) => s.language !== "cli");

  test("calls only documented paths, on the given base URL", () => {
    for (const sample of samples) {
      expect(sample.code, sample.label).toContain(base);
      expect(sample.code).not.toContain("api.convt.app");
      const used = (sample.code.match(/\/v1\/[^\s"'`)]*/g) ?? []).map((path) =>
        path.replace(/\$?\{[^}]+\}|\$\w+/g, "x"),
      );
      expect(used.length, sample.label).toBeGreaterThan(0);
      for (const path of used) expect(specPath(path), path).toBeDefined();
    }
  });

  test("sends every field create requires", () => {
    const required = schemas.CreateJob.required ?? [];
    expect(required.length).toBeGreaterThan(0);
    for (const sample of samples) {
      for (const field of required) expect(sample.code, sample.label).toContain(field);
    }
  });
});

describe("docs links", () => {
  const docs = new URL("../../../docs/", import.meta.url);
  const page = (href: string) => href.replace(/^\/docs\/?/, "").replace(/#.*$/, "") || "index";

  test("each dashboard link is a page in the Blume site", () => {
    const pages = [links.docs, links.docsQuickStart, links.docsErrors, links.docsLimits];
    for (const href of pages) {
      expect(existsSync(new URL(`content/${page(href)}.mdx`, docs)), href).toBe(true);
    }
    const formats = readFileSync(new URL("scripts/generate-formats.ts", docs), "utf8");
    expect(formats).toContain(`content/${page(links.docsFormats)}.mdx`);
  });

  test("the dashboard and the docs show the same API host", () => {
    const host = readFileSync(new URL("api-host.ts", docs), "utf8");
    expect(host).toContain(`export const apiBase = "${apiBaseUrl}";`);
  });

  test("the API reference link is Blume's OpenAPI route", () => {
    const config = readFileSync(new URL("blume.config.ts", docs), "utf8");
    expect(config).toContain(`route: "/${page(links.apiReference)}"`);
    expect(config).toContain(`spec: "../../crates/convt-server/openapi.json"`);
  });
});
