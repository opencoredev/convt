import { createFileRoute } from "@tanstack/react-router";
import type { ReactNode } from "react";

import { homebrewCommands } from "#/components/app/command-block";
import { cx, focusRing } from "#/components/app/ui";
import {
  Checked,
  Code,
  Commands,
  FactList,
  GuideSection,
  Output,
  Prose,
} from "#/components/site/guide";
import { TextLink, siteColumn } from "#/components/site/layout";
import { guidePath } from "#/lib/guides";
import {
  commandLineConverter,
  handbrakeAlternative,
  handbrakeAlternativeDownloadUrl,
  handbrakeAlternativeHead,
  macFileConverter,
} from "#/lib/landing-pages";
import { routes } from "#/lib/site";

// A comparison page for "handbrake alternative(s)". Every competitor fact (version, price,
// platforms, what it converts) was read on the vendor's own site or GitHub releases on
// 2026-11-24 and is linked from its block. HandBrake's limits are quoted from its own docs and
// its developers' replies on GitHub. Every convt command and output here was run on the released
// convt 0.5.0 Linux CLI for earlier pages (the Mac page, the MOV, MKV and MP3 guides); convt's
// video settings come from crates/convt-engines/src/ffmpeg.rs at v0.5.0.

const CHECKED = "November 24, 2026";

const page = handbrakeAlternative;
const downloadUrl = handbrakeAlternativeDownloadUrl;

const handbrakeAbout = "https://handbrake.fr/docs/en/latest/introduction/about.html";
const handbrakeFeatures = "https://handbrake.fr/features.php";

const sections = [
  { id: "compare", title: "Side by side" },
  { id: "switch", title: "When to switch" },
  { id: "tools", title: "The alternatives" },
  { id: "handbrake-better", title: "Where HandBrake is better" },
  { id: "convt", title: "The same jobs in convt" },
  { id: "faq", title: "Questions" },
];

const rows: [string, string, string, string][] = [
  ["HandBrake 1.11.2", "Free, open source", "Mac, Windows, Linux", "Video to MP4, MKV or WebM"],
  [
    "convt 0.5.0",
    "$39 once",
    "Mac (Apple silicon), Windows, Linux",
    "Video, audio, images, PDFs, Office documents",
  ],
  [
    "Shutter Encoder 20.5",
    "Free (donations)",
    "Mac (Apple silicon), Windows, Linux",
    "Video, audio, images",
  ],
  [
    "Permute 4",
    "$14.99 once, or Setapp",
    "Mac only (macOS 26, Apple silicon)",
    "Video, audio, images, PDF pages to images",
  ],
  [
    "VidCoder 12.23",
    "Free, open source",
    "Windows only",
    "Video to MP4 or MKV, with HandBrake's engine",
  ],
  [
    "FFmpeg 9.1",
    "Free, open source",
    "Mac, Windows, Linux",
    "Video and audio, from the command line",
  ],
];

type Tool = {
  name: string;
  href: string;
  bestFor: ReactNode;
  price: ReactNode;
  platforms: ReactNode;
  converts: ReactNode;
  batch: ReactNode;
  shortfalls: ReactNode;
};

const tools: Tool[] = [
  {
    name: "convt",
    href: "/",
    bestFor: "Converting video, audio, images and documents with a right-click or one command.",
    price: "$39 once for Desktop, with every future desktop update. No free tier.",
    platforms: "macOS 13 or later on Apple silicon, Windows x86_64, Linux x86_64.",
    converts: (
      <>
        MP4, MOV, WebM, MKV, AVI and GIF; MP3, WAV, FLAC, AAC, M4A, OGG and Opus; a dozen image
        formats; PDF pages to images; Word, Excel and PowerPoint to PDF (with LibreOffice, or the
        document pack on Windows). <TextLink href={routes.formats}>Every format</TextLink>.
      </>
    ),
    batch: (
      <>
        A folder in one command, subfolders with <Code>-r</Code>; a Convert with convt menu in
        Finder and in Linux file managers. The Windows Explorer menu isn't in the installer yet.
      </>
    ),
    shortfalls:
      "Version 0.5.0 always re-encodes video, picks at most one audio track for you and can't shrink an MP4 into a smaller MP4. No filters, device presets, or track and chapter controls.",
  },
  {
    name: "Shutter Encoder",
    href: "https://www.shutterencoder.com/",
    bestFor: "Editors who want a free tool with pro codecs and cuts without re-encoding.",
    price: "Free, funded by donations. GPL-3.0.",
    platforms: "Windows 7 to 11 (64-bit), macOS 11.3 or later on Apple silicon, Linux.",
    converts:
      "Video, audio (WAV, FLAC, MP3, AAC, Opus) and images (JPEG, JPEG XL). ProRes and DNxHR, plus DVD and Blu-ray tools.",
    batch: "A render queue and presets, and command-line options since version 20.5.",
    shortfalls: "No document conversion. Its long list of functions takes some learning.",
  },
  {
    name: "Permute",
    href: "https://software.charliemonroe.net/permute/",
    bestFor: "Mac users who want drag-and-drop conversion of media files.",
    price: "$14.99 once (up to 3 Macs, tax extra), or part of a Setapp subscription.",
    platforms: "Mac only. Version 4 needs macOS 26 or later and Apple silicon.",
    converts:
      "Video, audio and images; PDF pages to images; track extraction without re-encoding, OCR and transcription.",
    batch: "Drag in many files; edits apply to all of them.",
    shortfalls:
      "No Windows or Linux build, no Office documents, and its site names no command-line tool.",
  },
  {
    name: "VidCoder",
    href: "https://vidcoder.net/",
    bestFor: "Windows users who like HandBrake's output but want a simpler queue.",
    price: "Free. GPL-2.0.",
    platforms: "Windows 10 and 11, 64-bit only.",
    converts: "Video to MP4 or MKV, using HandBrake as its encoding engine.",
    batch: "Batch encoding with several encodes at once, and a watched folder.",
    shortfalls: "Same limits as HandBrake (no audio-only files, no remux), and no Mac build.",
  },
  {
    name: "FFmpeg",
    href: "https://ffmpeg.org/",
    bestFor: "People at ease in a terminal who want exact control, including remuxing.",
    price: "Free. LGPL 2.1 or later; GPL when built with GPL parts.",
    platforms: "Mac, Windows and Linux. ffmpeg.org ships source only; builds come from others.",
    converts: "Almost any video or audio format, remux without re-encoding, frames to images.",
    batch: "Shell loops you write yourself.",
    shortfalls: "No interface at all. Flags differ per codec, and a typo can leave an empty file.",
  },
];

export const Route = createFileRoute("/_site/handbrake-alternative")({
  head: handbrakeAlternativeHead,
  component: HandbrakeAlternative,
});

function HandbrakeAlternative() {
  return (
    <article className={cx(siteColumn, "pt-10 pb-20 md:pt-14")}>
      <p className="font-mono text-xs/4 text-ink-2 uppercase">Compared</p>
      <header className="mt-3 flex max-w-[720px] flex-col gap-4">
        <h1 className="text-[34px]/10 font-semibold tracking-[-0.03em] text-balance md:text-[44px]/12">
          {page.heading}
        </h1>
        <div className="flex flex-col gap-4 text-[17px]/[27px] text-pretty text-ink-2">
          <p>
            Keep HandBrake if you want a free video encoder with presets, filters, every audio and
            subtitle track, and smaller MP4s from big ones. Pick convt if your everyday job is
            turning video, audio, images or documents into another format from Finder&apos;s
            right-click menu or one command over a folder, for $39 once.
          </p>
          <p>
            HandBrake only makes video files, so it can&apos;t give you an MP3, a JPEG or a PDF.
            Below: when switching makes sense, five alternatives in the same format, and where
            HandBrake still wins.
          </p>
        </div>
        <p className="text-[13px]/5 text-ink-2">
          Versions and prices checked on <time dateTime="2026-11-24">{CHECKED}</time>
        </p>
        <div className="flex flex-wrap items-center gap-3 pt-1">
          <a
            href={downloadUrl}
            className={cx(
              "btn-primary inline-flex items-center justify-center rounded-lg px-4 py-2.5 text-sm/4.5 font-medium",
              focusRing,
            )}
          >
            Get convt
          </a>
          <TextLink href="#compare">Compare the six</TextLink>
        </div>
      </header>

      <div className="mt-12 grid gap-12 lg:grid-cols-[minmax(0,720px)_220px] lg:justify-between">
        <div className="flex min-w-0 flex-col gap-16">
          <GuideSection
            id="compare"
            title="HandBrake and five alternatives side by side"
            source={
              <Checked how="docs">
                From each tool&apos;s own site and release page on {CHECKED}; links in each block
                below.
              </Checked>
            }
          >
            <div className="overflow-x-auto rounded-2xl shadow-[inset_0_0_0_1px_var(--line)] [scrollbar-width:thin]">
              <table className="w-full min-w-[640px] border-collapse text-left text-[14px]/5">
                <thead className="bg-sunken text-ink-2">
                  <tr>
                    <th scope="col" className="px-4 py-3 font-medium">
                      Tool
                    </th>
                    <th scope="col" className="px-4 py-3 font-medium">
                      Price
                    </th>
                    <th scope="col" className="px-4 py-3 font-medium">
                      Runs on
                    </th>
                    <th scope="col" className="px-4 py-3 font-medium">
                      Converts
                    </th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-divider">
                  {rows.map(([tool, price, platforms, converts]) => (
                    <tr key={tool}>
                      <th scope="row" className="px-4 py-3 font-medium">
                        {tool}
                      </th>
                      <td className="px-4 py-3 text-ink-2">{price}</td>
                      <td className="px-4 py-3 text-ink-2">{platforms}</td>
                      <td className="px-4 py-3">{converts}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </GuideSection>

          <GuideSection
            id="switch"
            title="When should you switch from HandBrake?"
            source={
              <Checked how="docs">
                From HandBrake&apos;s <TextLink href={handbrakeAbout}>about page</TextLink>,{" "}
                <TextLink href={handbrakeFeatures}>feature list</TextLink> and its developers&apos;
                replies on GitHub.
              </Checked>
            }
          >
            <Prose>
              <p>
                Switch when the job is not &quot;re-encode this video&quot;. HandBrake is built for
                one thing, and these jobs fall outside it:
              </p>
              <ul className="flex list-disc flex-col gap-2 pl-5">
                <li>
                  <strong className="font-semibold">You want an audio file.</strong> HandBrake
                  writes MP4, MKV or WebM only. Asked for MP3 export in July 2025, its developers
                  answered that it&apos;s{" "}
                  <TextLink href="https://github.com/HandBrake/HandBrake/issues/7078">
                    not planned
                  </TextLink>
                  .
                </li>
                <li>
                  <strong className="font-semibold">You want a fast container change.</strong> Its
                  own docs say video is &quot;always converted&quot;, so an MKV to MP4 takes as long
                  as a full encode. When someone asked for remuxing in April 2026, the reply was
                  that video passthrough is{" "}
                  <TextLink href="https://github.com/HandBrake/HandBrake/issues/7826">
                    outside the project&apos;s scope
                  </TextLink>
                  .
                </li>
                <li>
                  <strong className="font-semibold">You convert photos or documents too.</strong>{" "}
                  HandBrake converts video only. In one report, a batch scan of a folder with HEIC
                  photos{" "}
                  <TextLink href="https://github.com/HandBrake/HandBrake/discussions/7657">
                    added them as one-second videos
                  </TextLink>{" "}
                  until the user excluded the extension in the scan settings.
                </li>
                <li>
                  <strong className="font-semibold">You batch a lot of files.</strong> HandBrake
                  scans many files into a list, then each one has to be{" "}
                  <TextLink href="https://handbrake.fr/docs/en/latest/advanced/queue.html">
                    added to the queue
                  </TextLink>
                  , a step users miss: dropping several files{" "}
                  <TextLink href="https://github.com/HandBrake/HandBrake/discussions/7463">
                    encoded only the first
                  </TextLink>{" "}
                  for one, and a folder of 88 clips{" "}
                  <TextLink href="https://github.com/HandBrake/HandBrake/discussions/8236">
                    gave one output
                  </TextLink>{" "}
                  for another. A{" "}
                  <TextLink href="https://github.com/HandBrake/HandBrake/issues/8089">
                    batch scan of ISO files
                  </TextLink>{" "}
                  isn&apos;t supported. Subfolder scanning exists, under Preferences, though{" "}
                  <TextLink href="https://github.com/HandBrake/HandBrake/issues/8240">
                    one user asked for it before finding it
                  </TextLink>
                  .
                </li>
              </ul>
              <p>
                If your job is shrinking or re-encoding video with fine control, none of these
                apply, and HandBrake is probably still the right tool.
              </p>
            </Prose>
          </GuideSection>

          <GuideSection id="tools" title="Five HandBrake alternatives">
            <Prose>
              <p>
                Each block has the same fields, so you can compare them line by line. If you only
                use a Mac, the <TextLink href={macFileConverter.path}>Mac file converter</TextLink>{" "}
                page also covers what Finder, Preview and QuickTime convert for free.
              </p>
            </Prose>
            {tools.map((tool) => (
              <ToolBlock key={tool.name} tool={tool} />
            ))}
          </GuideSection>

          <GuideSection id="handbrake-better" title="Where HandBrake is better">
            <Prose>
              <ul className="flex list-disc flex-col gap-2 pl-5">
                <li>It&apos;s free and open source (mostly GPL-2.0), on Mac, Windows and Linux.</li>
                <li>
                  It shrinks video into the same format: MP4 in, smaller MP4 out. convt 0.5.0
                  can&apos;t.
                </li>
                <li>
                  You choose which audio and subtitle tracks to keep, with audio passthrough for
                  AC-3, DTS, TrueHD, FLAC and more (which subtitles survive depends on the
                  container: MP4 can&apos;t hold Blu-ray picture subtitles). convt 0.5.0 picks at
                  most one audio track and has no track controls.
                </li>
                <li>
                  Filters (deinterlace, denoise, crop), device presets, chapters, and hardware
                  encoders from NVIDIA, Intel, AMD and Apple.
                </li>
                <li>It reads DVD and Blu-ray sources that aren&apos;t copy-protected.</li>
              </ul>
            </Prose>
          </GuideSection>

          <GuideSection
            id="convt"
            title="The same jobs in convt"
            source={<Checked how="tested">Run on convt 0.5.0, the current release.</Checked>}
          >
            <Prose>
              <p>
                An MP3 from a video, which HandBrake won&apos;t make, is one command. So are a MOV
                to MP4 and a 720-pixel copy, with no preset to pick:
              </p>
            </Prose>
            <Commands
              label="Terminal"
              commands={[
                "convt talk.mp4 --to mp3",
                "convt clip.mov --to mp4",
                "convt clip.mov --to mp4 --video-height 720 -o small",
              ]}
            />
            <Output label="Output, convt 0.5.0">
              {`talk.mp4 -> ./talk.mp3
clip.mov -> ./clip.mp4
clip.mov -> small/clip.mp4`}
            </Output>
            <Prose>
              <p>
                A whole folder, subfolders included, into a separate <Code>mp4</Code> folder with
                the same layout:
              </p>
            </Prose>
            <Commands label="Terminal" commands={["convt videos --to mp4 -r -o mp4"]} />
            <Output label="Output, convt 0.5.0">
              {`videos/2025/trip/b c.mov -> mp4/2025/trip/b c.mp4
videos/2025/x.mov -> mp4/2025/x.mp4
videos/a.MOV -> mp4/a.mp4
3 converted, 0 failed`}
            </Output>
            <Prose>
              <p>
                For MP4, convt makes H.264 video at CRF 20 with AAC audio at 192 kbit/s by default;{" "}
                <Code>--video-codec hevc</Code>, <Code>-q</Code> and <Code>--audio-bitrate</Code>{" "}
                change that. In a batch it converts one video at a time. For an MKV with several
                languages or subtitles, the{" "}
                <TextLink href={guidePath("mkv-to-mp4-mac")}>MKV to MP4 guide</TextLink> shows an
                FFmpeg command that keeps every audio track and text subtitles. More examples,
                including images and documents, are on the{" "}
                <TextLink href={commandLineConverter.path}>command-line page</TextLink> and in the{" "}
                <TextLink href={guidePath("mp4-flac-wav-to-mp3-mac")}>MP3 on Mac guide</TextLink>.
              </p>
              <p>
                On a Mac, Homebrew installs the app and the <Code>convt</Code> command:
              </p>
            </Prose>
            <Commands label="Homebrew" commands={homebrewCommands} />
            <aside className="flex flex-col gap-5 rounded-2xl bg-panel p-6 shadow-[inset_0_0_0_1px_var(--line)] sm:flex-row sm:items-center sm:justify-between sm:gap-8">
              <div className="flex flex-col gap-1.5 text-[14px]/[22px] text-ink-2">
                <p className="text-[16px]/6 font-semibold text-ink">
                  convt for Mac, Windows and Linux
                </p>
                <p>
                  Video, audio, images and documents, from Finder or Terminal. $39 once. Local
                  conversions never leave your computer.
                </p>
              </div>
              <a
                href={downloadUrl}
                className={cx(
                  "btn-primary inline-flex shrink-0 items-center justify-center rounded-lg px-4 py-2.5 text-sm/4.5 font-medium",
                  focusRing,
                )}
              >
                Get convt
              </a>
            </aside>
          </GuideSection>

          <GuideSection id="faq" title="Questions">
            <dl className="flex flex-col gap-6">
              <Question q="Is there a free HandBrake alternative?">
                Yes. Shutter Encoder and FFmpeg are free on Mac, Windows and Linux, and VidCoder is
                free on Windows. convt isn&apos;t free: the source is open (AGPL-3.0), but the
                signed app is $39 once, after a 7-day trial.
              </Question>
              <Question q="Can HandBrake convert video to MP3?">
                No. It only writes MP4, MKV and WebM, and its developers have said audio-only output
                is not planned. convt, Shutter Encoder, Permute and FFmpeg all make MP3s.
              </Question>
              <Question q="What's the simplest HandBrake alternative on a Mac?">
                For a few files, Permute or convt&apos;s Finder menu: pick the files, pick a format.
                Permute is Mac only: version 4 needs macOS 26 on Apple silicon, and the same
                purchase includes Permute 3 for macOS 11 to 15. convt runs on macOS 13 or later on
                Apple silicon and also does documents.
              </Question>
              <Question q="Does convt upload my videos?">
                Not for local conversions, which is everything on this page. The only uploads are
                cloud jobs, which you pick per job and which need Desktop + Cloud.
              </Question>
            </dl>
          </GuideSection>
        </div>

        <nav aria-label="On this page" className="max-lg:hidden">
          <div className="sticky top-8 flex flex-col gap-3">
            <p className="font-mono text-[11px]/4 text-ink-2 uppercase">On this page</p>
            <ol className="flex flex-col gap-2.5 border-l border-line">
              {sections.map((section) => (
                <li key={section.id}>
                  <a
                    href={`#${section.id}`}
                    className={cx(
                      "-ml-px block border-l border-transparent pl-3 text-[13px]/[18px] text-ink-2 hover:border-ink-2 hover:text-ink",
                      focusRing,
                    )}
                  >
                    {section.title}
                  </a>
                </li>
              ))}
            </ol>
          </div>
        </nav>
      </div>
    </article>
  );
}

function ToolBlock({ tool }: { tool: Tool }) {
  const id = `tool-${tool.name.toLowerCase().replaceAll(" ", "-")}`;
  return (
    <section aria-labelledby={id} className="flex flex-col gap-3">
      <h3 id={id} className="text-[19px]/7 font-semibold tracking-[-0.01em]">
        <TextLink href={tool.href}>{tool.name}</TextLink>
      </h3>
      <FactList
        facts={[
          [`${tool.name} is best for`, tool.bestFor],
          [`${tool.name} price`, tool.price],
          [`${tool.name} runs on`, tool.platforms],
          [`${tool.name} converts`, tool.converts],
          [`${tool.name} batches`, tool.batch],
          [`Where ${tool.name} falls short`, tool.shortfalls],
        ]}
      />
    </section>
  );
}

function Question({ q, children }: { q: string; children: ReactNode }) {
  return (
    <div className="flex flex-col gap-1.5">
      <dt className="text-[16px]/6 font-semibold">{q}</dt>
      <dd className="text-[16px]/[26px] text-ink-2">{children}</dd>
    </div>
  );
}
