import {
  createContext,
  useContext,
  useEffect,
  useId,
  useRef,
  useState,
  type ReactNode,
} from "react";

import type { Language, Sample } from "#/lib/api-samples";

import { cx } from "./ui";

// Dark code surface used by the API reference and the dashboard, in both themes. The
// colors are the landing page's code palette (components/landing/pricing.tsx).

type Token = { text: string; tone?: keyof typeof tones };
const tones = {
  comment: "text-[#838985] italic",
  string: "text-[#f2c46d]",
  keyword: "text-[#7fd3a6]",
  number: "text-[#f5a97f]",
  property: "text-[#9cc9ff]",
  flag: "text-[#9cc9ff]",
  variable: "text-[#c9b6ff]",
};

const keywords: Record<string, RegExp> = {
  node: /^(?:import|from|const|let|await|async|function|return|if|throw|new|while|of|in)$/,
  python: /^(?:import|def|return|if|not|in|while|raise|for|None|True|False)$/,
  curl: /^(?:while|do|done|case|esac|in|break|if|then|fi|curl|jq|xargs|sleep|wc|tr|convt)$/,
  cli: /^(?:convt)$/,
  json: /^(?:true|false|null)$/,
};

const patterns: Record<string, RegExp> = {
  node: /(\/\/[^\n]*)|("(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*'|`(?:\\.|[^`\\])*`)|(\b\d[\d_.]*\b)|([A-Za-z_$][\w$]*)/g,
  python: /(#[^\n]*)|(f?"(?:\\.|[^"\\])*"|f?'(?:\\.|[^'\\])*')|(\b\d[\d_.]*\b)|([A-Za-z_][\w]*)/g,
  curl: /((?:^|(?<=\s))#[^\n]*)|("(?:\\.|[^"\\])*"|'[^']*')|(\s--?[a-zA-Z][\w-]*)|(\$\(?[A-Za-z_][\w]*|\$\{[^}]+\})|([A-Za-z_][\w]*)/g,
  json: /("(?:\\.|[^"\\])*")(\s*:)?|(-?\b\d[\d.]*\b)|([a-z]+)/g,
};

export function tokenize(code: string, language: Language | "json"): Token[] {
  const lang = language === "cli" ? "curl" : language;
  const pattern = new RegExp(patterns[lang].source, "g");
  const tokens: Token[] = [];
  let last = 0;
  for (const m of code.matchAll(pattern)) {
    const index = m.index ?? 0;
    if (index > last) tokens.push({ text: code.slice(last, index) });
    last = index + m[0].length;
    if (lang === "json") {
      if (m[1]) {
        tokens.push({ text: m[1], tone: m[2] ? "property" : "string" });
        if (m[2]) tokens.push({ text: m[2] });
      } else if (m[3]) tokens.push({ text: m[3], tone: "number" });
      else tokens.push({ text: m[0], tone: keywords.json.test(m[0]) ? "keyword" : undefined });
      continue;
    }
    if (lang === "curl") {
      const [, comment, string, flag, variable] = m;
      const tone = comment
        ? "comment"
        : string
          ? "string"
          : flag
            ? "flag"
            : variable
              ? "variable"
              : keywords.curl.test(m[0])
                ? "keyword"
                : undefined;
      tokens.push({ text: m[0], tone });
      continue;
    }
    const [, comment, string, number] = m;
    const tone = comment
      ? "comment"
      : string
        ? "string"
        : number
          ? "number"
          : keywords[lang].test(m[0])
            ? "keyword"
            : undefined;
    tokens.push({ text: m[0], tone });
  }
  if (last < code.length) tokens.push({ text: code.slice(last) });
  return tokens;
}

export function Highlighted({ code, language }: { code: string; language: Language | "json" }) {
  return (
    <>
      {tokenize(code, language).map((t, i) =>
        t.tone ? (
          <span key={i} className={tones[t.tone]}>
            {t.text}
          </span>
        ) : (
          t.text
        ),
      )}
    </>
  );
}

const LanguageContext = createContext<{
  language: Language;
  setLanguage: (l: Language) => void;
} | null>(null);
const storageKey = "convt:code-language";

/** Shares the chosen language across every CodePanel inside it and remembers it. */
export function LanguageProvider({ children }: { children: ReactNode }) {
  const [language, setState] = useState<Language>("curl");
  useEffect(() => {
    const saved = localStorage.getItem(storageKey);
    if (saved === "curl" || saved === "node" || saved === "python" || saved === "cli")
      setState(saved);
  }, []);
  const setLanguage = (l: Language) => {
    setState(l);
    try {
      localStorage.setItem(storageKey, l);
    } catch {}
  };
  return <LanguageContext value={{ language, setLanguage }}>{children}</LanguageContext>;
}

function CopyIcon() {
  return (
    <svg width="13" height="13" viewBox="0 0 13 13" aria-hidden="true">
      <path
        d="M4.5 4.5 H10.5 V10.5 H4.5 Z M2.5 8.5 V2.5 H8.5"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.2"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

export function CopyCode({ value, className }: { value: string; className?: string }) {
  const [copied, setCopied] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout>>(undefined);
  useEffect(() => () => clearTimeout(timer.current), []);
  return (
    <button
      type="button"
      onClick={async () => {
        try {
          await navigator.clipboard.writeText(value);
          setCopied(true);
          clearTimeout(timer.current);
          timer.current = setTimeout(() => setCopied(false), 1500);
        } catch {
          setCopied(false);
        }
      }}
      className={cx(
        "flex shrink-0 cursor-pointer items-center gap-1.5 rounded-md px-2 py-1 text-xs/4 font-medium text-[#a1a6a3] outline-none hover:bg-white/6 hover:text-[#edefee] focus-visible:ring-2 focus-visible:ring-[#3fcb84]",
        className,
      )}
    >
      <CopyIcon />
      <span aria-live="polite">{copied ? "Copied" : "Copy"}</span>
    </button>
  );
}

/**
 * Tabbed code block. Inside a LanguageProvider the tab follows the shared choice when the
 * panel has that language; on its own it keeps local state.
 */
export function CodePanel({
  samples,
  title,
  className,
  maxHeight,
}: {
  samples: Sample[];
  title?: string;
  className?: string;
  /** Tailwind max-height class for the code area; it scrolls past that. */
  maxHeight?: string;
}) {
  const base = useId();
  const shared = useContext(LanguageContext);
  const [local, setLocal] = useState<Language>(samples[0].language);
  const wanted = shared?.language ?? local;
  const current = samples.find((s) => s.language === wanted) ?? samples[0];
  const tabs = useRef<Array<HTMLButtonElement | null>>([]);
  const choose = (l: Language) => (shared ? shared.setLanguage(l) : setLocal(l));

  function onKeyDown(event: React.KeyboardEvent, index: number) {
    const n = samples.length;
    const next =
      event.key === "ArrowRight"
        ? (index + 1) % n
        : event.key === "ArrowLeft"
          ? (index - 1 + n) % n
          : event.key === "Home"
            ? 0
            : event.key === "End"
              ? n - 1
              : -1;
    if (next < 0) return;
    event.preventDefault();
    choose(samples[next].language);
    tabs.current[next]?.focus();
  }

  return (
    <div
      className={cx(
        "flex min-w-0 flex-col overflow-clip rounded-xl bg-code shadow-[0_0_0_1px_var(--code-ring),0_1px_2px_rgb(0_0_0/0.08)]",
        className,
      )}
    >
      <div className="flex h-11 shrink-0 items-center justify-between gap-2 border-b border-white/8 pr-2 pl-2">
        <div className="flex min-w-0 items-center gap-1">
          {title && (
            <span className="truncate px-2 font-mono text-[11px]/4 text-[#a1a6a3]">{title}</span>
          )}
          {samples.length > 1 && (
            <div role="tablist" aria-label="Language" className="flex items-center gap-0.5">
              {samples.map((sample, index) => {
                const selected = sample.language === current.language;
                return (
                  <button
                    key={sample.language}
                    ref={(el) => {
                      tabs.current[index] = el;
                    }}
                    type="button"
                    role="tab"
                    id={`${base}-tab-${sample.language}`}
                    aria-selected={selected}
                    aria-controls={`${base}-panel`}
                    tabIndex={selected ? 0 : -1}
                    onClick={() => choose(sample.language)}
                    onKeyDown={(e) => onKeyDown(e, index)}
                    className={cx(
                      "cursor-pointer rounded-md px-2.5 py-1.5 text-xs/4 font-medium whitespace-nowrap outline-none focus-visible:ring-2 focus-visible:ring-[#3fcb84]",
                      selected
                        ? "bg-white/10 text-[#edefee]"
                        : "text-[#a1a6a3] hover:text-[#edefee]",
                    )}
                  >
                    {sample.label}
                  </button>
                );
              })}
            </div>
          )}
        </div>
        <CopyCode value={current.code} />
      </div>
      <div
        role={samples.length > 1 ? "tabpanel" : undefined}
        id={`${base}-panel`}
        aria-labelledby={samples.length > 1 ? `${base}-tab-${current.language}` : undefined}
        tabIndex={0}
        className={cx(
          "overflow-auto px-4 py-4 outline-none focus-visible:ring-2 focus-visible:ring-[#3fcb84] focus-visible:ring-inset",
          maxHeight,
        )}
      >
        <pre className="m-0 w-max font-mono text-[12.5px]/[20px] text-[#e6e8e7]">
          <code>
            <Highlighted code={current.code} language={current.language} />
          </code>
        </pre>
      </div>
    </div>
  );
}

/** Untabbed JSON block, for example responses. */
export function JsonPanel({ value, title }: { value: unknown; title: string }) {
  const code = JSON.stringify(value, null, 2);
  return (
    <div className="flex min-w-0 flex-col overflow-clip rounded-xl bg-code shadow-[0_0_0_1px_var(--code-ring)]">
      <div className="flex h-10 shrink-0 items-center justify-between border-b border-white/8 pr-2 pl-4">
        <span className="font-mono text-[11px]/4 text-[#a1a6a3]">{title}</span>
        <CopyCode value={code} />
      </div>
      <div
        tabIndex={0}
        className="max-h-80 overflow-auto px-4 py-3.5 outline-none focus-visible:ring-2 focus-visible:ring-[#3fcb84] focus-visible:ring-inset"
      >
        <pre className="m-0 w-max font-mono text-[12.5px]/[20px] text-[#e6e8e7]">
          <code>
            <Highlighted code={code} language="json" />
          </code>
        </pre>
      </div>
    </div>
  );
}
