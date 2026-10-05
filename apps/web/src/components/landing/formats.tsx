import type { ReactNode } from "react";

import { Container } from "./ui";

const groups = [
  {
    name: "Images",
    formats: [
      "JPG",
      "PNG",
      "WEBP",
      "HEIC",
      "AVIF",
      "GIF",
      "TIFF",
      "BMP",
      "ICO",
      "TGA",
      "PPM",
      "QOI",
      "EXR",
      "SVG",
    ],
    preview: <ImagePreview />,
  },
  {
    name: "Video",
    formats: ["MP4", "MOV", "WEBM", "MKV", "AVI"],
    preview: <VideoPreview />,
  },
  {
    name: "Audio",
    formats: ["MP3", "WAV", "FLAC", "AAC", "M4A", "OGG", "OPUS"],
    preview: <AudioPreview />,
  },
  {
    name: "Documents",
    formats: [
      "PDF",
      "DOCX",
      "DOC",
      "ODT",
      "RTF",
      "TXT",
      "HTML",
      "PPTX",
      "PPT",
      "ODP",
      "XLSX",
      "XLS",
      "ODS",
      "CSV",
    ],
    preview: <DocumentPreview />,
  },
];

export function Formats() {
  return (
    <Container id="formats" className="scroll-mt-6 md:pb-[140px]">
      <div className="flex flex-col items-center gap-3.5 pt-20 pb-10 text-center md:pt-[140px] md:pb-12">
        <h2 className="text-[34px]/[40px] font-medium tracking-[-0.035em] text-ink md:text-[44px]/[48px]">
          40 formats, one menu.
        </h2>
        <p className="max-w-[520px] text-[17px]/[26px] text-ink-2">
          Images, video, audio and documents. Right-click a file, pick what you want, done.
        </p>
      </div>
      <ul className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
        {groups.map((group) => (
          <FormatCard key={group.name} {...group} />
        ))}
      </ul>
    </Container>
  );
}

function FormatCard({
  name,
  formats,
  preview,
}: {
  name: string;
  formats: string[];
  preview: ReactNode;
}) {
  return (
    <li className="flex flex-col overflow-clip rounded-[14px] bg-raised shadow-land-card">
      <div className="h-[200px] shrink-0">{preview}</div>
      <div className="flex flex-col gap-3.5 p-5">
        <div className="flex items-baseline justify-between">
          <h3 className="text-[17px]/[22px] font-medium text-ink">{name}</h3>
          <span className="font-mono text-[12px]/[16px] text-land-muted">
            {formats.length}
            <span className="sr-only"> formats</span>
          </span>
        </div>
        <ul className="flex flex-wrap gap-1.5">
          {formats.map((format) => (
            <li
              key={format}
              className="rounded-md bg-sunken px-[7px] py-[3px] font-mono text-[11.5px]/[14px] text-ink-2"
            >
              {format}
            </li>
          ))}
        </ul>
      </div>
    </li>
  );
}

function ImagePreview() {
  return (
    <img
      src="/landing/miso.jpg"
      alt=""
      width={800}
      height={550}
      loading="lazy"
      className="size-full object-cover object-[50%_40%]"
    />
  );
}

function VideoPreview() {
  return (
    <div
      aria-hidden="true"
      className="flex size-full flex-col items-center justify-center gap-3 bg-land-well px-6"
    >
      <div className="relative flex h-[124px] w-full items-center justify-center overflow-clip rounded-lg">
        <img
          src="/landing/miso.jpg"
          alt=""
          loading="lazy"
          className="absolute inset-0 size-full object-cover object-[50%_75%]"
        />
        <div className="absolute inset-0 bg-[#00000047]" />
        <div className="relative flex size-10 items-center justify-center rounded-full bg-[#ffffffeb]">
          <svg width="14" height="16" viewBox="0 0 14 16">
            <path d="M2 1.5v13L13 8 2 1.5Z" fill="#0a0a0a" />
          </svg>
        </div>
      </div>
      <div className="flex w-full items-center gap-2.5 font-mono text-[10.5px]/[14px]">
        <span className="text-[#9aa19d]">0:12</span>
        <div className="h-[3px] flex-1 rounded-[2px] bg-[#ffffff24]">
          <div className="h-[3px] w-[28%] rounded-[2px] bg-land-accent" />
        </div>
        <span className="text-land-muted">0:42</span>
      </div>
    </div>
  );
}

// Bar heights from the design: [y, height] on a 72px tall canvas, 8px apart.
const playedBars = [
  [30, 12],
  [24, 24],
  [14, 44],
  [20, 32],
  [8, 56],
  [18, 36],
  [26, 20],
  [12, 48],
  [4, 64],
  [16, 40],
  [24, 24],
  [10, 52],
];
const restBars = [
  [20, 32],
  [28, 16],
  [14, 44],
  [6, 60],
  [18, 36],
  [26, 20],
  [12, 48],
  [22, 28],
  [30, 12],
  [16, 40],
  [8, 56],
  [20, 32],
  [28, 16],
  [18, 36],
  [24, 24],
  [14, 44],
  [26, 20],
  [31, 10],
];

function AudioPreview() {
  return (
    <div
      aria-hidden="true"
      className="flex size-full flex-col justify-center gap-3.5 bg-[#0f1a14] px-6"
    >
      <svg width="240" height="72" viewBox="0 0 240 72" className="max-w-full shrink-0">
        {[...playedBars, ...restBars].map(([y, height], i) => (
          <rect
            key={i}
            x={i * 8}
            y={y}
            width="4"
            height={height}
            rx="2"
            fill={i < playedBars.length ? "#3fcb84" : "#2a4a39"}
          />
        ))}
      </svg>
      <div className="flex items-center justify-between font-mono text-[10.5px]/[14px]">
        <span className="text-green">voice-memo.m4a</span>
        <span className="text-land-muted">1:04</span>
      </div>
    </div>
  );
}

const textLines = ["100%", "94%", "100%", "62%", null, "100%", "88%", "96%", "40%"];

function DocumentPreview() {
  return (
    <div aria-hidden="true" className="relative size-full overflow-clip bg-sunken">
      {/* Fixed 288px stage, centered, so the stacked pages keep their layout at any card width. */}
      <div className="absolute inset-y-0 left-1/2 w-[288px] -translate-x-1/2">
        <div className="absolute top-[34px] left-[118px] flex h-[170px] w-[132px] origin-top-left rotate-6 flex-col rounded-md bg-hover p-3 shadow-[0_0_0_1px_#2e3331,0_6px_16px_#00000066]">
          <div className="h-4 shrink-0 bg-green-tint shadow-[inset_0_-1px_0_#1e3a2a]" />
          {Array.from({ length: 6 }, (_, i) => (
            <div key={i} className="h-4 shrink-0 shadow-[inset_0_-1px_0_#2a2e2d]" />
          ))}
        </div>
        <div className="absolute top-7 left-11 flex h-[176px] w-[132px] origin-top-left -rotate-4 flex-col gap-[7px] rounded-md bg-hover px-3.5 py-4 shadow-[0_0_0_1px_#2e3331,0_8px_20px_#00000080]">
          <div className="h-[7px] w-[70%] shrink-0 rounded-[2px] bg-[#c9cecb]" />
          <div className="h-1.5 shrink-0" />
          {textLines.map((width, i) =>
            width ? (
              <div
                key={i}
                className="h-1 shrink-0 rounded-[2px] bg-line-strong"
                style={{ width }}
              />
            ) : (
              <div key={i} className="h-1 shrink-0" />
            ),
          )}
        </div>
      </div>
    </div>
  );
}
