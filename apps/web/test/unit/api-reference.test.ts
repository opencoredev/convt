import { describe, expect, test } from "bun:test";

import spec from "../../src/generated/openapi.json";
import serverSpec from "../../../../crates/convt-server/openapi.json";
import { endpointSamples, quickStart } from "#/lib/api-samples";
import {
  byTag,
  endpoints,
  enumValues,
  exampleFor,
  resolver,
  slug,
  type OpenApiDocument,
} from "#/lib/openapi";

const doc = spec as unknown as OpenApiDocument;
const base = "https://api.example.test";
const list = endpoints(doc);
const resolve = resolver(doc);

const specPath = (url: string) => {
  const path = url.replace(base, "").replace(/[?#].*$/, "");
  return Object.keys(doc.paths).find((p) =>
    new RegExp(`^${p.replace(/\{[^}]+\}/g, "[^/]+")}$`).test(path),
  );
};

test("the web copy of the spec is the server's spec", () => {
  expect(spec).toEqual(serverSpec);
});

describe("endpoints", () => {
  test("lists exactly the operations convt-server routes", () => {
    expect(list.map((e) => `${e.method.toUpperCase()} ${e.path}`).sort()).toEqual([
      "GET /v1/formats",
      "GET /v1/jobs/{id}",
      "GET /v1/jobs/{id}/download",
      "POST /v1/jobs",
      "POST /v1/jobs/{id}/cancel",
      "POST /v1/jobs/{id}/start",
    ]);
  });

  test("only the formats list is public", () => {
    expect(list.filter((e) => !e.authenticated).map((e) => e.path)).toEqual(["/v1/formats"]);
  });

  test("anchors are stable and unique", () => {
    expect(slug("get", "/v1/jobs/{id}/download")).toBe("get-v1-jobs-id-download");
    expect(new Set(list.map((e) => e.id)).size).toBe(list.length);
  });

  test("groups by tag in spec order", () => {
    expect(byTag(doc).map((g) => g.name)).toEqual(["Jobs", "Formats"]);
  });

  test("every $ref resolves", () => {
    for (const e of list) {
      for (const p of e.params) expect(p?.name).toBeString();
      for (const r of e.responses) expect(r.description).not.toBe("");
    }
  });

  test("create's example body only uses declared fields", () => {
    const create = list.find((e) => e.id === "post-v1-jobs");
    const body = create?.bodyExample as Record<string, unknown>;
    expect(Object.keys(body).every((k) => k in (create?.body?.properties ?? {}))).toBe(true);
    for (const field of create?.body?.required ?? []) expect(body).toHaveProperty(field);
  });
});

describe("objects", () => {
  test("the Job example has every documented field", () => {
    const job = doc.components?.schemas?.Job;
    const example = exampleFor({ $ref: "#/components/schemas/Job" }, resolve) as object;
    expect(Object.keys(example).sort()).toEqual(Object.keys(job?.properties ?? {}).sort());
  });

  test("every error code has a description that starts with its HTTP status", () => {
    const codes = enumValues(doc.components?.schemas?.ErrorCode);
    expect(codes.length).toBeGreaterThan(0);
    for (const { value, description } of codes) {
      expect(description, value).toMatch(/^\d{3}(, \d{3})*\. /);
    }
  });
});

describe("samples", () => {
  test("every sample calls a path the spec documents, on the given base URL", () => {
    const samples = [...quickStart(base), ...list.flatMap((e) => endpointSamples(e, base))];
    for (const sample of samples.filter((s) => s.language !== "cli")) {
      expect(sample.code, sample.label).toContain(base);
      const paths = (sample.code.match(/\/v1\/[^\s"'`)]*/g) ?? []).map(
        (path) => `${base}${path.replace(/\$?\{[^}]+\}|\$\w+/g, "x")}`,
      );
      expect(paths.length, sample.label).toBeGreaterThan(0);
      for (const url of paths) expect(specPath(url), url).toBeDefined();
      expect(sample.code).not.toContain("api.convt.app");
    }
  });

  test("authenticated samples send a bearer key; the public one does not", () => {
    for (const e of list) {
      const curl = endpointSamples(e, base).find((s) => s.language === "curl")?.code ?? "";
      expect(curl.includes("Authorization: Bearer"), e.id).toBe(e.authenticated);
    }
  });
});
