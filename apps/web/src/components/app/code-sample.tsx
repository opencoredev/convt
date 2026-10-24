import { Copy01Icon } from "@hugeicons/core-free-icons";
import { useId, useRef, useState } from "react";

import { Icon } from "#/components/icon";
import { apiBaseUrl } from "#/lib/config";

import { cx } from "./ui";

// Each example follows the jobs API. The SDK handles upload and polling.
const samples = [
  {
    id: "node",
    label: "Node",
    code: `import { Convt } from "@convt/sdk";

// Set CONVT_API_KEY in your environment.
const convt = new Convt();
const out = await convt.convert("report.docx", {
  to: "pdf",
});
await out.save("report.pdf");`,
  },
  {
    id: "curl",
    label: "cURL",
    code: `# Create a reservation for a 104-byte SVG.
job=$(curl -fsS ${apiBaseUrl}/v1/jobs \\
  -H "Authorization: Bearer $CONVT_KEY" \\
  -H "Content-Type: application/json" \\
  -d '{"input_format":"svg","target_format":"png","input_bytes":104}')
# PUT your file to upload_url, then POST /v1/jobs/{id}/start.
# Poll GET /v1/jobs/{id}; on success, GET its /download URLs.
# The SDK performs each of these steps for you.`,
  },
  {
    id: "browser",
    label: "Browser",
    code: `import { Convt } from "@convt/sdk";

// Use a short-lived token issued by your server.
const convt = new Convt({
  token: () => fetchToken(),
  baseUrl: "${apiBaseUrl}",
});
const out = await convt.convert(file, { to: "pdf" });
const blob = await out.blob();`,
  },
  {
    id: "cli",
    label: "CLI",
    code: `# The desktop CLI converts on your machine.
# It needs no API key and uploads nothing.
convt report.docx --to pdf`,
  },
] as const;

type SampleId = (typeof samples)[number]["id"];

export function CodeSample() {
  const base = useId();
  const [active, setActive] = useState<SampleId>("node");
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
          <Icon icon={Copy01Icon} size={13} strokeWidth={1.6} />
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
