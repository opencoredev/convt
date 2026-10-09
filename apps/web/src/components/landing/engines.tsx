import { Mark } from "#/components/logo";

import { Container, revealDelay } from "./ui";

const engines = [
  {
    name: "FFmpeg",
    from: "clip.mov",
    to: "clip.mp4",
    description: "Moves video and audio between containers and codecs.",
    kind: "Video · Audio",
    formats: "MP4 MOV WEBM MKV AVI MP3 WAV FLAC AAC M4A OGG OPUS",
  },
  {
    name: "LibreOffice",
    from: "deck.pptx",
    to: "deck.pdf",
    description:
      "Opens Word, Excel and PowerPoint files and their open formats, and saves any of them as PDF.",
    kind: "Documents",
    formats: "DOCX DOC ODT RTF TXT HTML PPTX PPT ODP XLSX XLS ODS CSV",
  },
  {
    name: "PDFium",
    from: "lease.pdf",
    to: "lease-1.png",
    description: "The PDF renderer inside Chrome. Turns every page into an image.",
    kind: "PDF",
    formats: "PDF to PNG or JPG, one image per page",
  },
  {
    name: "image + resvg",
    from: "icon.svg",
    to: "icon.png",
    description: "Pure Rust decoders for photos and a Rust SVG renderer.",
    kind: "Images",
    formats: "JPG PNG WEBP AVIF GIF TIFF BMP ICO TGA PPM QOI EXR SVG",
  },
];

// The 1056px diagram (pill, connectors, four 252px cards) needs this much room;
// below it the cards fall back to a grid without connectors.
export function Engines() {
  return (
    <section aria-labelledby="engines-title" className="py-16 md:py-[120px]">
      <Container className="flex flex-col gap-10 md:gap-14">
        <div className="flex flex-col gap-5 lg:flex-row lg:items-end lg:justify-between">
          <h2
            id="engines-title"
            className="max-w-[560px] text-[34px]/[40px] font-medium tracking-[-0.035em] text-ink md:text-[44px]/[48px]"
          >
            Built on FFmpeg, LibreOffice and PDFium.
          </h2>
          <p className="max-w-[420px] shrink-0 text-[17px]/[26px] text-ink-2 lg:w-[420px]">
            The same tools video editors and office suites run on. convt drives them from Rust, on
            your computer, so your files stay there.
          </p>
        </div>
        <div className="relative overflow-clip rounded-2xl shadow-[0_0_0_1px_var(--line)]">
          <div aria-hidden="true" className="bg-land-glow absolute inset-0 bg-top" />
          <div className="relative flex flex-col items-center px-5 py-10 sm:px-8 min-[1120px]:h-[520px] min-[1120px]:justify-center min-[1120px]:p-0">
            <div className="flex items-center gap-2.5 rounded-xl bg-land-well py-2.5 pr-4 pl-3 shadow-[0_0_0_1px_#4cc28373,0_0_24px_#4cc2832e]">
              <Mark size={22} />
              <span className="font-mono text-[13px]/[16px] text-ink">convt</span>
              <span className="font-mono text-[12px]/[16px] text-land-muted">picks the engine</span>
            </div>
            <Connectors />
            <ul className="mt-6 grid w-full gap-4 sm:grid-cols-2 min-[1120px]:mt-0 min-[1120px]:flex min-[1120px]:w-[1056px]">
              {engines.map((engine, i) => (
                <EngineCard key={engine.name} index={i} {...engine} />
              ))}
            </ul>
          </div>
        </div>
      </Container>
    </section>
  );
}

function Connectors() {
  const stroke = "#4cc28373";
  return (
    <svg
      width="1056"
      height="56"
      viewBox="0 0 1056 56"
      aria-hidden="true"
      className="hidden shrink-0 min-[1120px]:block"
    >
      <path
        d="M528 0v20a8 8 0 0 1-8 8H134a8 8 0 0 0-8 8v20"
        fill="none"
        stroke={stroke}
        strokeWidth="1.5"
      />
      <path
        d="M528 0v20a8 8 0 0 1-8 8H402a8 8 0 0 0-8 8v20"
        fill="none"
        stroke={stroke}
        strokeWidth="1.5"
      />
      <path
        d="M528 0v20a8 8 0 0 0 8 8h118a8 8 0 0 1 8 8v20"
        fill="none"
        stroke={stroke}
        strokeWidth="1.5"
      />
      <path
        d="M528 0v20a8 8 0 0 0 8 8h386a8 8 0 0 1 8 8v20"
        fill="none"
        stroke={stroke}
        strokeWidth="1.5"
      />
      {[126, 394, 662, 930].map((cx) => (
        <circle key={cx} cx={cx} cy="53" r="3" fill="#4cc283" />
      ))}
    </svg>
  );
}

function EngineCard({
  name,
  from,
  to,
  description,
  kind,
  formats,
  index,
}: (typeof engines)[number] & { index: number }) {
  return (
    <li
      data-reveal=""
      style={revealDelay(index)}
      className="reveal lift flex flex-col justify-between gap-7 rounded-[14px] bg-raised/90 p-6 shadow-[0_0_0_1px_#0000000f,0_24px_48px_#0a1e141a] backdrop-blur-sm hover:shadow-[0_0_0_1px_#1fa46366,0_24px_48px_#0a1e1424] min-[1120px]:w-[252px] dark:bg-[#0b0d0cdb] dark:shadow-[0_0_0_1px_#ffffff14,0_24px_48px_#00000066] dark:hover:shadow-[0_0_0_1px_#4cc28366,0_24px_48px_#00000080] min-[1120px]:shrink-0"
    >
      <div className="flex flex-col gap-5">
        <p className="flex items-center gap-2 font-mono text-[12px]/[16px] text-land-mono">
          {from}
          <span aria-hidden="true" className="text-land-accent">
            →
          </span>
          <span className="sr-only">to</span>
          {to}
        </p>
        <div className="flex flex-col gap-2">
          <h3 className="text-[20px]/[24px] font-semibold tracking-[-0.01em] text-ink">{name}</h3>
          <p className="text-[14px]/[21px] text-ink-2">{description}</p>
        </div>
      </div>
      <div className="flex flex-col gap-3 border-t border-line pt-4 dark:border-[#ffffff14]">
        <p className="font-mono text-[11px]/[14px] tracking-[0.06em] text-land-label uppercase">
          {kind}
        </p>
        <p className="min-h-[60px] font-mono text-[12px]/[20px] text-land-mono">{formats}</p>
      </div>
    </li>
  );
}
