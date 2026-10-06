import { GITHUB_URL, routes } from "#/lib/site";

import { ButtonLink, Container, DownloadIcon } from "./ui";

export function Hero() {
  return (
    <Container>
      <div className="flex flex-col items-center gap-[22px] pt-14 pb-12 text-center md:pt-[88px] md:pb-16">
        <h1 className="max-w-[900px] text-[40px]/[44px] font-medium tracking-[-0.04em] text-balance text-ink sm:text-[56px]/[60px] lg:text-[68px]/[72px]">
          Convert any file
          <br className="hidden sm:inline" /> with a right-click.
        </h1>
        <p className="max-w-[560px] text-[17px]/[26px] text-ink-2 sm:text-[18px]/[28px]">
          Images, video, audio and documents, converted on your own computer. Nothing gets uploaded.
        </p>
        <div className="flex flex-wrap justify-center gap-2.5 pt-2.5">
          <ButtonLink
            variant="primary"
            href={`${routes.download}?os=macos`}
            className="rounded-[10px] px-[18px] py-[11px] text-[15px]/[18px]"
          >
            <DownloadIcon />
            Download for macOS
          </ButtonLink>
          <ButtonLink
            variant="secondary"
            href={GITHUB_URL}
            className="rounded-[10px] px-[18px] py-[11px] text-[15px]/[18px]"
          >
            Star on GitHub
          </ButtonLink>
        </div>
      </div>
      <ConvertPanel />
    </Container>
  );
}

function ConvertPanel() {
  return (
    <figure className="bg-land-glow flex items-center justify-center overflow-clip rounded-2xl bg-bottom px-5 py-12 lg:h-[560px] lg:py-0">
      <figcaption className="sr-only">A 4.8 MB HEIC photo converted to a 612 KB WebP.</figcaption>
      <div className="flex w-full min-w-0 flex-col items-center gap-7 lg:w-auto lg:flex-row">
        <FileCard name="miso.heic" size="4.8 MB" alt="A cat photo saved as miso.heic" />
        <div className="bg-land-green flex size-12 shrink-0 rotate-90 items-center justify-center rounded-full shadow-[inset_0_1px_0_#ffffff47,0_0_0_1px_#157f4a,0_4px_12px_#0a3c2340] lg:rotate-0">
          <svg width="20" height="20" viewBox="0 0 24 24" aria-hidden="true">
            <path
              d="M5 12h14M13 6l6 6-6 6"
              fill="none"
              stroke="#ffffff"
              strokeWidth="2.2"
              strokeLinecap="round"
              strokeLinejoin="round"
            />
          </svg>
        </div>
        <FileCard name="miso.webp" size="612 KB" alt="The same photo converted to miso.webp" done />
      </div>
    </figure>
  );
}

function FileCard({
  name,
  size,
  alt,
  done,
}: {
  name: string;
  size: string;
  alt: string;
  done?: boolean;
}) {
  return (
    <div className="flex w-[320px] max-w-full flex-col gap-3 rounded-[14px] bg-raised px-2.5 pt-2.5 pb-3.5 shadow-land-float">
      <img
        src="/landing/miso.jpg"
        alt={alt}
        width={300}
        height={220}
        className="aspect-[300/220] w-full rounded-lg object-cover"
      />
      <div className="flex items-center justify-between px-1">
        <span className="text-[14px]/[18px] font-medium text-ink">{name}</span>
        <span
          className={
            done
              ? "font-mono text-[12px]/[16px] text-green"
              : "font-mono text-[12px]/[16px] text-land-muted"
          }
        >
          {size}
        </span>
      </div>
    </div>
  );
}
