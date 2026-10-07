// Code samples for the API reference and the dashboard. Each quick start is a complete
// program that runs the whole job flow: create, upload, start, poll, download. They use
// plain HTTP because @convt/sdk is not published to npm yet.

import type { Endpoint } from "./openapi";

export type Language = "curl" | "node" | "python" | "cli";
export type Sample = { language: Language; label: string; code: string };

export const languageLabels: Record<Language, string> = {
  curl: "cURL",
  node: "Node.js",
  python: "Python",
  cli: "CLI",
};

export function quickStart(base: string): Sample[] {
  return [
    {
      language: "curl",
      label: languageLabels.curl,
      code: `# Needs curl 7.76+ and jq. Converts photo.png to photo.webp.
API=${base}
AUTH="Authorization: Bearer $CONVT_API_KEY"
size=$(wc -c < photo.png | tr -d ' ')

# 1. Reserve the job and get an upload URL
created=$(curl -sS --fail-with-body "$API/v1/jobs" -H "$AUTH" \\
  -H "Content-Type: application/json" \\
  -d "{\\"input_format\\":\\"png\\",\\"target_format\\":\\"webp\\",\\"input_bytes\\":$size}")
id=$(jq -r .job.id <<<"$created")

# 2. Upload exactly input_bytes, then start
curl -sS --fail-with-body -X PUT --upload-file photo.png "$(jq -r .upload_url <<<"$created")"
curl -sS --fail-with-body -X POST "$API/v1/jobs/$id/start" -H "$AUTH"

# 3. Poll every 2 seconds until the job finishes
while :; do
  status=$(curl -sS --fail-with-body "$API/v1/jobs/$id" -H "$AUTH" | jq -r .status)
  case $status in succeeded|failed|cancelled) break ;; esac
  sleep 2
done

# 4. Download the output
curl -sS --fail-with-body "$API/v1/jobs/$id/download" -H "$AUTH" \\
  | jq -r '.outputs[0].url' | xargs curl -sS --fail-with-body -o photo.webp`,
    },
    {
      language: "node",
      label: languageLabels.node,
      code: `// Node 18 or newer, as an ES module. Converts photo.png to photo.webp.
import { readFile, writeFile } from "node:fs/promises";

const API = "${base}";
const auth = { Authorization: \`Bearer \${process.env.CONVT_API_KEY}\` };

async function api(path, init = {}) {
  const res = await fetch(API + path, { ...init, headers: { ...auth, ...init.headers } });
  const body = await res.json().catch(() => ({}));
  if (!res.ok) throw new Error(body.error?.code ?? \`HTTP \${res.status}\`);
  return body;
}

// 1. Reserve the job and get an upload URL
const file = await readFile("photo.png");
const { job, upload_url } = await api("/v1/jobs", {
  method: "POST",
  headers: { "Content-Type": "application/json" },
  body: JSON.stringify({ input_format: "png", target_format: "webp", input_bytes: file.byteLength }),
});

// 2. Upload exactly input_bytes, then start
const put = await fetch(upload_url, { method: "PUT", body: file });
if (!put.ok) throw new Error(\`upload failed: \${put.status}\`);
let current = await api(\`/v1/jobs/\${job.id}/start\`, { method: "POST" });

// 3. Poll every 2 seconds until the job finishes
while (!["succeeded", "failed", "cancelled"].includes(current.status)) {
  await new Promise((resolve) => setTimeout(resolve, 2000));
  current = await api(\`/v1/jobs/\${job.id}\`);
}
if (current.status !== "succeeded") throw new Error(current.error_code ?? current.status);

// 4. Download the output
const { outputs } = await api(\`/v1/jobs/\${job.id}/download\`);
const output = await fetch(outputs[0].url);
await writeFile("photo.webp", Buffer.from(await output.arrayBuffer()));`,
    },
    {
      language: "python",
      label: languageLabels.python,
      code: `# Python 3 with requests. Converts photo.png to photo.webp.
import os, time, requests

API = "${base}"
AUTH = {"Authorization": f"Bearer {os.environ['CONVT_API_KEY']}"}

def api(method, path, **kwargs):
    res = requests.request(method, API + path, headers=AUTH, **kwargs)
    if not res.ok:
        raise RuntimeError(f"{res.status_code}: {res.text}")
    return res.json()

# 1. Reserve the job and get an upload URL
data = open("photo.png", "rb").read()
created = api("POST", "/v1/jobs", json={
    "input_format": "png", "target_format": "webp", "input_bytes": len(data),
})
job_id = created["job"]["id"]

# 2. Upload exactly input_bytes, then start
requests.put(created["upload_url"], data=data).raise_for_status()
job = api("POST", f"/v1/jobs/{job_id}/start")

# 3. Poll every 2 seconds until the job finishes
while job["status"] not in ("succeeded", "failed", "cancelled"):
    time.sleep(2)
    job = api("GET", f"/v1/jobs/{job_id}")
if job["status"] != "succeeded":
    raise RuntimeError(job["error_code"] or job["status"])

# 4. Download the output
url = api("GET", f"/v1/jobs/{job_id}/download")["outputs"][0]["url"]
open("photo.webp", "wb").write(requests.get(url).content)`,
    },
    {
      language: "cli",
      label: languageLabels.cli,
      code: `# The convt CLI ships with the desktop app and converts on your machine.
# It needs no API key and uploads nothing.
convt photo.png --to webp`,
    },
  ];
}

function pathWithExamples(endpoint: Endpoint) {
  return endpoint.path.replace(/\{(\w+)\}/g, (_, name: string) => {
    const param = endpoint.params.find((p) => p.name === name);
    const example = param?.schema && "example" in param.schema ? param.schema.example : undefined;
    return typeof example === "string" ? example : `{${name}}`;
  });
}

const pyLiteral = (value: unknown): string =>
  JSON.stringify(value, null, 4)
    .replace(/\bnull\b/g, "None")
    .replace(/\btrue\b/g, "True")
    .replace(/\bfalse\b/g, "False");

/** cURL, Node.js and Python for one operation, with the spec's example values. */
export function endpointSamples(endpoint: Endpoint, base: string): Sample[] {
  const method = endpoint.method.toUpperCase();
  const url = `${base}${pathWithExamples(endpoint)}`;
  const body = endpoint.bodyExample;
  const json = body === undefined ? null : JSON.stringify(body, null, 2);

  const curl = [
    `curl${method === "GET" ? "" : ` -X ${method}`} ${url}`,
    endpoint.authenticated && `  -H "Authorization: Bearer $CONVT_API_KEY"`,
    json && `  -H "Content-Type: application/json"`,
    json && `  -d '${json.replace(/\n/g, "\n  ")}'`,
  ]
    .filter(Boolean)
    .join(" \\\n");

  const headers = [
    endpoint.authenticated && "Authorization: `Bearer ${process.env.CONVT_API_KEY}`",
    json && `"Content-Type": "application/json"`,
  ].filter(Boolean);
  const init = [
    method !== "GET" && `method: "${method}"`,
    headers.length && `headers: {\n    ${headers.join(",\n    ")},\n  }`,
    json && `body: JSON.stringify(${json.replace(/\n/g, "\n  ")})`,
  ].filter(Boolean);
  const node = `const res = await fetch("${url}"${
    init.length ? `, {\n  ${init.join(",\n  ")},\n}` : ""
  });
const body = await res.json();`;

  const pyArgs = [
    `"${method}"`,
    `"${url}"`,
    endpoint.authenticated && `headers={"Authorization": f"Bearer {os.environ['CONVT_API_KEY']}"}`,
    body !== undefined && `json=${pyLiteral(body).replace(/\n/g, "\n    ")}`,
  ].filter(Boolean);
  const python = `${endpoint.authenticated ? "import os, requests" : "import requests"}

res = requests.request(
    ${pyArgs.join(",\n    ")},
)
body = res.json()`;

  return [
    { language: "curl", label: languageLabels.curl, code: curl },
    { language: "node", label: languageLabels.node, code: node },
    { language: "python", label: languageLabels.python, code: python },
  ];
}
