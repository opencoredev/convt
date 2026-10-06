import { cx, focusRing } from "#/components/app/ui";

import { siteColumn } from "./layout";
import { Inline } from "./markdown";

// Renders an OpenAPI 3.x document as the API reference. Handles the subset convt-server
// uses: tagged operations, path and query parameters, JSON request and response bodies,
// and component schemas referenced with $ref. Unknown fields are ignored.

type Ref = { $ref: string };
export type Schema = {
  type?: string;
  format?: string;
  description?: string;
  enum?: string[];
  example?: unknown;
  minimum?: number;
  items?: Schema | Ref;
  properties?: Record<string, Schema | Ref>;
  required?: string[];
};
type MediaTypes = Record<string, { schema?: Schema | Ref }>;
type Parameter = {
  name: string;
  in: string;
  required?: boolean;
  description?: string;
  schema?: Schema | Ref;
};
type Response = { description?: string; content?: MediaTypes };
type Operation = {
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
  info: { title: string; version: string; description?: string };
  servers?: { url: string }[];
  tags?: { name: string; description?: string }[];
  paths: Record<string, Record<string, Operation>>;
  components?: {
    schemas?: Record<string, Schema>;
    parameters?: Record<string, Parameter>;
    responses?: Record<string, Response>;
    securitySchemes?: Record<string, { type: string; scheme?: string; description?: string }>;
  };
};

const methods = ["get", "post", "put", "patch", "delete"] as const;

const isRef = (value: unknown): value is Ref =>
  typeof value === "object" && value !== null && "$ref" in value;
const refName = (ref: string) => ref.split("/").pop() ?? ref;

function resolver(doc: OpenApiDocument) {
  return function resolve<T>(value: T | Ref): T {
    if (!isRef(value)) return value;
    const [, , group, name] = value.$ref.split("/");
    const components = doc.components as Record<string, Record<string, unknown>> | undefined;
    return components?.[group]?.[name] as T;
  };
}

const slug = (method: string, path: string) =>
  `${method}-${path
    .replace(/[{}]/g, "")
    .replace(/[^a-z0-9]+/gi, "-")
    .replace(/^-|-$/g, "")}`;

type Entry = { method: string; path: string; op: Operation; id: string };

function groupByTag(doc: OpenApiDocument) {
  const groups = new Map<string, Entry[]>();
  for (const tag of doc.tags ?? []) groups.set(tag.name, []);
  for (const [path, item] of Object.entries(doc.paths)) {
    for (const method of methods) {
      const op = item[method];
      if (!op) continue;
      const tag = op.tags?.[0] ?? "Endpoints";
      if (!groups.has(tag)) groups.set(tag, []);
      groups.get(tag)?.push({ method, path, op, id: op.operationId ?? slug(method, path) });
    }
  }
  return [...groups].filter(([, entries]) => entries.length);
}

const methodTone: Record<string, string> = {
  get: "text-[#157f4a] dark:text-green bg-green-tint",
  post: "text-[#2f6fdb] bg-[#eaf1fd] dark:text-[#8db8ff] dark:bg-[#14223a]",
  delete: "text-error bg-[#fbeceb] dark:bg-[#33171a]",
};

function Method({ method }: { method: string }) {
  return (
    <span
      className={cx(
        "inline-flex w-[52px] shrink-0 justify-center rounded-[5px] py-0.5 font-mono text-[11px]/4 font-medium uppercase",
        methodTone[method] ?? "bg-chip text-ink-2",
      )}
    >
      {method}
    </span>
  );
}

export function ApiReference({ doc }: { doc: OpenApiDocument }) {
  const resolve = resolver(doc);
  const groups = groupByTag(doc);
  const server = doc.servers?.[0]?.url ?? "";
  const schemas = Object.entries(doc.components?.schemas ?? {});
  const auth = Object.values(doc.components?.securitySchemes ?? {})[0];

  return (
    <div className={cx(siteColumn, "grid gap-10 pt-10 pb-20 lg:grid-cols-[220px_1fr] lg:gap-14")}>
      {/* Phones get a collapsed list above the content; wide screens a sticky sidebar. */}
      <details className="group rounded-xl bg-sunken shadow-[inset_0_0_0_1px_var(--line)] lg:hidden">
        <summary
          className={cx(
            "flex cursor-pointer list-none items-center justify-between px-4 py-3 text-sm/5 font-medium [&::-webkit-details-marker]:hidden",
            focusRing,
          )}
        >
          On this page
          <span aria-hidden="true" className="text-ink-2 group-open:rotate-180">
            ▾
          </span>
        </summary>
        <nav aria-label="API reference" className="flex flex-col gap-6 px-4 pb-4">
          <ApiNav groups={groups} schemas={schemas} />
        </nav>
      </details>
      <nav
        aria-label="API reference"
        className="hidden flex-col gap-6 lg:sticky lg:top-6 lg:flex lg:max-h-[calc(100vh-48px)] lg:self-start lg:overflow-y-auto"
      >
        <ApiNav groups={groups} schemas={schemas} />
      </nav>

      <div className="flex min-w-0 flex-col gap-14">
        <header id="introduction" className="flex max-w-[680px] scroll-mt-6 flex-col gap-3">
          <p className="font-mono text-xs/4 text-ink-2 uppercase">
            API reference · {doc.info.version}
          </p>
          <h1 className="text-[34px]/10 font-semibold tracking-[-0.03em] md:text-[44px]/12">
            {doc.info.title}
          </h1>
          {doc.info.description && (
            <p className="text-[17px]/[26px] text-ink-2">
              <Inline text={doc.info.description} />
            </p>
          )}
          {server && (
            <p className="flex flex-wrap items-center gap-2 pt-1 text-sm/5 text-ink-2">
              Base URL
              <code className="rounded-md bg-chip px-2 py-0.5 font-mono text-[13px] text-ink shadow-[inset_0_0_0_1px_var(--chip-line)]">
                {server}
              </code>
            </p>
          )}
        </header>

        <section
          id="authentication"
          aria-labelledby="auth-title"
          className="flex max-w-[680px] scroll-mt-6 flex-col gap-3"
        >
          <h2 id="auth-title" className="text-2xl/8 font-semibold tracking-[-0.02em]">
            Authentication
          </h2>
          <p className="text-[15px]/6 text-ink-2">
            {auth?.description ? (
              <Inline text={auth.description} />
            ) : (
              "Send your API key as a bearer token."
            )}{" "}
            Create keys on the dashboard under API. Keys work once a payment method is on file.
          </p>
        </section>

        {groups.map(([tag, entries]) => (
          <section key={tag} aria-labelledby={`tag-${tag}`} className="flex flex-col gap-6">
            <div className="flex flex-col gap-1.5 border-b border-line pb-4">
              <h2 id={`tag-${tag}`} className="text-2xl/8 font-semibold tracking-[-0.02em]">
                {tag}
              </h2>
              {doc.tags?.find((t) => t.name === tag)?.description && (
                <p className="text-[15px]/6 text-ink-2">
                  {doc.tags.find((t) => t.name === tag)?.description}
                </p>
              )}
            </div>
            {entries.map((entry) => (
              <OperationView key={entry.id} entry={entry} server={server} resolve={resolve} />
            ))}
          </section>
        ))}

        {schemas.length > 0 && (
          <section aria-labelledby="objects-title" className="flex flex-col gap-6">
            <h2
              id="objects-title"
              className="border-b border-line pb-4 text-2xl/8 font-semibold tracking-[-0.02em]"
            >
              Objects
            </h2>
            {schemas.map(([name, schema]) => (
              <article key={name} id={`schema-${name}`} className="flex scroll-mt-6 flex-col gap-3">
                <h3 className="font-mono text-[15px]/6 font-medium">{name}</h3>
                {schema.description && (
                  <p className="text-sm/[21px] text-ink-2">{schema.description}</p>
                )}
                {schema.properties ? (
                  <Fields schema={schema} resolve={resolve} />
                ) : (
                  <p className="font-mono text-[13px] text-ink-2">{typeLabel(schema)}</p>
                )}
              </article>
            ))}
          </section>
        )}
      </div>
    </div>
  );
}

function ApiNav({
  groups,
  schemas,
}: {
  groups: ReturnType<typeof groupByTag>;
  schemas: [string, Schema][];
}) {
  return (
    <>
      <NavGroup
        title="Overview"
        items={[
          { id: "introduction", label: "Introduction" },
          { id: "authentication", label: "Authentication" },
        ]}
      />
      {groups.map(([tag, entries]) => (
        <NavGroup
          key={tag}
          title={tag}
          items={entries.map((e) => ({
            id: e.id,
            label: e.op.summary ?? e.path,
            method: e.method,
          }))}
        />
      ))}
      {schemas.length > 0 && (
        <NavGroup
          title="Objects"
          items={schemas.map(([name]) => ({ id: `schema-${name}`, label: name }))}
        />
      )}
    </>
  );
}

function NavGroup({
  title,
  items,
}: {
  title: string;
  items: { id: string; label: string; method?: string }[];
}) {
  return (
    <div className="flex flex-col gap-2">
      <h2 className="font-mono text-[11px]/3.5 text-ink-2 uppercase">{title}</h2>
      <ul className="flex flex-col gap-1.5">
        {items.map((item) => (
          <li key={item.id}>
            <a
              href={`#${item.id}`}
              className={cx(
                "flex items-center gap-2 rounded-sm text-[13px]/5 text-ink-2 hover:text-ink",
                focusRing,
              )}
            >
              {item.method && (
                <span className="w-9 shrink-0 font-mono text-[10px]/4 text-ink-2 uppercase">
                  {item.method}
                </span>
              )}
              {item.label}
            </a>
          </li>
        ))}
      </ul>
    </div>
  );
}

type Resolve = ReturnType<typeof resolver>;

function typeLabel(schema: Schema | Ref | undefined): string {
  if (!schema) return "any";
  if (isRef(schema)) return refName(schema.$ref);
  if (schema.type === "array") return `array of ${typeLabel(schema.items)}`;
  return schema.format ? `${schema.type} (${schema.format})` : (schema.type ?? "object");
}

function TypeLink({ schema }: { schema: Schema | Ref | undefined }) {
  const target = isRef(schema)
    ? schema
    : schema && !isRef(schema) && isRef(schema.items)
      ? schema.items
      : null;
  const label = typeLabel(schema);
  if (!target) return <>{label}</>;
  return (
    <a
      href={`#schema-${refName(target.$ref)}`}
      className={cx("rounded-sm text-[#157f4a] dark:text-green hover:underline", focusRing)}
    >
      {label}
    </a>
  );
}

function Fields({ schema, resolve }: { schema: Schema; resolve: Resolve }) {
  const required = new Set(schema.required ?? []);
  return (
    <ul className="divide-y divide-divider rounded-xl bg-raised shadow-[inset_0_0_0_1px_var(--line)] dark:bg-panel">
      {Object.entries(schema.properties ?? {}).map(([name, raw]) => {
        const prop = resolve(raw);
        const description = (!isRef(raw) && raw.description) || prop?.description;
        return (
          <li key={name} className="flex flex-col gap-1 px-4 py-3">
            <div className="flex flex-wrap items-baseline gap-x-2.5 gap-y-1 font-mono text-[13px]/5">
              <span className="font-medium text-ink">{name}</span>
              <span className="text-xs text-ink-2">
                <TypeLink schema={raw} />
              </span>
              {required.has(name) && <span className="text-[11px] text-error">required</span>}
            </div>
            {description && (
              <p className="text-sm/[21px] text-ink-2">
                <Inline text={description} />
              </p>
            )}
            {prop?.enum && (
              <p className="flex flex-wrap gap-1 pt-0.5">
                {prop.enum.map((value) => (
                  <code
                    key={value}
                    className="rounded bg-chip px-1.5 font-mono text-[11.5px]/[18px] text-ink-2"
                  >
                    {value}
                  </code>
                ))}
              </p>
            )}
          </li>
        );
      })}
    </ul>
  );
}

function jsonSchema(content: MediaTypes | undefined) {
  return content?.["application/json"]?.schema;
}

// A request body with every required field. Fields without an example in the spec get
// a placeholder (`"<name>"`, the minimum for numbers) so the request is never silently
// incomplete.
function example(schema: Schema | undefined, resolve: Resolve) {
  if (!schema?.properties) return null;
  const body: Record<string, unknown> = {};
  for (const name of schema.required ?? []) {
    const prop = resolve(schema.properties[name]);
    if (prop?.example !== undefined) body[name] = prop.example;
    else if (prop?.enum?.length) body[name] = prop.enum[0];
    else if (prop?.type === "integer" || prop?.type === "number") body[name] = prop.minimum ?? 1;
    else if (prop?.type === "boolean") body[name] = false;
    else body[name] = `<${name}>`;
  }
  return body;
}

function OperationView({
  entry,
  server,
  resolve,
}: {
  entry: Entry;
  server: string;
  resolve: Resolve;
}) {
  const { method, path, op } = entry;
  const params = (op.parameters ?? []).map((p) => resolve(p));
  const body = op.requestBody ? resolve(op.requestBody) : undefined;
  const bodySchema = body ? resolve(jsonSchema(body.content) ?? {}) : undefined;
  const responses = Object.entries(op.responses ?? {}).map(
    ([code, r]) => [code, resolve(r)] as const,
  );
  const authenticated = !(op.security && op.security.length === 0);
  // An inline success schema has no Objects entry to link to, so show its fields here.
  const [successCode, successResponse] = responses.find(([code]) => code.startsWith("2")) ?? [];
  const successRaw = jsonSchema(successResponse?.content);
  const success = successRaw && !isRef(successRaw) ? successRaw : undefined;
  const sample = example(bodySchema, resolve);
  const curl = [
    `curl${method === "get" ? "" : ` -X ${method.toUpperCase()}`} ${server}${path.replace(/\{(\w+)\}/g, (_, name) => `${(resolve(params.find((p) => p.name === name)?.schema ?? {}) as Schema).example ?? name}`)}`,
    authenticated && `  -H "Authorization: Bearer $CONVT_API_KEY"`,
    sample && `  -H "Content-Type: application/json"`,
    sample && `  -d '${JSON.stringify(sample, null, 2).replace(/\n/g, "\n    ")}'`,
  ]
    .filter(Boolean)
    .join(" \\\n");

  return (
    <article
      id={entry.id}
      aria-labelledby={`${entry.id}-title`}
      className="grid scroll-mt-6 gap-5 xl:grid-cols-[minmax(0,1fr)_minmax(0,400px)] xl:gap-8"
    >
      <div className="flex min-w-0 flex-col gap-4">
        <div className="flex flex-col gap-2">
          <h3 id={`${entry.id}-title`} className="text-lg/6 font-semibold">
            {op.summary ?? path}
          </h3>
          <p className="flex min-w-0 items-center gap-2.5">
            <Method method={method} />
            <code className="truncate font-mono text-[13px]/5 text-ink">{path}</code>
          </p>
        </div>
        {op.description && (
          <p className="text-[15px]/6 text-ink-2">
            <Inline text={op.description} />
          </p>
        )}
        {params.length > 0 && (
          <div className="flex flex-col gap-2">
            <h4 className="font-mono text-[11px]/3.5 text-ink-2 uppercase">Parameters</h4>
            <Fields
              resolve={resolve}
              schema={{
                properties: Object.fromEntries(
                  params.map((p) => [
                    p.name,
                    {
                      ...resolve(p.schema ?? {}),
                      description: `${p.in}${p.description ? `. ${p.description}` : ""}`,
                    },
                  ]),
                ),
                required: params.filter((p) => p.required).map((p) => p.name),
              }}
            />
          </div>
        )}
        {bodySchema?.properties && (
          <div className="flex flex-col gap-2">
            <h4 className="font-mono text-[11px]/3.5 text-ink-2 uppercase">Request body</h4>
            <Fields schema={bodySchema} resolve={resolve} />
          </div>
        )}
        {success?.properties && (
          <div className="flex flex-col gap-2">
            <h4 className="font-mono text-[11px]/3.5 text-ink-2 uppercase">
              Response body · {successCode}
            </h4>
            <Fields schema={success} resolve={resolve} />
          </div>
        )}
        <div className="flex flex-col gap-2">
          <h4 className="font-mono text-[11px]/3.5 text-ink-2 uppercase">Responses</h4>
          <ul className="flex flex-col gap-1.5">
            {responses.map(([code, response]) => (
              <li key={code} className="flex flex-wrap items-baseline gap-x-3 gap-y-0.5 text-sm/5">
                <span
                  className={cx(
                    "w-9 font-mono text-[13px]",
                    code.startsWith("2") ? "text-[#157f4a] dark:text-green" : "text-error",
                  )}
                >
                  {code}
                </span>
                <span className="text-ink-2">{response?.description}</span>
                {jsonSchema(response?.content) && (
                  <span className="font-mono text-xs text-ink-2">
                    <TypeLink schema={jsonSchema(response?.content)} />
                  </span>
                )}
              </li>
            ))}
          </ul>
        </div>
      </div>
      <div className="min-w-0 xl:pt-[60px]">
        <div className="flex flex-col overflow-clip rounded-[10px] bg-code shadow-[inset_0_0_0_1px_var(--code-ring)]">
          <div className="flex h-[34px] shrink-0 items-center px-3.5 font-mono text-[11px]/[14px] text-[#6f7571] shadow-[inset_0_-1px_0_#ffffff0f]">
            Request
          </div>
          <pre className="overflow-x-auto p-3.5 font-mono text-[12px]/5 text-code-ink">
            <code>{curl}</code>
          </pre>
        </div>
      </div>
    </article>
  );
}
