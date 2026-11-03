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
import { guidePath, guides } from "#/lib/guides";
import {
  commandLineConverter,
  commandLineConverterDownloadUrl,
  commandLineConverterHead,
  macFileConverter,
} from "#/lib/landing-pages";
import { GITHUB_URL, RAYCAST_URL, routes } from "#/lib/site";

// Every convt command and result on this page comes from a run of the released convt 0.5.0
// Linux CLI (CNVM-16) on files we made: a 3-second 1080p ProRes MOV with a tone (FFmpeg), a WAV,
// a 1600 × 1200 PNG, PNGs in a folder with a subfolder, a DOCX made from a text file, and the
// PDF that came out of it. The DOCX to PDF run used a system LibreOffice, which is what the
// Windows document pack installs; Mac and Linux builds use a system LibreOffice in 0.5.0. `--json`, `--skill`, presets and exit codes were run the same way.

const page = commandLineConverter;
const downloadUrl = commandLineConverterDownloadUrl;

const convtExe = String.raw`& "$env:LOCALAPPDATA\Programs\convt\convt.exe"`;

const sections = [
  { id: "one-command", title: "One command for every file" },
  { id: "install", title: "Install on Mac, Windows, Linux" },
  { id: "folders", title: "Folders" },
  { id: "options", title: "Options" },
  { id: "presets", title: "Presets" },
  { id: "scripts", title: "Scripts and JSON" },
  { id: "agents", title: "AI coding agents" },
  { id: "compare", title: "FFmpeg, ImageMagick and others" },
  { id: "limits", title: "What it doesn't do" },
  { id: "guides", title: "Step-by-step guides" },
  { id: "faq", title: "Questions" },
];

const options: [ReactNode, ReactNode][] = [
  [<Code>-t, --to</Code>, "The target format, such as png, mp4, mp3 or pdf."],
  [<Code>-o, --out-dir</Code>, "Where results go. Without it, each lands next to its original."],
  [<Code>-r, --recursive</Code>, "Include subfolders of folder inputs."],
  [<Code>-q, --quality</Code>, "Lossy quality from 1 to 100 for JPEG, HEIC, AVIF and video."],
  [<Code>--max-size</Code>, "Longest edge of image output, in pixels."],
  [<Code>--pages, --dpi</Code>, "Which PDF pages to render (2, 1-3 or 4-) and at what resolution."],
  [<Code>--video-height</Code>, "Maximum video height in pixels. It never enlarges."],
  [<Code>--video-codec</Code>, "h264 (the default) or hevc for MP4, MOV and MKV."],
  [<Code>--audio-bitrate</Code>, "Audio bitrate in kbit/s."],
  [<Code>--no-audio</Code>, "Leave the sound out of video output."],
  [
    <Code>--background</Code>,
    "What transparent areas become: white, black, transparent or #rrggbb.",
  ],
  [<Code>-j, --jobs</Code>, "How many files at once. Videos always run one at a time."],
];

const comparison: [string, string, string][] = [
  [
    "FFmpeg",
    "Video and audio, with every codec and filter setting you could want",
    "convt runs FFmpeg for video and audio, with a short list of options instead of codec flags",
  ],
  [
    "ImageMagick",
    "Images, plus resizing, cropping, compositing and effects",
    "convt converts and resizes images; it doesn't edit them",
  ],
  [
    "LibreOffice (soffice)",
    "Office documents, converted from the command line in headless mode",
    "convt drives LibreOffice for documents, so the command is the same as for a photo",
  ],
  [
    "Pandoc",
    "Markup documents: Markdown, HTML, DOCX, LaTeX and more",
    "convt doesn't read Markdown or LaTeX; use Pandoc for those",
  ],
];

export const Route = createFileRoute("/_site/command-line-file-converter")({
  head: commandLineConverterHead,
  component: CommandLineFileConverter,
});

function CommandLineFileConverter() {
  return (
    <article className={cx(siteColumn, "pt-10 pb-20 md:pt-14")}>
      <p className="font-mono text-xs/4 text-ink-2 uppercase">The convt command</p>
      <header className="mt-3 flex max-w-[720px] flex-col gap-4">
        <h1 className="text-[34px]/10 font-semibold tracking-[-0.03em] text-balance md:text-[44px]/12">
          {page.heading}
        </h1>
        <div className="flex flex-col gap-4 text-[17px]/[27px] text-pretty text-ink-2">
          <p>
            <Code>convt</Code> is a command-line file converter for images, video, audio, PDFs and
            Office documents. Every conversion is the same command: the file or folder, then{" "}
            <Code>--to</Code> and the format. It picks the engine for you (FFmpeg, PDFium,
            LibreOffice, libheif or resvg) and runs on your computer. The <Code>convt</Code> command
            never uploads a file.
          </p>
          <p>
            It comes with the convt desktop app for macOS, Windows and Linux. A Desktop license is
            $39 once.
          </p>
        </div>
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
          <TextLink href={routes.formats}>See every format</TextLink>
        </div>
      </header>

      <div className="mt-12 grid gap-12 lg:grid-cols-[minmax(0,720px)_220px] lg:justify-between">
        <div className="flex min-w-0 flex-col gap-16">
          <GuideSection
            id="one-command"
            title="One command for images, video, audio and documents"
            source={<Checked how="tested">Run on convt 0.5.0, the current release.</Checked>}
          >
            <Prose>
              <p>
                You don't need to remember which tool handles which file, or its flags. These five
                lines cover a photo, a video, a recording, a Word file and a PDF:
              </p>
            </Prose>
            <Commands
              label="convt"
              commands={[
                "convt photo.png --to webp",
                "convt clip.mov --to mp4 --video-height 720",
                "convt memo.wav --to mp3",
                "convt report.docx --to pdf",
                "convt notes.pdf --to png --pages 1 --dpi 150",
              ]}
            />
            <Output label="Output, convt 0.5.0">
              {`photo.png -> ./photo.webp
clip.mov -> ./clip.mp4
memo.wav -> ./memo.mp3
report.docx -> ./report.pdf
notes.pdf -> ./notes.png`}
            </Output>
            <Prose>
              <p>
                Our 28.8 MB ProRes MOV came out as a 1.0 MB MP4 at 720p. The original file always
                stays as it was. To see what a file can become, ask convt:
              </p>
            </Prose>
            <Commands label="convt" commands={["convt targets clip.mov", "convt formats"]} />
            <Prose>
              <p>
                <Code>convt targets clip.mov</Code> listed MP4, WebM, MKV, AVI, GIF, the sound
                formats (MP3, M4A, WAV, FLAC, AAC, OGG, Opus) and still images. When convt can't go
                straight from one format to another, it chains up to three steps, such as SVG to PNG
                to WebP.
              </p>
            </Prose>
          </GuideSection>

          <GuideSection id="install" title="Install convt on Mac, Windows or Linux">
            <Prose>
              <p>
                <strong className="font-semibold">Mac</strong> (Apple silicon): Homebrew puts{" "}
                <Code>convt</Code> on your PATH. If you install the disk image from the{" "}
                <TextLink href={downloadUrl}>download page</TextLink> instead, the command is{" "}
                <Code>/Applications/convt.app/Contents/MacOS/convt</Code>.
              </p>
            </Prose>
            <Commands label="Homebrew" commands={homebrewCommands} />
            <Prose>
              <p>
                <strong className="font-semibold">Windows</strong> (x86_64): the installer doesn't
                add convt to your PATH, so in PowerShell type its full path:{" "}
                <Code>{String.raw`${convtExe} photo.png --to webp`}</Code>.
              </p>
              <p>
                <strong className="font-semibold">Linux</strong> (x86_64): a .deb, .rpm, AppImage
                and tarball are on{" "}
                <TextLink href={`${GITHUB_URL}/releases/latest`}>GitHub releases</TextLink>. The
                tarball has <Code>convt</Code> next to the FFmpeg and libraries it uses.
              </p>
              <p>
                Word, Excel and PowerPoint files need LibreOffice. On Windows,{" "}
                <Code>convt pack install documents</Code> downloads it once as convt's document
                pack. On a Mac, install LibreOffice from{" "}
                <TextLink href="https://www.libreoffice.org/download/download-libreoffice/">
                  libreoffice.org
                </TextLink>{" "}
                into Applications and convt finds it; on Linux, install it from your package
                manager. Run <Code>convt engines</Code> to see which engines are ready on your
                machine.
              </p>
            </Prose>
          </GuideSection>

          <GuideSection
            id="folders"
            title="Convert whole folders"
            source={<Checked how="tested">Run on convt 0.5.0, the current release.</Checked>}
          >
            <Prose>
              <p>
                Give convt a folder instead of a file. <Code>-r</Code> takes the subfolders too, and{" "}
                <Code>-o</Code> writes the results into a separate folder with the same layout:
              </p>
            </Prose>
            <Commands label="convt" commands={["convt Photos --to webp -r -o webp"]} />
            <Output label="Output, convt 0.5.0">
              {`Photos/a.png -> webp/a.webp
Photos/2026/b.png -> webp/2026/b.webp
2 converted, 0 failed`}
            </Output>
            <Prose>
              <p>
                convt converts several files at once, one per CPU core unless you set{" "}
                <Code>-j</Code>. Files already in the target format are skipped. It never
                overwrites: running the same command again wrote <Code>a (1).webp</Code> next to{" "}
                <Code>a.webp</Code>. A folder input converts every file that can become the target,
                so a folder of mixed files to <Code>--to jpg</Code> also turns videos and PDFs into
                images.
              </p>
            </Prose>
          </GuideSection>

          <GuideSection id="options" title="Options">
            <Prose>
              <p>
                Options that don't apply to a file are ignored for it, so one set of options can
                cover a folder of photos and videos. <Code>convt --help</Code> prints the same list:
              </p>
            </Prose>
            <FactList facts={options} />
          </GuideSection>

          <GuideSection
            id="presets"
            title="Save your settings as a preset"
            source={<Checked how="tested">Run on convt 0.5.0, the current release.</Checked>}
          >
            <Prose>
              <p>
                A preset is a small TOML file with a target format and options. Pass its path with{" "}
                <Code>-p</Code>, or save it in convt's presets folder and use its name.{" "}
                <Code>convt presets</Code> lists the saved ones.
              </p>
            </Prose>
            <Output label="web.toml">
              {`to = "webp"
quality = 80
max_size = 1200`}
            </Output>
            <Commands label="convt" commands={["convt photo.png -p web.toml -o web"]} />
            <Output label="Output, convt 0.5.0">{`photo.png -> web/photo.webp`}</Output>
          </GuideSection>

          <GuideSection
            id="scripts"
            title="Use it in scripts"
            source={<Checked how="tested">Run on convt 0.5.0, the current release.</Checked>}
          >
            <Prose>
              <p>
                <Code>--json</Code> prints one JSON event per line on stdout, so a script or another
                app can show progress and read the output paths:
              </p>
            </Prose>
            <Commands label="convt" commands={["convt logo.svg --to webp -o out --json"]} />
            <Output label="Output, convt 0.5.0">
              {`{"event":"started","index":0,"input":"logo.svg"}
{"event":"progress","index":0,"fraction":0.5}
{"event":"progress","index":0,"fraction":0.75}
{"event":"progress","index":0,"fraction":1.0}
{"event":"done","index":0,"input":"logo.svg","outputs":["out/logo.webp"]}
{"event":"summary","done":1,"failed":0,"cancelled":0}`}
            </Output>
            <Prose>
              <p>
                convt exits with 0 when every file converted and 1 when any failed, such as a file
                it can't read or a conversion it has no route for. <Code>convt formats --json</Code>{" "}
                lists every format with its extensions and MIME type. The{" "}
                <TextLink href={RAYCAST_URL}>Raycast extension</TextLink> runs the same command.
              </p>
            </Prose>
          </GuideSection>

          <GuideSection
            id="agents"
            title="Give it to an AI coding agent"
            source={<Checked how="tested">Run on convt 0.5.0, the current release.</Checked>}
          >
            <Prose>
              <p>
                <Code>convt --skill</Code> prints a SKILL.md: every command, option and format, and
                how to check which engines are installed. Save it where your agent reads skills, and
                the agent can convert files on your machine without uploading them anywhere.
              </p>
            </Prose>
            <Commands label="convt" commands={["convt --skill > SKILL.md"]} />
          </GuideSection>

          <GuideSection id="compare" title="convt, FFmpeg, ImageMagick, LibreOffice and Pandoc">
            <Prose>
              <p>
                The tools people usually reach for each cover one kind of file, and they go much
                deeper than convt does there. convt is the one command across all of them:
              </p>
            </Prose>
            <div className="overflow-x-auto rounded-2xl shadow-[inset_0_0_0_1px_var(--line)] [scrollbar-width:thin]">
              <table className="w-full min-w-[560px] border-collapse text-left text-[14px]/5">
                <thead className="bg-sunken text-ink-2">
                  <tr>
                    <th scope="col" className="px-4 py-3 font-medium">
                      Tool
                    </th>
                    <th scope="col" className="px-4 py-3 font-medium">
                      What it's best at
                    </th>
                    <th scope="col" className="px-4 py-3 font-medium">
                      Next to convt
                    </th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-divider">
                  {comparison.map(([tool, best, next]) => (
                    <tr key={tool}>
                      <th scope="row" className="px-4 py-3 font-medium">
                        {tool}
                      </th>
                      <td className="px-4 py-3 text-ink-2">{best}</td>
                      <td className="px-4 py-3">{next}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
            <Prose>
              <p>
                FFmpeg, ImageMagick, LibreOffice and Pandoc are free. If you already know their
                flags and only work with one kind of file, they may be all you need.
              </p>
            </Prose>
          </GuideSection>

          <GuideSection id="limits" title="What convt doesn't do yet">
            <Prose>
              <ul className="flex list-disc flex-col gap-2 pl-5">
                <li>
                  It can't shrink a file without changing its format: MP4 to a smaller MP4 or JPEG
                  to a smaller JPEG isn't possible in 0.5.0.
                </li>
                <li>
                  HEIC to JPEG drops the photo's EXIF data: the date taken, the camera and the
                  location.
                </li>
                <li>It doesn't turn PDFs into Word files, images into SVG, or SVG into PDF.</li>
                <li>It doesn't edit: no cropping, trimming, filters or merging files.</li>
                <li>The Mac build needs Apple silicon; there is no Intel Mac build.</li>
              </ul>
            </Prose>
          </GuideSection>

          <GuideSection id="guides" title="Step-by-step guides">
            <Prose>
              <p>
                Each guide does one job, built-in tools first, with every convt command tested. For
                Finder's right-click menu, see the{" "}
                <TextLink href={macFileConverter.path}>Mac file converter</TextLink> page.
              </p>
            </Prose>
            <ul className="flex flex-col divide-y divide-divider border-y border-line">
              {guides.map((guide) => (
                <li key={guide.slug}>
                  <a
                    href={guidePath(guide.slug)}
                    className={cx("group flex flex-col gap-1 rounded-sm py-4", focusRing)}
                  >
                    <span className="text-[16px]/6 font-semibold tracking-[-0.01em] group-hover:text-green">
                      {guide.heading}
                    </span>
                    <span className="text-[14px]/[22px] text-ink-2">{guide.description}</span>
                  </a>
                </li>
              ))}
            </ul>
          </GuideSection>

          <GuideSection id="faq" title="Questions">
            <dl className="flex flex-col gap-6">
              <Question q="Is convt free?">
                No. The source code is open (AGPL-3.0), but the signed app and command-line tool are
                $39 once for a Desktop license. The command-line tool starts a trial the first time
                it converts a file, so you can try it first.
              </Question>
              <Question q="Does the convt command upload my files?">
                No. It converts on your computer and makes no network calls, except{" "}
                <Code>convt pack install</Code> on Windows, which downloads the document pack when
                you ask for it. convt's paid cloud API is a separate service that does receive
                files.
              </Question>
              <Question q="Is it a wrapper around FFmpeg?">
                For video and audio, yes: it runs the FFmpeg that ships with it. Images, SVG, PDFs,
                HEIC and documents go through other engines, and convt picks the one that fits.
              </Question>
              <Question q="Does it run on a server?">
                The Linux build runs from a terminal on x86_64, so it works over SSH or in a script.
                The cloud API is the other option when files should be converted on convt's servers;
                see the <TextLink href="/docs">API docs</TextLink>.
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

function Question({ q, children }: { q: string; children: ReactNode }) {
  return (
    <div className="flex flex-col gap-1.5">
      <dt className="text-[16px]/6 font-semibold">{q}</dt>
      <dd className="text-[16px]/[26px] text-ink-2">{children}</dd>
    </div>
  );
}
