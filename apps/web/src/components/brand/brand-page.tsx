import { useEffect, useRef, useState, type CSSProperties, type ReactNode } from "react";

import type { Account } from "#/lib/types";

import { CallToAction, Footer } from "../landing/closing";
import { Nav } from "../landing/nav";
import { Container, cx, focusRing } from "../landing/ui";
import manifest from "./manifest.json";

// The SVG text ships with the page so Copy SVG writes to the clipboard inside the click;
// a fetch first would lose the user gesture in Safari. scripts/brand-assets.py writes the files.
const svgs: Record<string, string> = Object.fromEntries(
  Object.entries(
    import.meta.glob<string>("../../../public/brand/*.svg", {
      query: "?raw",
      import: "default",
      eager: true,
    }),
  ).map(([path, text]) => [path.split("/").pop()!.replace(".svg", ""), text]),
);

type Stem = keyof typeof manifest.assets;
type Background = "dark" | "light";
type Asset = {
  title: string;
  note: string;
  /** File stem for each background, from public/brand. */
  stems: Record<Background, Stem>;
  /** Preview size in the tile. */
  previewClass: string;
  wide?: boolean;
};

const assets: Asset[] = [
  {
    title: "Lockup",
    note: "The mark and the name together. Use it wherever there is room.",
    stems: { dark: "convt-lockup-on-dark", light: "convt-lockup-on-light" },
    previewClass: "h-14 sm:h-[76px]",
    wide: true,
  },
  {
    title: "Mark",
    note: "Avatars, favicons and anywhere the name is already nearby.",
    stems: { dark: "convt-mark-on-dark", light: "convt-mark-on-light" },
    previewClass: "h-[88px] sm:h-[104px]",
  },
  {
    title: "Wordmark",
    note: "The name alone, in Geist SemiBold, outlined.",
    stems: { dark: "convt-wordmark-on-dark", light: "convt-wordmark-on-light" },
    previewClass: "h-10 sm:h-12",
  },
  {
    title: "One-color mark",
    note: "For print, stamps and busy photos. The overlap is cut out.",
    stems: { dark: "convt-mark-white", light: "convt-mark-black" },
    previewClass: "h-[88px] sm:h-[104px]",
  },
  {
    title: "App icon",
    note: "The mark on its tile, as it ships on macOS and Linux.",
    stems: { dark: "convt-app-icon", light: "convt-app-icon" },
    previewClass: "h-[112px] sm:h-[128px]",
  },
];

type Palette = {
  background: Background;
  title: string;
  page: string;
  mark: Stem;
  colors: { name: string; hex: string; role: string }[];
};

const palettes: Palette[] = [
  {
    background: "dark",
    title: "On dark",
    page: "#0A0B0B",
    mark: "convt-mark-on-dark",
    colors: [
      { name: "Ink", hex: "#EDEFEE", role: "Source square and wordmark" },
      { name: "Green", hex: "#46D08B", role: "Converted square, top" },
      { name: "Deep green", hex: "#1FA463", role: "Converted square, bottom" },
      { name: "Overlap", hex: "#A6F0C8", role: "Where the two meet" },
    ],
  },
  {
    background: "light",
    title: "On light",
    page: "#FFFFFF",
    mark: "convt-mark-on-light",
    colors: [
      { name: "Ink", hex: "#0A0A0A", role: "Source square and wordmark" },
      { name: "Green", hex: "#1FB36C", role: "Converted square, top" },
      { name: "Deep green", hex: "#127A47", role: "Converted square, bottom" },
      { name: "Overlap", hex: "#0B5C34", role: "Where the two meet" },
    ],
  },
];

const kb = (bytes: number) => `${Math.round(bytes / 1024)} KB`;
const vars = (values: Record<string, string>) => values as CSSProperties;

export function BrandPage({ account }: { account: Account | null }) {
  useReveal();
  return (
    <div className="min-h-screen overflow-x-clip bg-page text-ink">
      <a
        href="#main"
        className={`sr-only rounded-md bg-raised px-3 py-2 text-sm focus:not-sr-only focus:fixed focus:top-3 focus:left-3 focus:z-50 ${focusRing}`}
      >
        Skip to content
      </a>
      <Nav account={account} path="/brand" />
      <main id="main">
        <Intro />
        <Logos />
        <Colors />
        <Typography />
        <Usage />
        <CallToAction signedIn={false} />
      </main>
      <Footer />
    </div>
  );
}

/**
 * Fades `[data-reveal]` elements in as they scroll into view. Without JavaScript, or with
 * reduced motion, everything is simply visible: only elements still below the fold when
 * the page hydrates get hidden, so nothing on screen blinks out.
 */
function useReveal() {
  useEffect(() => {
    const items = [...document.querySelectorAll<HTMLElement>("[data-reveal]")];
    if (matchMedia("(prefers-reduced-motion: reduce)").matches) {
      for (const item of items) item.dataset.reveal = "shown";
      return;
    }
    const observer = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (!entry.isIntersecting) continue;
          (entry.target as HTMLElement).dataset.reveal = "shown";
          observer.unobserve(entry.target);
        }
      },
      { rootMargin: "0px 0px -10% 0px" },
    );
    for (const item of items) {
      if (item.getBoundingClientRect().top < innerHeight) {
        item.dataset.reveal = "shown";
      } else {
        item.dataset.reveal = "hidden";
        observer.observe(item);
      }
    }
    return () => observer.disconnect();
  }, []);
}

/** Stagger for revealed siblings. */
const stagger = (index: number) => vars({ "--reveal-delay": `${index * 70}ms` });

function Intro() {
  return (
    <Container>
      <div className="flex flex-col items-center gap-[22px] pt-14 pb-12 text-center md:pt-[88px] md:pb-16">
        <p className="brand-rise font-mono text-[12px]/[16px] text-land-muted uppercase">Brand</p>
        <h1
          className="brand-rise max-w-[900px] text-[40px]/[44px] font-medium tracking-[-0.04em] text-balance text-ink sm:text-[56px]/[60px] lg:text-[68px]/[72px]"
          style={vars({ "--d": "60ms" })}
        >
          The convt brand kit.
        </h1>
        <p
          className="brand-rise max-w-[560px] text-[17px]/[26px] text-pretty text-ink-2 sm:text-[18px]/[28px]"
          style={vars({ "--d": "120ms" })}
        >
          Logos, colors and type for anyone writing about convt. Copy any logo as SVG or PNG
          straight into your design tool, or take the whole kit.
        </p>
        <div
          className="brand-rise flex flex-wrap justify-center gap-2.5 pt-2.5"
          style={vars({ "--d": "180ms" })}
        >
          <a
            href="/brand/convt-brand.zip"
            download
            className={cx(
              "group bg-land-green inline-flex items-center gap-2 rounded-[10px] px-[18px] py-[11px] text-[15px]/[18px] font-medium whitespace-nowrap text-white shadow-land-primary transition-[filter,scale] duration-150 ease-out hover:brightness-110 motion-safe:active:scale-[0.97]",
              focusRing,
            )}
          >
            <DownloadGlyph className="transition-transform duration-200 ease-out motion-safe:group-hover:translate-y-px" />
            Download the kit
            <span className="font-mono text-[12px]/[16px] text-white/75">
              ZIP · {kb(manifest.zipBytes)}
            </span>
          </a>
          <CopyButton
            copy={() => writeText(svgs["convt-mark-on-dark"])}
            className="rounded-[10px] px-[18px] py-[11px] text-[15px]/[18px]"
          >
            Copy mark as SVG
          </CopyButton>
        </div>
      </div>
      <Construction />
    </Container>
  );
}

/** The mark assembling itself on its 32-unit grid, with the measurements it is drawn from. */
function Construction() {
  const [run, setRun] = useState(0);
  const u = 10;
  const size = 32 * u;
  const lines = Array.from({ length: 33 }, (_, i) => i * u);
  const note = (index: number) => vars({ "--d": `${1050 + index * 90}ms` });
  return (
    <figure
      className="brand-rise bg-land-glow relative flex items-center justify-center overflow-clip rounded-2xl bg-bottom px-5 py-14 lg:h-[560px] lg:py-0"
      style={vars({ "--d": "240ms" })}
    >
      <figcaption className="sr-only">
        The convt mark on a 32 by 32 grid: two 19-unit squares with 5-unit corners, the second
        offset 9 units down and to the right, overlapping in a 10-unit square.
      </figcaption>
      <div className="relative w-full max-w-[440px]">
        <svg
          key={run}
          viewBox={`-64 -52 ${size + 128} ${size + 104}`}
          className="block w-full"
          aria-hidden="true"
        >
          <defs>
            <linearGradient id="construction-green" x1="0" y1="0" x2="0" y2="1">
              <stop offset="0" stopColor="#46d08b" />
              <stop offset="1" stopColor="#1fa463" />
            </linearGradient>
            <clipPath id="construction-source">
              <rect x={2 * u} y={2 * u} width={19 * u} height={19 * u} rx={5 * u} />
            </clipPath>
          </defs>
          <g className="brand-grid">
            {lines.map((p) => (
              <g key={p} stroke={p % (4 * u) === 0 ? "#ffffff2e" : "#ffffff12"} strokeWidth="1">
                <line x1={p} y1={-20} x2={p} y2={size + 20} />
                <line x1={-20} y1={p} x2={size + 20} y2={p} />
              </g>
            ))}
          </g>
          <rect
            className="brand-source"
            x={2 * u}
            y={2 * u}
            width={19 * u}
            height={19 * u}
            rx={5 * u}
            fill="#edefee"
          />
          <rect
            className="brand-result"
            x={11 * u}
            y={11 * u}
            width={19 * u}
            height={19 * u}
            rx={5 * u}
            fill="url(#construction-green)"
          />
          <rect
            className="brand-overlap"
            x={11 * u}
            y={11 * u}
            width={19 * u}
            height={19 * u}
            rx={5 * u}
            fill="#a6f0c8"
            clipPath="url(#construction-source)"
          />
          <g fill="none" stroke="#46d08b" strokeWidth="1.5">
            {/* The corner radius, then 19 above, 19 on the right and the 9-unit offset below. */}
            <circle
              className="brand-note"
              style={note(0)}
              cx={7 * u}
              cy={7 * u}
              r={5 * u}
              strokeDasharray="4 4"
            />
            <path
              className="brand-note"
              style={note(1)}
              d={`M${2 * u} -14v-12M${21 * u} -14v-12M${2 * u} -20H${21 * u}`}
            />
            <path
              className="brand-note"
              style={note(2)}
              d={`M${size + 14} ${11 * u}h12M${size + 14} ${30 * u}h12M${size + 20} ${11 * u}V${30 * u}`}
            />
            <path
              className="brand-note"
              style={note(3)}
              d={`M${2 * u} ${size + 14}v12M${11 * u} ${size + 14}v12M${2 * u} ${size + 20}H${11 * u}`}
            />
          </g>
          <g className="font-mono" fontSize="13" fill="#c4c9c6">
            <text
              className="brand-note"
              style={note(0)}
              x={7 * u}
              y={7 * u + 4}
              textAnchor="middle"
              fill="#0a0b0b"
            >
              r 5
            </text>
            <text className="brand-note" style={note(1)} x={11.5 * u} y={-32} textAnchor="middle">
              19
            </text>
            <text className="brand-note" style={note(2)} x={size + 36} y={20.5 * u + 4}>
              19
            </text>
            <text
              className="brand-note"
              style={note(3)}
              x={6.5 * u}
              y={size + 44}
              textAnchor="middle"
            >
              9
            </text>
            <text
              className="brand-note"
              style={note(4)}
              x={size}
              y={size + 44}
              textAnchor="end"
              fill="#848c88"
            >
              32 × 32 grid
            </text>
          </g>
        </svg>
      </div>
      <button
        type="button"
        onClick={() => setRun((n) => n + 1)}
        className={cx(
          "group absolute right-4 bottom-4 inline-flex h-8 items-center gap-1.5 rounded-lg bg-[#0a0b0bb3] px-2.5 text-[13px]/[16px] font-medium text-ink-2 shadow-land-secondary backdrop-blur-sm transition-[color,scale] duration-150 ease-out hover:text-ink motion-safe:active:scale-[0.97]",
          focusRing,
        )}
      >
        <ReplayGlyph className="transition-transform duration-300 ease-out motion-safe:group-hover:-rotate-90" />
        Replay
      </button>
    </figure>
  );
}

function SectionHeading({ id, title, body }: { id: string; title: string; body: string }) {
  return (
    <div
      data-reveal
      className="brand-reveal flex flex-col gap-5 lg:flex-row lg:items-end lg:justify-between"
    >
      <h2
        id={id}
        className="max-w-[560px] text-[34px]/[40px] font-medium tracking-[-0.035em] text-ink md:text-[44px]/[48px]"
      >
        {title}
      </h2>
      <p className="max-w-[420px] shrink-0 text-[17px]/[26px] text-pretty text-ink-2 lg:w-[420px]">
        {body}
      </p>
    </div>
  );
}

function Logos() {
  return (
    <section aria-labelledby="logos-title" className="pt-16 md:pt-[120px]">
      <Container className="flex flex-col gap-10 md:gap-14">
        <SectionHeading
          id="logos-title"
          title="Logos"
          body="Every logo comes as SVG and PNG, in versions for dark and light backgrounds. Flip a tile to see each one."
        />
        <ul className="grid gap-4 lg:grid-cols-3">
          {assets.map((asset, i) => (
            <AssetTile key={asset.title} asset={asset} index={i} />
          ))}
        </ul>
      </Container>
    </section>
  );
}

function AssetTile({ asset, index }: { asset: Asset; index: number }) {
  const [background, setBackground] = useState<Background>("dark");
  const stem = asset.stems[background];
  const [w, h] = manifest.assets[stem].png;
  return (
    <li
      data-reveal
      style={stagger(index % 3)}
      className={cx(
        "brand-reveal flex flex-col overflow-clip rounded-2xl bg-raised shadow-land-card",
        asset.wide && "lg:col-span-2",
      )}
    >
      <div
        className={cx(
          "relative grid h-[220px] place-items-center px-8 transition-colors duration-300 ease-out sm:h-[260px]",
          background === "dark" ? "bg-land-well" : "bg-white",
        )}
      >
        {/* Both versions stay mounted so switching crossfades instead of flashing. */}
        {(["dark", "light"] as const).map((option) => (
          <img
            key={option}
            src={`/brand/${asset.stems[option]}.svg`}
            alt={
              option === background
                ? `convt ${asset.title.toLowerCase()} for ${option} backgrounds`
                : ""
            }
            aria-hidden={option !== background}
            className={cx(
              "col-start-1 row-start-1 block w-auto max-w-full transition-[opacity,scale] duration-300 ease-out motion-reduce:transition-none",
              asset.previewClass,
              option === background ? "scale-100 opacity-100" : "scale-[0.97] opacity-0",
            )}
          />
        ))}
        <BackgroundSwitch
          label={asset.title}
          value={background}
          onChange={setBackground}
          className="absolute top-3 right-3"
        />
      </div>
      <div
        className={cx(
          "flex flex-1 flex-col justify-between gap-4 border-t border-line p-5",
          // Narrow tiles stack the buttons under the text; wide ones have room beside it.
          asset.wide ? "md:flex-row md:items-end" : "md:max-lg:flex-row md:max-lg:items-end",
        )}
      >
        <div className="flex flex-col gap-1">
          <h3 className="text-[15px]/[20px] font-medium text-ink">{asset.title}</h3>
          <p className="text-[14px]/[20px] text-pretty text-ink-2">{asset.note}</p>
          <p className="pt-1 font-mono text-[11px]/[16px] text-land-muted">
            SVG · PNG {w}×{h}
          </p>
        </div>
        <div className="flex shrink-0 flex-wrap gap-2">
          <ButtonGroup icon={<CopyGlyph />} label="Copy">
            <CopyButton
              grouped
              copy={() => writeText(svgs[stem])}
              aria-label={`Copy ${asset.title} as SVG`}
            >
              SVG
            </CopyButton>
            <CopyButton
              grouped
              copy={() => writePng(`/brand/${stem}.png`)}
              aria-label={`Copy ${asset.title} as PNG`}
            >
              PNG
            </CopyButton>
          </ButtonGroup>
          <ButtonGroup icon={<DownloadGlyph />} label="Save">
            <FileLink href={`/brand/${stem}.svg`} label={`Download ${asset.title} as SVG`}>
              SVG
            </FileLink>
            <FileLink href={`/brand/${stem}.png`} label={`Download ${asset.title} as PNG`}>
              PNG
            </FileLink>
          </ButtonGroup>
        </div>
      </div>
    </li>
  );
}

function BackgroundSwitch({
  label,
  value,
  onChange,
  className,
}: {
  label: string;
  value: Background;
  onChange: (value: Background) => void;
  className?: string;
}) {
  const onDark = value === "dark";
  return (
    <div
      role="radiogroup"
      aria-label={`${label} background`}
      className={cx(
        "grid grid-cols-2 rounded-lg p-0.5 transition-[background-color,box-shadow] duration-300 ease-out",
        onDark
          ? "bg-[#161918] shadow-[0_0_0_1px_#2e3331]"
          : "bg-[#f3f4f3] shadow-[0_0_0_1px_#e0e3e1]",
        className,
      )}
    >
      {/* The thumb slides between the two options. */}
      <span
        aria-hidden="true"
        className={cx(
          "brand-thumb absolute inset-y-0.5 left-0.5 w-[calc(50%-2px)] rounded-md",
          onDark
            ? "translate-x-0 bg-[#2a2f2d]"
            : "translate-x-full bg-white shadow-[0_0_0_1px_#e0e3e1,0_1px_2px_#0000000f]",
        )}
      />
      {(["dark", "light"] as const).map((option) => {
        const selected = value === option;
        return (
          <button
            key={option}
            type="button"
            role="radio"
            aria-checked={selected}
            // One tab stop for the group; the arrow keys move between the options.
            tabIndex={selected ? 0 : -1}
            onClick={() => onChange(option)}
            onKeyDown={(event) => {
              if (!["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown"].includes(event.key)) return;
              event.preventDefault();
              const next = option === "dark" ? "light" : "dark";
              onChange(next);
              event.currentTarget.parentElement
                ?.querySelector<HTMLElement>(`[data-option="${next}"]`)
                ?.focus();
            }}
            data-option={option}
            className={cx(
              "relative h-7 w-12 rounded-md text-[12px]/[16px] font-medium capitalize transition-colors duration-200 ease-out",
              focusRing,
              !onDark && focusOnLight,
              onDark
                ? selected
                  ? "text-white"
                  : "text-[#a1a6a3] hover:text-white"
                : selected
                  ? "text-[#0a0a0a]"
                  : "text-[#5c615e] hover:text-[#0a0a0a]",
            )}
          >
            {option}
          </button>
        );
      })}
    </div>
  );
}

/** A labeled pill of joined buttons: "[icon Copy | SVG | PNG]". */
function ButtonGroup({
  icon,
  label,
  children,
}: {
  icon: ReactNode;
  label: string;
  children: ReactNode;
}) {
  return (
    <div
      role="group"
      aria-label={label}
      className="inline-flex h-8 items-stretch rounded-lg bg-sunken shadow-land-secondary"
    >
      <span
        aria-hidden="true"
        className="inline-flex items-center gap-1.5 pr-1.5 pl-2.5 text-[12px]/[16px] text-land-muted"
      >
        {icon}
        {label}
      </span>
      {children}
    </div>
  );
}

/** The focus outline on white surfaces, where the light green is too faint. */
const focusOnLight = "focus-visible:outline-[#127a47]!";

const pressable =
  "transition-[background-color,color,scale] duration-150 ease-out motion-safe:active:scale-[0.96]";
const groupedButton = cx(
  "relative inline-grid min-w-11 place-items-center px-2.5 text-[13px]/[16px] font-medium text-ink hover:bg-hover last:rounded-r-lg",
  "before:absolute before:inset-y-2 before:left-0 before:w-px before:bg-line-strong",
  pressable,
);

function FileLink({ href, label, children }: { href: string; label: string; children: ReactNode }) {
  return (
    <a href={href} download aria-label={label} className={cx(groupedButton, focusRing)}>
      {children}
    </a>
  );
}

function writeText(text: string) {
  return (navigator.clipboard?.writeText(text) ?? Promise.reject()).catch(() => {
    // The Clipboard API needs HTTPS; the old selection copy works without it.
    if (!copyBySelection(text)) throw new Error("copy failed");
  });
}

/** Copies an image. The fetch goes inside ClipboardItem so Safari keeps the click's permission. */
function writePng(url: string) {
  if (typeof ClipboardItem === "undefined" || !navigator.clipboard?.write) {
    return Promise.reject(new Error("image copy is not available here"));
  }
  const blob = fetch(url).then((response) => {
    if (!response.ok) throw new Error(`${url}: ${response.status}`);
    return response.blob();
  });
  return navigator.clipboard.write([new ClipboardItem({ "image/png": blob })]);
}

function copyBySelection(text: string) {
  const focused = document.activeElement as HTMLElement | null;
  const area = document.createElement("textarea");
  area.value = text;
  area.setAttribute("readonly", "");
  area.style.cssText = "position:fixed;top:0;left:0;opacity:0";
  document.body.append(area);
  area.select();
  const ok = document.execCommand("copy");
  area.remove();
  focused?.focus();
  return ok;
}

type CopyState = "idle" | "copied" | "failed";

/** Runs a copy and shows its result in place for a moment. */
function useCopy() {
  const [state, setState] = useState<CopyState>("idle");
  const timer = useRef<ReturnType<typeof setTimeout>>(undefined);
  useEffect(() => () => clearTimeout(timer.current), []);
  const show = (next: CopyState) => {
    setState(next);
    clearTimeout(timer.current);
    timer.current = setTimeout(() => setState("idle"), next === "failed" ? 2400 : 1600);
  };
  // Call inside the click: the clipboard only accepts writes started by a user gesture.
  const run = (copy: () => Promise<unknown>) => {
    copy().then(
      () => show("copied"),
      () => show("failed"),
    );
  };
  return { state, run };
}

const announcements: Record<CopyState, string> = {
  idle: "",
  copied: "Copied",
  failed: "This browser blocked the copy. Use Save instead.",
};

// Colors have no download to fall back on, so their message stops at the failure.
const colorAnnouncements: Record<CopyState, string> = {
  idle: "",
  copied: "Copied",
  failed: "This browser blocked the copy.",
};

/** Icons or labels stacked in one grid cell; only the one for the current state shows. */
function Swap({
  show,
  className,
  children,
}: {
  show: boolean;
  className?: string;
  children: ReactNode;
}) {
  return (
    <span
      className={cx("brand-swap col-start-1 row-start-1", !show && "brand-swap-out", className)}
    >
      {children}
    </span>
  );
}

function CopyButton({
  copy,
  grouped,
  className,
  children,
  ...props
}: {
  copy: () => Promise<unknown>;
  grouped?: boolean;
  className?: string;
  children: ReactNode;
  "aria-label"?: string;
}) {
  const { state, run } = useCopy();
  return (
    <button
      type="button"
      onClick={() => run(copy)}
      title={state === "failed" ? announcements.failed : undefined}
      className={cx(
        grouped
          ? groupedButton
          : cx(
              "inline-flex shrink-0 items-center justify-center gap-2 font-medium whitespace-nowrap bg-sunken text-ink shadow-land-secondary hover:bg-hover",
              pressable,
            ),
        focusRing,
        className,
      )}
      {...props}
    >
      {grouped ? (
        // Every state shares one cell, so the button keeps its width.
        <span className="grid place-items-center" aria-hidden="true">
          <Swap show={state === "idle"}>{children}</Swap>
          <Swap show={state === "copied"} className="text-green">
            <CheckGlyph />
          </Swap>
          <Swap show={state === "failed"}>
            <CrossGlyph />
          </Swap>
        </span>
      ) : (
        <>
          <span className="grid size-3.5 place-items-center" aria-hidden="true">
            <Swap show={state === "idle"}>
              <CopyGlyph />
            </Swap>
            <Swap show={state === "copied"} className="text-green">
              <CheckGlyph />
            </Swap>
            <Swap show={state === "failed"}>
              <CrossGlyph />
            </Swap>
          </span>
          {children}
        </>
      )}
      <span className="sr-only" aria-live="polite">
        {announcements[state]}
      </span>
    </button>
  );
}

function Colors() {
  return (
    <section aria-labelledby="colors-title" className="pt-16 md:pt-[120px]">
      <Container className="flex flex-col gap-10 md:gap-14">
        <SectionHeading
          id="colors-title"
          title="Colors"
          body="Ink for the file you have, green for the file you get, and a third shade where they overlap. Each background has its own set. Click a color to copy it."
        />
        <div className="grid gap-4 lg:grid-cols-2">
          {palettes.map((palette, i) => (
            <PalettePanel key={palette.background} palette={palette} index={i} />
          ))}
        </div>
      </Container>
    </section>
  );
}

function PalettePanel({ palette, index }: { palette: Palette; index: number }) {
  const dark = palette.background === "dark";
  const [, top, bottom] = palette.colors;
  const gradient = `linear-gradient(180deg, ${top.hex} 0%, ${bottom.hex} 100%)`;
  const muted = dark ? "text-land-muted" : "text-[#5c615e]";
  return (
    <div
      data-reveal
      style={stagger(index)}
      className={cx(
        "brand-reveal flex flex-col overflow-clip rounded-2xl",
        dark ? "bg-land-well text-ink shadow-land-card" : "bg-white text-[#0a0a0a]",
      )}
    >
      <div className="flex items-baseline justify-between gap-4 px-6 pt-6 sm:px-8 sm:pt-7">
        <h3 className="text-[15px]/[20px] font-medium">{palette.title}</h3>
        <CopyText
          text={palette.page}
          label={`Page background, ${palette.page}. Copy hex`}
          className={cx("font-mono text-[12px]/[16px]", muted)}
          dark={dark}
        >
          Page {palette.page}
        </CopyText>
      </div>
      <div className="flex flex-col gap-6 px-4 pt-6 pb-4 sm:flex-row sm:items-center sm:gap-8 sm:px-6 sm:pb-6">
        <img
          src={`/brand/${palette.mark}.svg`}
          alt=""
          className="size-24 shrink-0 self-center sm:ml-2 sm:size-32"
        />
        <ul className="flex min-w-0 flex-1 flex-col">
          {palette.colors.map((color) => (
            <li key={color.name}>
              <ColorRow {...color} dark={dark} />
            </li>
          ))}
        </ul>
      </div>
      <CopyRow
        text={gradient}
        label={`Mark gradient, ${gradient}. Copy CSS`}
        dark={dark}
        className={cx(
          "border-t px-6 py-4 sm:px-8",
          dark ? "border-line hover:bg-[#111312]" : "border-[#eef0ef] hover:bg-[#f7f8f7]",
        )}
        swatch={
          <span
            className="h-8 w-14 shrink-0 rounded-lg shadow-[inset_0_1px_0_#ffffff38]"
            style={{ backgroundImage: gradient }}
          />
        }
        copiedLabel="Copied the gradient CSS"
      >
        <span className={cx("truncate font-mono text-[12px]/[16px]", muted)}>{gradient}</span>
      </CopyRow>
    </div>
  );
}

function ColorRow({
  name,
  hex,
  role,
  dark,
}: {
  name: string;
  hex: string;
  role: string;
  dark: boolean;
}) {
  return (
    <CopyRow
      text={hex}
      label={`${name}, ${hex}, ${role}. Copy hex`}
      dark={dark}
      className={cx("rounded-xl px-2.5 py-2", dark ? "hover:bg-[#161918]" : "hover:bg-[#f3f4f3]")}
      swatch={
        <span
          className={cx(
            "size-9 shrink-0 rounded-lg transition-transform duration-200 ease-out motion-safe:group-hover:scale-105",
            dark ? "shadow-[inset_0_0_0_1px_#ffffff1f]" : "shadow-[inset_0_0_0_1px_#0000001a]",
          )}
          style={{ backgroundColor: hex }}
        />
      }
      trailing={hex}
    >
      <span className="text-[14px]/[20px] font-medium">{name}</span>
      <span
        className={cx("truncate text-[13px]/[18px]", dark ? "text-land-muted" : "text-[#5c615e]")}
      >
        {role}
      </span>
    </CopyRow>
  );
}

/** A full-width row that copies `text`; the trailing value (or the body) turns into "Copied". */
function CopyRow({
  text,
  label,
  dark,
  className,
  swatch,
  trailing,
  copiedLabel = "Copied",
  children,
}: {
  text: string;
  label: string;
  dark: boolean;
  className?: string;
  swatch: ReactNode;
  trailing?: string;
  copiedLabel?: string;
  children: ReactNode;
}) {
  const { state, run } = useCopy();
  const copied = (
    <>
      <Swap
        show={state === "copied"}
        className={cx("inline-flex items-center gap-1", dark ? "text-green" : "text-[#127a47]")}
      >
        <CheckGlyph />
        {copiedLabel}
      </Swap>
      <CopyFailed show={state === "failed"} dark={dark} />
    </>
  );
  return (
    <button
      type="button"
      onClick={() => run(() => writeText(text))}
      aria-label={label}
      className={cx(
        "group flex w-full items-center gap-3.5 text-left transition-[background-color,scale] duration-150 ease-out motion-safe:active:scale-[0.99]",
        focusRing,
        !dark && focusOnLight,
        className,
      )}
    >
      {swatch}
      {trailing ? (
        <>
          <span className="flex min-w-0 flex-1 flex-col">{children}</span>
          <span
            aria-hidden="true"
            className={cx(
              "grid justify-items-end font-mono text-[12px]/[16px]",
              dark ? "text-ink-2" : "text-[#3d413f]",
            )}
          >
            <Swap show={state === "idle"}>{trailing}</Swap>
            {copied}
          </span>
        </>
      ) : (
        <span aria-hidden="true" className="grid min-w-0 flex-1 font-mono text-[12px]/[16px]">
          <Swap show={state === "idle"} className="flex min-w-0">
            {children}
          </Swap>
          {copied}
        </span>
      )}
      <span className="sr-only" aria-live="polite">
        {colorAnnouncements[state]}
      </span>
    </button>
  );
}

/** Shown in place of a value when the browser refuses the copy. */
function CopyFailed({
  show,
  dark,
  className,
}: {
  show: boolean;
  dark: boolean;
  className?: string;
}) {
  return (
    <Swap
      show={show}
      className={cx(
        "inline-flex items-center gap-1",
        dark ? "text-[#f2786d]" : "text-[#b3261e]",
        className,
      )}
    >
      Couldn't copy
    </Swap>
  );
}

/** Small inline text that copies itself. */
function CopyText({
  text,
  label,
  dark,
  className,
  children,
}: {
  text: string;
  label: string;
  dark: boolean;
  className?: string;
  children: ReactNode;
}) {
  const { state, run } = useCopy();
  return (
    <button
      type="button"
      onClick={() => run(() => writeText(text))}
      aria-label={label}
      className={cx(
        "grid rounded-sm transition-colors duration-150 ease-out",
        dark ? "hover:text-ink" : "hover:text-[#0a0a0a]",
        focusRing,
        !dark && focusOnLight,
        className,
      )}
    >
      <Swap show={state === "idle"} className="justify-self-end">
        {children}
      </Swap>
      <CopyFailed show={state === "failed"} dark={dark} className="justify-self-end" />
      <Swap
        show={state === "copied"}
        className={cx("justify-self-end", dark ? "text-green" : "text-[#127a47]")}
      >
        Copied
      </Swap>
      <span className="sr-only" aria-live="polite">
        {colorAnnouncements[state]}
      </span>
    </button>
  );
}

function Typography() {
  return (
    <section aria-labelledby="type-title" className="pt-16 md:pt-[120px]">
      <Container className="flex flex-col gap-10 md:gap-14">
        <SectionHeading
          id="type-title"
          title="Type"
          body="Inter for everything people read, Geist Mono for file names, sizes and commands. Both are free under the SIL Open Font License. The wordmark is Geist SemiBold, outlined."
        />
        <div className="grid gap-4 lg:grid-cols-[3fr_2fr]">
          <TypeCard
            name="Inter"
            usage="Headings in Medium with tight tracking, body in Regular."
            href="https://rsms.me/inter/"
          >
            <p className="text-[96px]/[1] font-semibold tracking-[-0.04em] text-ink sm:text-[128px]/[1]">
              Aa
            </p>
            <p className="text-[28px]/[32px] font-medium tracking-[-0.04em] text-balance text-ink sm:text-[34px]/[38px]">
              Convert any file with a right-click.
            </p>
          </TypeCard>
          <TypeCard
            name="Geist Mono"
            usage="File names, sizes, formats and anything typed in a terminal."
            href="https://vercel.com/font"
          >
            <p className="font-mono text-[96px]/[1] text-ink sm:text-[128px]/[1]">Aa</p>
            <div className="flex flex-col gap-1.5 font-mono text-[14px]/[20px]">
              <p className="text-land-mono">
                <span className="text-land-muted">$ </span>convt miso.heic --to webp
              </p>
              <p className="text-land-muted">
                miso.webp <span className="text-green">612 KB</span>
              </p>
            </div>
          </TypeCard>
        </div>
      </Container>
    </section>
  );
}

function TypeCard({
  name,
  usage,
  href,
  children,
}: {
  name: string;
  usage: string;
  href: string;
  children: React.ReactNode;
}) {
  return (
    <div className="flex flex-col justify-between gap-12 rounded-2xl bg-raised p-6 shadow-land-card sm:p-8">
      <div className="flex flex-col gap-6">{children}</div>
      <div className="flex flex-col gap-4 border-t border-line pt-5 sm:flex-row sm:items-end sm:justify-between">
        <div className="flex flex-col gap-1">
          <h3 className="text-[15px]/[20px] font-medium text-ink">{name}</h3>
          <p className="max-w-[360px] text-[14px]/[20px] text-pretty text-ink-2">{usage}</p>
        </div>
        <a
          href={href}
          className={cx(
            "shrink-0 rounded-sm text-[14px]/[18px] text-green underline-offset-4 hover:underline",
            focusRing,
          )}
        >
          Get {name} ↗
        </a>
      </div>
    </div>
  );
}

// Each example starts as the real mark and bends into the mistake as it scrolls into view.
const donts: { label: string; transform?: string; filter?: string }[] = [
  { label: "Don't recolor it", filter: "hue-rotate(150deg) saturate(1.4)" },
  { label: "Don't stretch it", transform: "scaleX(1.45)" },
  { label: "Don't flip it", transform: "scaleX(-1)" },
  { label: "Don't add effects", filter: "drop-shadow(0 0 16px #46d08b) blur(0.6px)" },
];

function Usage() {
  return (
    <section aria-labelledby="usage-title" className="py-16 md:py-[120px]">
      <Container className="flex flex-col gap-10 md:gap-14">
        <SectionHeading
          id="usage-title"
          title="Using the name and logo"
          body="A few rules keep convt recognizable. You don't need to ask before writing about convt or using these files to link to it."
        />
        <div className="grid gap-4 lg:grid-cols-3">
          <Rule index={0} title="Lowercase, always">
            Write <span className="font-medium text-ink">convt</span>, even at the start of a
            sentence. Not Convt, ConvT or CONVT.
          </Rule>
          <Rule index={1} title="Give it room">
            Keep clear space at least as wide as the overlap square around the logo, and set the
            mark at 16 px or larger.
          </Rule>
          <Rule index={2} title="Use the files as they are">
            Pick the version made for your background instead of editing one. On a busy photo, use
            the one-color mark.
          </Rule>
        </div>
        <ul className="grid grid-cols-2 gap-4 md:grid-cols-4">
          {donts.map((dont, i) => (
            <li
              key={dont.label}
              data-reveal
              style={stagger(i)}
              className="brand-reveal flex flex-col gap-3"
            >
              <div className="flex aspect-[4/3] items-center justify-center overflow-clip rounded-2xl bg-land-well shadow-land-card">
                <img
                  src="/brand/convt-mark-on-dark.svg"
                  alt=""
                  className="brand-dont size-16 sm:size-20"
                  style={vars({
                    "--dont-transform": dont.transform ?? "none",
                    "--dont-filter": dont.filter ?? "none",
                  })}
                />
              </div>
              <p className="flex items-center gap-2 text-[14px]/[18px] text-ink-2">
                <CrossGlyph />
                {dont.label}
              </p>
            </li>
          ))}
        </ul>
      </Container>
    </section>
  );
}

function Rule({ index, title, children }: { index: number; title: string; children: ReactNode }) {
  return (
    <div
      data-reveal
      style={stagger(index)}
      className="brand-reveal flex flex-col gap-2 rounded-2xl bg-raised p-6 shadow-land-card"
    >
      <h3 className="text-[15px]/[20px] font-medium text-ink">{title}</h3>
      <p className="text-[14px]/[21px] text-pretty text-ink-2">{children}</p>
    </div>
  );
}

function DownloadGlyph({ className }: { className?: string }) {
  return (
    <svg
      width="14"
      height="14"
      viewBox="0 0 14 14"
      aria-hidden="true"
      className={cx("shrink-0", className)}
    >
      <path
        d="M7 1.5v8M3.5 6.5L7 10l3.5-3.5M2 12.5h10"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.6"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

function CopyGlyph() {
  return (
    <svg width="14" height="14" viewBox="0 0 14 14" aria-hidden="true" className="shrink-0">
      <rect
        x="4.5"
        y="4.5"
        width="8"
        height="8"
        rx="2"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.5"
      />
      <path
        d="M9.5 2.6A1.8 1.8 0 0 0 8 1.5H3.5a2 2 0 0 0-2 2V8a1.8 1.8 0 0 0 1.1 1.5"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.5"
        strokeLinecap="round"
      />
    </svg>
  );
}

function CheckGlyph() {
  return (
    <svg width="14" height="14" viewBox="0 0 14 14" aria-hidden="true" className="shrink-0">
      <path
        d="M2.5 7.5l3 3 6-7"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.8"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

function CrossGlyph() {
  return (
    <svg width="12" height="12" viewBox="0 0 12 12" aria-hidden="true" className="shrink-0">
      <path
        d="M2.5 2.5l7 7M9.5 2.5l-7 7"
        fill="none"
        stroke="#f2786d"
        strokeWidth="1.6"
        strokeLinecap="round"
      />
    </svg>
  );
}

function ReplayGlyph({ className }: { className?: string }) {
  return (
    <svg
      width="14"
      height="14"
      viewBox="0 0 14 14"
      aria-hidden="true"
      className={cx("shrink-0", className)}
    >
      <path
        d="M2.5 7a4.5 4.5 0 1 0 1.4-3.25M2.5 1.75v2.5H5"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.5"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}
