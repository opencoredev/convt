import { useEffect, useRef, useState } from "react";

import { CallToAction, Footer } from "../landing/closing";
import { Nav } from "../landing/nav";
import { Container, cx, focusRing } from "../landing/ui";
import lockupBlack from "../../../public/brand/convt-lockup-black.svg?raw";
import lockupWhite from "../../../public/brand/convt-lockup-white.svg?raw";
import markBlack from "../../../public/brand/convt-mark-black.svg?raw";
import mark from "../../../public/brand/convt-mark.svg?raw";
import markWhite from "../../../public/brand/convt-mark-white.svg?raw";
import wordmarkBlack from "../../../public/brand/convt-wordmark-black.svg?raw";
import wordmarkWhite from "../../../public/brand/convt-wordmark-white.svg?raw";
import manifest from "./manifest.json";

// The SVG text ships with the page so Copy SVG writes to the clipboard inside the click;
// a fetch first would lose the user gesture in Safari.
const svgs: Record<string, string> = {
  "convt-mark": mark,
  "convt-mark-black": markBlack,
  "convt-mark-white": markWhite,
  "convt-wordmark-black": wordmarkBlack,
  "convt-wordmark-white": wordmarkWhite,
  "convt-lockup-black": lockupBlack,
  "convt-lockup-white": lockupWhite,
};

type Background = "dark" | "light";
type Asset = {
  title: string;
  note: string;
  /** File stem for each background, from public/brand. */
  stems: Record<Background, string>;
  /** Preview size in the tile. */
  previewClass: string;
  wide?: boolean;
};

const assets: Asset[] = [
  {
    title: "Lockup",
    note: "The mark and the name together. Use it wherever there is room.",
    stems: { dark: "convt-lockup-white", light: "convt-lockup-black" },
    previewClass: "h-14 sm:h-[72px]",
    wide: true,
  },
  {
    title: "Mark",
    note: "App icon, avatars and favicons.",
    stems: { dark: "convt-mark", light: "convt-mark" },
    previewClass: "h-[88px] sm:h-[104px]",
  },
  {
    title: "Wordmark",
    note: "The name on its own, set in Geist SemiBold and outlined.",
    stems: { dark: "convt-wordmark-white", light: "convt-wordmark-black" },
    previewClass: "h-11 sm:h-14",
    wide: true,
  },
  {
    title: "One-color mark",
    note: "For print, embossing and anywhere green won't work.",
    stems: { dark: "convt-mark-white", light: "convt-mark-black" },
    previewClass: "h-[88px] sm:h-[104px]",
  },
];

const colors = [
  { name: "Green", hex: "#2FBF78", role: "Top of the mark" },
  { name: "Deep green", hex: "#1F9A5C", role: "Bottom of the mark" },
  { name: "Signal", hex: "#3FCB84", role: "Green text on dark" },
  { name: "Forest", hex: "#127A47", role: "Green text on light" },
  { name: "Ink", hex: "#0A0B0B", role: "Page and dark text" },
  { name: "Graphite", hex: "#111312", role: "Cards and panels" },
  { name: "Line", hex: "#232726", role: "Borders on dark" },
  { name: "Paper", hex: "#EDEFEE", role: "Text on dark" },
];

const kb = (bytes: number) => `${Math.round(bytes / 1024)} KB`;

export function BrandPage() {
  return (
    <div className="min-h-screen overflow-x-clip bg-page text-ink">
      <a
        href="#main"
        className={`sr-only rounded-md bg-raised px-3 py-2 text-sm focus:not-sr-only focus:fixed focus:top-3 focus:left-3 focus:z-50 ${focusRing}`}
      >
        Skip to content
      </a>
      <Nav />
      <main id="main">
        <Intro />
        <Logos />
        <Colors />
        <Typography />
        <Usage />
        <CallToAction />
      </main>
      <Footer />
    </div>
  );
}

function Intro() {
  return (
    <Container>
      <div className="flex flex-col items-center gap-[22px] pt-14 pb-12 text-center md:pt-[88px] md:pb-16">
        <p className="font-mono text-[12px]/[16px] text-land-muted uppercase">Brand</p>
        <h1 className="max-w-[900px] text-[40px]/[44px] font-medium tracking-[-0.04em] text-balance text-ink sm:text-[56px]/[60px] lg:text-[68px]/[72px]">
          The convt brand kit.
        </h1>
        <p className="max-w-[560px] text-[17px]/[26px] text-pretty text-ink-2 sm:text-[18px]/[28px]">
          Logos, colors and type for anyone writing about convt. Copy an SVG straight into your
          design tool, or take the whole kit.
        </p>
        <div className="flex flex-wrap justify-center gap-2.5 pt-2.5">
          <a
            href="/brand/convt-brand.zip"
            download
            className={cx(
              "bg-land-green inline-flex items-center gap-2 rounded-[10px] px-[18px] py-[11px] text-[15px]/[18px] font-medium whitespace-nowrap text-white shadow-land-primary transition-[filter] duration-150 hover:brightness-110",
              focusRing,
            )}
          >
            <DownloadGlyph />
            Download the kit
            <span className="font-mono text-[12px]/[16px] text-white/75">
              ZIP · {kb(manifest.zipBytes)}
            </span>
          </a>
          <CopyButton
            text={mark}
            label="Copy mark as SVG"
            className="rounded-[10px] px-[18px] py-[11px] text-[15px]/[18px]"
          />
        </div>
      </div>
      <Construction />
    </Container>
  );
}

/** The mark on its 32-unit grid, with the measurements it is drawn from. */
function Construction() {
  const unit = 10;
  const size = 32 * unit;
  const lines = Array.from({ length: 33 }, (_, i) => i * unit);
  return (
    <figure className="bg-land-glow relative flex items-center justify-center overflow-clip rounded-2xl bg-bottom px-5 py-14 lg:h-[560px] lg:py-0">
      <figcaption className="sr-only">
        The convt mark drawn on a 32 by 32 grid: an 8-unit corner radius and two arrows with a
        2.6-unit stroke.
      </figcaption>
      <div className="relative w-full max-w-[420px]">
        <svg
          viewBox={`-60 -40 ${size + 120} ${size + 80}`}
          className="block w-full"
          aria-hidden="true"
        >
          <defs>
            <linearGradient id="construction-g" x1="0" y1="0" x2="0" y2="1">
              <stop offset="0" stopColor="#2fbf78" />
              <stop offset="1" stopColor="#1f9a5c" />
            </linearGradient>
          </defs>
          <g transform={`scale(${unit})`}>
            <rect width="32" height="32" rx="8" fill="url(#construction-g)" />
            <path
              d="M8 12.5h14M18.5 9l3.5 3.5-3.5 3.5M24 19.5H10M13.5 16 10 19.5l3.5 3.5"
              fill="none"
              stroke="#fff"
              strokeWidth="2.6"
              strokeLinecap="round"
              strokeLinejoin="round"
            />
          </g>
          {lines.map((p) => (
            <g key={p} stroke={p % (4 * unit) === 0 ? "#ffffff2e" : "#ffffff12"} strokeWidth="1">
              <line x1={p} y1={-24} x2={p} y2={size + 24} />
              <line x1={-24} y1={p} x2={size + 24} y2={p} />
            </g>
          ))}
          {/* Arrow centerlines and the corner radius. */}
          <g fill="none" stroke="#0a0b0b" strokeWidth="1.5" strokeDasharray="4 4" opacity="0.7">
            <line x1={8 * unit} y1={12.5 * unit} x2={22 * unit} y2={12.5 * unit} />
            <line x1={10 * unit} y1={19.5 * unit} x2={24 * unit} y2={19.5 * unit} />
          </g>
          <circle
            cx={8 * unit}
            cy={8 * unit}
            r={8 * unit}
            fill="none"
            stroke="#3fcb84"
            strokeWidth="1.5"
            strokeDasharray="4 4"
          />
          <g fill="#3fcb84">
            <circle cx={8 * unit} cy={8 * unit} r="3" />
            <circle cx={22 * unit} cy={12.5 * unit} r="3" />
            <circle cx={10 * unit} cy={19.5 * unit} r="3" />
          </g>
          <g
            className="font-mono"
            fontSize="13"
            fill="#c4c9c6"
            style={{ fontVariantNumeric: "tabular-nums" }}
          >
            <text x={size / 2} y={-28} textAnchor="middle">
              32
            </text>
            <text x={-30} y={size / 2 + 4} textAnchor="middle">
              32
            </text>
            <text x={8 * unit} y={8 * unit - 12} textAnchor="middle" fill="#3fcb84">
              r 8
            </text>
            <text x={size + 30} y={12.5 * unit + 4} textAnchor="middle">
              12.5
            </text>
            <text x={size + 30} y={19.5 * unit + 4} textAnchor="middle">
              19.5
            </text>
            <text x={size / 2} y={size + 34} textAnchor="middle">
              stroke 2.6 · round caps
            </text>
          </g>
        </svg>
      </div>
    </figure>
  );
}

function SectionHeading({ id, title, body }: { id: string; title: string; body: string }) {
  return (
    <div className="flex flex-col gap-5 lg:flex-row lg:items-end lg:justify-between">
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
          body="Every logo comes as an SVG and a PNG, for dark and light backgrounds. Switch a tile to see the version for each."
        />
        <ul className="grid gap-4 lg:grid-cols-3">
          {assets.map((asset) => (
            <AssetTile key={asset.title} asset={asset} />
          ))}
        </ul>
      </Container>
    </section>
  );
}

function AssetTile({ asset }: { asset: Asset }) {
  const [background, setBackground] = useState<Background>("dark");
  const stem = asset.stems[background];
  const [w, h] = manifest.assets[stem as keyof typeof manifest.assets].png;
  return (
    <li
      className={cx(
        "flex flex-col overflow-clip rounded-2xl bg-raised shadow-land-card",
        asset.wide && "lg:col-span-2",
      )}
    >
      <div
        className={cx(
          "relative flex h-[220px] items-center justify-center px-8 transition-colors duration-200 sm:h-[260px]",
          background === "dark" ? "bg-land-well" : "bg-white",
        )}
      >
        <img
          src={`/brand/${stem}.svg`}
          alt={`convt ${asset.title.toLowerCase()} for ${background} backgrounds`}
          className={cx("block w-auto max-w-full", asset.previewClass)}
        />
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
        </div>
        <div className="flex shrink-0 flex-wrap gap-1.5">
          <CopyButton
            text={svgs[stem]}
            label="Copy SVG"
            className="h-8 rounded-lg px-2.5 text-[13px]/[16px]"
          />
          <FileLink href={`/brand/${stem}.svg`} label="SVG" />
          <FileLink href={`/brand/${stem}.png`} label="PNG" detail={`${w}×${h}`} />
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
  return (
    <div
      role="radiogroup"
      aria-label={`${label} background`}
      className={cx(
        "flex rounded-lg p-0.5 transition-colors duration-200",
        value === "dark"
          ? "bg-[#161918] shadow-[0_0_0_1px_#2e3331]"
          : "bg-[#f3f4f3] shadow-[0_0_0_1px_#e0e3e1]",
        className,
      )}
    >
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
              const sibling =
                event.currentTarget.previousElementSibling ??
                event.currentTarget.nextElementSibling;
              (sibling as HTMLElement | null)?.focus();
            }}
            className={cx(
              "h-7 rounded-md px-2.5 text-[12px]/[16px] font-medium capitalize transition-colors duration-150",
              focusRing,
              value === "dark"
                ? selected
                  ? "bg-[#2a2f2d] text-white"
                  : "text-[#a1a6a3] hover:text-white"
                : selected
                  ? "bg-white text-[#0a0a0a] shadow-[0_0_0_1px_#e0e3e1,0_1px_2px_#0000000f]"
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

const smallButton =
  "inline-flex shrink-0 items-center justify-center gap-1.5 font-medium whitespace-nowrap bg-sunken text-ink shadow-land-secondary transition-colors duration-150 hover:bg-hover";

function FileLink({ href, label, detail }: { href: string; label: string; detail?: string }) {
  return (
    <a
      href={href}
      download
      className={cx(smallButton, "h-8 rounded-lg px-2.5 text-[13px]/[16px]", focusRing)}
    >
      <DownloadGlyph />
      {label}
      {detail && (
        <span className="font-mono text-[11px]/[16px] font-normal text-land-muted">{detail}</span>
      )}
    </a>
  );
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

/** Copies `text` and confirms in place for a moment. */
function useCopy() {
  const [copied, setCopied] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout>>(undefined);
  useEffect(() => () => clearTimeout(timer.current), []);
  const copy = async (text: string) => {
    try {
      await navigator.clipboard.writeText(text);
    } catch {
      // The Clipboard API needs HTTPS and permission; the old selection copy works without.
      if (!copyBySelection(text)) return;
    }
    setCopied(true);
    clearTimeout(timer.current);
    timer.current = setTimeout(() => setCopied(false), 1600);
  };
  return { copied, copy };
}

function CopyButton({
  text,
  label,
  className,
}: {
  text: string;
  label: string;
  className?: string;
}) {
  const { copied, copy } = useCopy();
  return (
    <button
      type="button"
      onClick={() => copy(text)}
      className={cx(smallButton, focusRing, className)}
    >
      <span className="relative grid size-3.5 place-items-center" aria-hidden="true">
        <CopyGlyph
          className={cx(
            "col-start-1 row-start-1 transition-[opacity,scale] duration-150 motion-reduce:transition-none",
            copied ? "scale-50 opacity-0" : "opacity-100",
          )}
        />
        <CheckGlyph
          className={cx(
            "col-start-1 row-start-1 text-green transition-[opacity,scale] duration-150 motion-reduce:transition-none",
            copied ? "opacity-100" : "scale-50 opacity-0",
          )}
        />
      </span>
      {/* Both labels share one cell so the button keeps its width. */}
      <span className="grid">
        <span className={cx("col-start-1 row-start-1", copied && "invisible")}>{label}</span>
        <span className={cx("col-start-1 row-start-1", !copied && "invisible")} aria-live="polite">
          {copied ? "Copied" : ""}
        </span>
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
          body="One green, drawn as a gradient on the mark, and a set of near-black neutrals. Click a swatch to copy its hex."
        />
        <div className="flex flex-col gap-4">
          <GradientSwatch />
          <ul className="grid grid-cols-2 gap-4 md:grid-cols-4">
            {colors.map((color) => (
              <li key={color.hex}>
                <Swatch {...color} />
              </li>
            ))}
          </ul>
        </div>
      </Container>
    </section>
  );
}

const gradientCss = "linear-gradient(180deg, #2FBF78 0%, #1F9A5C 100%)";

function GradientSwatch() {
  const { copied, copy } = useCopy();
  return (
    <button
      type="button"
      onClick={() => copy(gradientCss)}
      className={cx(
        "group relative flex h-40 w-full flex-col justify-between overflow-clip rounded-2xl p-5 text-left shadow-[inset_0_1px_0_#ffffff38,0_0_0_1px_#157f4a] md:h-48",
        focusRing,
      )}
      style={{ backgroundImage: gradientCss }}
    >
      <span className="text-[15px]/[20px] font-medium text-[#0a0b0b]">Mark gradient</span>
      <span className="flex items-end justify-between gap-4">
        <span className="font-mono text-[13px]/[18px] text-balance text-[#0a0b0b]">
          {gradientCss}
        </span>
        <CopyHint copied={copied} label="Copy CSS" />
      </span>
    </button>
  );
}

function Swatch({ name, hex, role }: { name: string; hex: string; role: string }) {
  const { copied, copy } = useCopy();
  return (
    <button
      type="button"
      onClick={() => copy(hex)}
      aria-label={`${name}, ${hex}. Copy hex`}
      className={cx(
        "group flex w-full flex-col overflow-clip rounded-2xl bg-raised text-left shadow-land-card",
        focusRing,
      )}
    >
      <span
        className="relative flex h-24 w-full items-end justify-end p-3 shadow-[inset_0_-1px_0_#ffffff0f] md:h-28"
        style={{ backgroundColor: hex }}
      >
        <CopyHint copied={copied} label="Copy" />
      </span>
      <span className="flex flex-col gap-1 p-4">
        <span className="flex flex-col gap-0.5 sm:flex-row sm:items-baseline sm:justify-between sm:gap-2">
          <span className="text-[15px]/[20px] font-medium text-ink">{name}</span>
          <span className="font-mono text-[12px]/[16px] text-ink-2">{hex}</span>
        </span>
        <span className="text-[13px]/[18px] text-land-muted">{role}</span>
      </span>
    </button>
  );
}

/** "Copy" on hover or focus, "Copied" after a click. Readable on any swatch. */
function CopyHint({ copied, label }: { copied: boolean; label: string }) {
  return (
    <span
      aria-live="polite"
      className={cx(
        "shrink-0 rounded-md bg-[#0a0b0bcc] px-2 py-1 font-mono text-[11px]/[14px] text-white backdrop-blur-sm transition-opacity duration-150 motion-reduce:transition-none",
        copied
          ? "opacity-100"
          : "opacity-0 group-hover:opacity-100 group-focus-visible:opacity-100 pointer-coarse:opacity-100",
      )}
    >
      {copied ? "Copied" : label}
    </span>
  );
}

function Typography() {
  return (
    <section aria-labelledby="type-title" className="pt-16 md:pt-[120px]">
      <Container className="flex flex-col gap-10 md:gap-14">
        <SectionHeading
          id="type-title"
          title="Type"
          body="Geist for everything people read, Geist Mono for file names, sizes and commands. Both are free under the SIL Open Font License."
        />
        <div className="grid gap-4 lg:grid-cols-[3fr_2fr]">
          <TypeCard
            name="Geist"
            usage="Wordmark in SemiBold, headings in Medium with tight tracking, body in Regular."
            href="https://vercel.com/font"
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

const donts = [
  { label: "Don't recolor it", style: { filter: "hue-rotate(150deg)" } },
  { label: "Don't stretch it", style: { transform: "scaleX(1.45)" } },
  { label: "Don't rotate it", style: { transform: "rotate(-18deg)" } },
  {
    label: "Don't add effects",
    style: { filter: "drop-shadow(0 0 14px #4cc283) drop-shadow(4px 6px 0 #ffffff)" },
  },
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
          <Rule title="Lowercase, always">
            Write <span className="font-medium text-ink">convt</span>, even at the start of a
            sentence. Not Convt, ConvT or CONVT.
          </Rule>
          <Rule title="Give it room">
            Keep clear space of a quarter of the mark's width on every side, and set the mark at 16
            px or larger.
          </Rule>
          <Rule title="Use the files as they are">
            Pick the version made for your background instead of editing one. On a busy photo, use
            the one-color mark.
          </Rule>
        </div>
        <ul className="grid grid-cols-2 gap-4 md:grid-cols-4">
          {donts.map((dont) => (
            <li key={dont.label} className="flex flex-col gap-3">
              <div className="flex aspect-[4/3] items-center justify-center overflow-clip rounded-2xl bg-land-well shadow-land-card">
                <img
                  src="/brand/convt-mark.svg"
                  alt=""
                  className="size-16 sm:size-20"
                  style={dont.style}
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

function Rule({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div className="flex flex-col gap-2 rounded-2xl bg-raised p-6 shadow-land-card">
      <h3 className="text-[15px]/[20px] font-medium text-ink">{title}</h3>
      <p className="text-[14px]/[21px] text-pretty text-ink-2">{children}</p>
    </div>
  );
}

function DownloadGlyph() {
  return (
    <svg width="14" height="14" viewBox="0 0 14 14" aria-hidden="true" className="shrink-0">
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

function CopyGlyph({ className }: { className?: string }) {
  return (
    <svg width="14" height="14" viewBox="0 0 14 14" className={className}>
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

function CheckGlyph({ className }: { className?: string }) {
  return (
    <svg width="14" height="14" viewBox="0 0 14 14" className={className}>
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
