import { useId, useRef, useState } from "react";

import { apiBaseUrl } from "#/lib/config";

import { cx } from "./ui";

// PLACEHOLDER: the API is not built (plan P9). The cURL sample is the one in the
// design; the others show the same request. Update all four when the real endpoint
// and `@convt/sdk` exist.
const samples = [
  {
    id: "curl",
    label: "cURL",
    code: `curl ${apiBaseUrl}/v1/convert \\
  -H "Authorization: Bearer $CONVT_KEY" \\
  -F file=@interview.mov \\
  -F to=mp4 \\
  -o interview.mp4`,
  },
  {
    id: "node",
    label: "Node",
    code: `import { Convt } from "@convt/sdk";

const convt = new Convt();
const out = await convt.convert("interview.mov", {
  to: "mp4",
});
await out.save("interview.mp4");`,
  },
  {
    id: "python",
    label: "Python",
    code: `import os, requests

with open("interview.mov", "rb") as f:
    r = requests.post(
        "${apiBaseUrl}/v1/convert",
        headers={"Authorization": f"Bearer {os.environ['CONVT_KEY']}"},
        files={"file": f},
        data={"to": "mp4"},
    )
r.raise_for_status()
open("interview.mp4", "wb").write(r.content)`,
  },
  {
    id: "cli",
    label: "CLI",
    code: `# The convt CLI converts on your machine.
# It needs no API key and uploads nothing.
convt interview.mov --to mp4`,
  },
] as const;

type SampleId = (typeof samples)[number]["id"];

export function CodeSample() {
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
