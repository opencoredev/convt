// Typed access to the subset of OpenAPI 3.1 that convt-server's spec uses: tagged
// operations, path parameters, JSON bodies, $ref into components, enums with
// `x-enum-descriptions`, and plain-text framework rejections. Unknown fields are ignored.

export type Ref = { $ref: string };
export type Schema = {
  type?: string | string[];
  format?: string;
  description?: string;
  enum?: string[];
  "x-enum-descriptions"?: Record<string, string>;
  example?: unknown;
  minimum?: number;
  maximum?: number;
  items?: Schema | Ref;
  properties?: Record<string, Schema | Ref>;
  required?: string[];
  oneOf?: (Schema | Ref)[];
};
type MediaTypes = Record<string, { schema?: Schema | Ref; example?: unknown }>;
export type Parameter = {
  name: string;
  in: string;
  required?: boolean;
  description?: string;
  schema?: Schema | Ref;
};
export type Response = { description?: string; content?: MediaTypes };
export type Operation = {
  tags?: string[];
  operationId?: string;
  summary?: string;
  description?: string;
  parameters?: (Parameter | Ref)[];
  requestBody?: { required?: boolean; content?: MediaTypes } | Ref;
  responses?: Record<string, Response | Ref>;
  security?: unknown[];
};
export type OpenApiDocument = {
  "x-convt-placeholder"?: boolean;
  info: { title: string; version: string; summary?: string; description?: string };
  servers?: { url: string; description?: string }[];
  tags?: { name: string; description?: string }[];
  security?: unknown[];
  paths: Record<string, Record<string, Operation>>;
  components?: {
    schemas?: Record<string, Schema>;
    parameters?: Record<string, Parameter>;
    responses?: Record<string, Response>;
    securitySchemes?: Record<string, { type: string; scheme?: string; description?: string }>;
  };
};

const methods = ["get", "post", "put", "patch", "delete"] as const;

export const isRef = (value: unknown): value is Ref =>
  typeof value === "object" && value !== null && "$ref" in value;
export const refName = (ref: string) => ref.split("/").pop() ?? ref;

export type Resolve = <T>(value: T | Ref) => T;

export function resolver(doc: OpenApiDocument): Resolve {
  return function resolve<T>(value: T | Ref): T {
    if (!isRef(value)) return value;
    const [, , group, name] = value.$ref.split("/");
    const components = doc.components as Record<string, Record<string, unknown>> | undefined;
    return components?.[group]?.[name] as T;
  };
}

export type Endpoint = {
  id: string;
  method: string;
  path: string;
  op: Operation;
  params: Parameter[];
  body: Schema | null;
  bodyExample: unknown;
  /** Status code, description, and the JSON schema if the body is an Error or object. */
  responses: { code: string; description: string; schema: Schema | Ref | null; text: boolean }[];
  authenticated: boolean;
};

export const slug = (method: string, path: string) =>
  `${method}-${path
    .replace(/[{}]/g, "")
    .replace(/[^a-z0-9]+/gi, "-")
    .replace(/^-|-$/g, "")}`.toLowerCase();

export function endpoints(doc: OpenApiDocument): Endpoint[] {
  const resolve = resolver(doc);
  const list: Endpoint[] = [];
  for (const [path, item] of Object.entries(doc.paths)) {
    for (const method of methods) {
      const op = item[method];
      if (!op) continue;
      const body = op.requestBody ? resolve(op.requestBody) : undefined;
      const json = body?.content?.["application/json"];
      const responses = Object.entries(op.responses ?? {}).map(([code, raw]) => {
        const r = resolve(raw);
        const content = r?.content ?? {};
        return {
          code,
          description: r?.description ?? "",
          schema: content["application/json"]?.schema ?? null,
          text: !content["application/json"] && "text/plain" in content,
        };
      });
      const security = op.security ?? doc.security ?? [];
      list.push({
        id: slug(method, path),
        method,
        path,
        op,
        params: (op.parameters ?? []).map((p) => resolve(p)),
        body: json?.schema ? resolve(json.schema) : null,
        bodyExample: json?.example ?? (json?.schema ? exampleFor(json.schema, resolve) : undefined),
        responses,
        authenticated: security.length > 0,
      });
    }
  }
  return list;
}

/** Endpoints grouped by their first tag, in the order the spec lists its tags. */
export function byTag(doc: OpenApiDocument, list = endpoints(doc)) {
  const groups = new Map<string, Endpoint[]>();
  for (const tag of doc.tags ?? []) groups.set(tag.name, []);
  for (const e of list) {
    const tag = e.op.tags?.[0] ?? "Endpoints";
    if (!groups.has(tag)) groups.set(tag, []);
    groups.get(tag)?.push(e);
  }
  return [...groups]
    .filter(([, entries]) => entries.length)
    .map(([name, entries]) => ({
      name,
      description: doc.tags?.find((t) => t.name === name)?.description,
      entries,
    }));
}

/** A value built from the schema's examples, enums and types, following $refs. */
export function exampleFor(raw: Schema | Ref, resolve: Resolve, depth = 0): unknown {
  const schema = resolve(raw);
  if (!schema || depth > 6) return null;
  if (schema.example !== undefined) return schema.example;
  if (schema.oneOf?.length) return exampleFor(schema.oneOf[0], resolve, depth + 1);
  if (schema.enum?.length) return schema.enum[0];
  const type = Array.isArray(schema.type) ? schema.type.find((t) => t !== "null") : schema.type;
  if (type === "array") return schema.items ? [exampleFor(schema.items, resolve, depth + 1)] : [];
  if (type === "object" || schema.properties) {
    return Object.fromEntries(
      Object.entries(schema.properties ?? {}).map(([name, prop]) => [
        name,
        exampleFor(prop, resolve, depth + 1),
      ]),
    );
  }
  if (type === "integer" || type === "number") return schema.minimum ?? 0;
  if (type === "boolean") return false;
  return "string";
}

/** Short type label: `string`, `integer | null`, `array of Output`, `JobStatus`. */
export function typeLabel(raw: Schema | Ref | undefined): string {
  if (!raw) return "any";
  if (isRef(raw)) return refName(raw.$ref);
  if (raw.oneOf) return raw.oneOf.map((s) => typeLabel(s)).join(" | ");
  if (raw.type === "array") return `array of ${typeLabel(raw.items)}`;
  const type = Array.isArray(raw.type) ? raw.type.join(" | ") : (raw.type ?? "object");
  return raw.format && raw.format !== "uri" ? `${type} (${raw.format})` : type;
}

/** Enum values with their descriptions, in spec order. */
export function enumValues(schema: Schema | undefined) {
  return (schema?.enum ?? []).map((value) => ({
    value,
    description: schema?.["x-enum-descriptions"]?.[value] ?? "",
  }));
}
