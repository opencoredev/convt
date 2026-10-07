import { useId, useRef, useState } from "react";

import { cx } from "./ui";

// Each example runs the whole jobs flow against the API: reserve, upload, start,
// poll, download. They are tested as written; keep them runnable.
function samplesFor(api: string) {
  return [
    {
      id: "curl",
      label: "cURL",
      code: `# Needs curl and jq. Set CONVT_KEY to your API key.
api=${api}
auth="Authorization: Bearer $CONVT_KEY"
job=$(curl -fsS $api/v1/jobs -H "$auth" -H "Content-Type: application/json" \\
  -d "{\\"input_format\\":\\"png\\",\\"target_format\\":\\"webp\\",\\"input_bytes\\":$(wc -c < photo.png)}")
id=$(jq -r .job.id <<<"$job")
curl -fsS -X PUT --upload-file photo.png "$(jq -r .upload_url <<<"$job")"
curl -fsS -X POST $api/v1/jobs/$id/start -H "$auth" >/dev/null
while status=$(curl -fsS $api/v1/jobs/$id -H "$auth" | jq -r .status);
  [ "$status" = queued ] || [ "$status" = running ]; do sleep 1; done
echo "$status"
curl -fsS $api/v1/jobs/$id/download -H "$auth" \\
  | jq -r '.outputs[0].url' | xargs curl -fsS -o photo.webp`,
    },
    {
      id: "node",
      label: "Node",
      code: `// Node 20 or later. Set CONVT_KEY to your API key.
import { readFile, writeFile } from "node:fs/promises";

const api = "${api}";
const auth = { Authorization: \`Bearer \${process.env.CONVT_KEY}\` };
const call = (path, init = {}) =>
  fetch(api + path, { ...init, headers: { ...auth, ...init.headers } }).then((r) => r.json());

const input = await readFile("photo.png");
const created = await call("/v1/jobs", {
  method: "POST",
  headers: { "Content-Type": "application/json" },
  body: JSON.stringify({ input_format: "png", target_format: "webp", input_bytes: input.length }),
});
await fetch(created.upload_url, { method: "PUT", body: input });
const id = created.job.id;
let job = await call(\`/v1/jobs/\${id}/start\`, { method: "POST" });
while (job.status === "queued" || job.status === "running") {
  await new Promise((r) => setTimeout(r, 1000));
  job = await call(\`/v1/jobs/\${id}\`);
}
const { outputs } = await call(\`/v1/jobs/\${id}/download\`);
await writeFile("photo.webp", Buffer.from(await (await fetch(outputs[0].url)).arrayBuffer()));`,
    },
    {
      id: "cli",
      label: "CLI",
      code: `# The desktop CLI converts on your machine.
# It needs no API key and uploads nothing.
convt photo.png --to webp`,
    },
  ] as const;
}

type SampleId = ReturnType<typeof samplesFor>[number]["id"];

export function CodeSample({ apiUrl }: { apiUrl: string }) {
  const samples = samplesFor(apiUrl);
  const base = useId();
  const [active, setActive] = useState<SampleId>("curl");
  const [copied, setCopied] = useState(false);
  const tabRefs = useRef<Array<HTMLButtonElement | null>>([]);
  const current = samples.find((s) => s.id === active) ?? samples[0];

  function onKeyDown(event: React.KeyboardEvent, index: number) {
    let next = index;
    if (event.key === "ArrowRight") next = (index + 1) % samples.length;
    else if (event.key === "ArrowLeft") next = (index - 1 + samples.length) % samples.length;
    else if (event.key === "Home") next = 0;
    else if (event.key === "End") next = samples.length - 1;
    else return;
    event.preventDefault();
    setActive(samples[next].id);
    setCopied(false);
    tabRefs.current[next]?.focus();
  }

  async function copy() {
    try {
      await navigator.clipboard.writeText(current.code);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      setCopied(false);
    }
  }

  return (
    <div className="flex min-w-0 flex-1 flex-col overflow-clip rounded-xl bg-code ring-1 ring-code-ring">
      <div className="flex h-11 shrink-0 items-center justify-between border-b border-white/8 pr-4 pl-3">
        <div role="tablist" aria-label="Language" className="flex items-center gap-1">
          {samples.map((sample, index) => {
            const selected = sample.id === active;
            return (
              <button
                key={sample.id}
                ref={(el) => {
                  tabRefs.current[index] = el;
                }}
                type="button"
                role="tab"
                id={`${base}-tab-${sample.id}`}
                aria-selected={selected}
                aria-controls={`${base}-panel`}
                tabIndex={selected ? 0 : -1}
                onClick={() => {
                  setActive(sample.id);
                  setCopied(false);
                }}
                onKeyDown={(e) => onKeyDown(e, index)}
                className={cx(
                  "cursor-pointer rounded-md px-2.5 py-1.5 text-xs/4 font-medium outline-none focus-visible:ring-2 focus-visible:ring-[#3fcb84]",
                  selected ? "bg-white/8 text-[#edefee]" : "text-[#8a8f8c] hover:text-[#edefee]",
                )}
              >
                {sample.label}
              </button>
            );
          })}
        </div>
        <button
          type="button"
          onClick={copy}
          className="flex cursor-pointer items-center gap-1.5 rounded-sm text-xs/4 font-medium text-[#8a8f8c] outline-none hover:text-[#edefee] focus-visible:ring-2 focus-visible:ring-[#3fcb84]"
        >
          <svg width="13" height="13" viewBox="0 0 13 13" aria-hidden="true">
            <path
              d="M4.5 4.5 H10.5 V10.5 H4.5 Z"
              fill="none"
              stroke="currentColor"
              strokeWidth="1.2"
              strokeLinejoin="round"
            />
            <path
              d="M2.5 8.5 V2.5 H8.5"
              fill="none"
              stroke="currentColor"
              strokeWidth="1.2"
              strokeLinecap="round"
              strokeLinejoin="round"
            />
          </svg>
          <span aria-live="polite">{copied ? "Copied" : "Copy"}</span>
        </button>
      </div>
      <div
        role="tabpanel"
        id={`${base}-panel`}
        aria-labelledby={`${base}-tab-${current.id}`}
        tabIndex={0}
        className="min-h-[196px] overflow-x-auto px-6 py-5 outline-none focus-visible:ring-2 focus-visible:ring-[#3fcb84] focus-visible:ring-inset"
      >
        <pre className="m-0 w-max font-mono text-xs/5 text-code-ink">
          <code>{current.code}</code>
        </pre>
      </div>
    </div>
  );
}
