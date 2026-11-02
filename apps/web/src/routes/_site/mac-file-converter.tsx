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
  macFileConverter,
  macFileConverterDownloadUrl,
  macFileConverterHead,
} from "#/lib/landing-pages";
import { RAYCAST_URL, routes } from "#/lib/site";

// Every convt command and result on this page comes from a run of the released convt 0.5.0
// Linux CLI (CNVM-14) on files we made with FFmpeg: a 3-second 1080p ProRes MOV with a 48 kHz
// tone, a WAV, PNG photos in a folder with a subfolder, and a text file turned into DOCX. The
// Mac app runs the same engines. The DOCX to PDF run used a system LibreOffice, which is what
// the document pack installs. The Finder, Preview, QuickTime and Music steps come from Apple's
// guides and say so on the page.

const page = macFileConverter;
const downloadUrl = macFileConverterDownloadUrl;

const sections = [
  { id: "built-in", title: "What macOS converts already" },
  { id: "finder", title: "Right-click in Finder" },
  { id: "formats", title: "Video, audio, images, documents" },
  { id: "folders", title: "Folders and Terminal" },
  { id: "limits", title: "What it doesn't do" },
  { id: "price", title: "Price and install" },
  { id: "guides", title: "Step-by-step guides" },
  { id: "faq", title: "Questions" },
];

const comparison: [string, string, string][] = [
  [
    "HEIC photo to JPEG or PNG",
    "Finder Quick Action or Preview",
    "Finder menu or Terminal, whole folders",
  ],
  [
    "WebP, AVIF or SVG to PNG or JPEG",
    "Preview exports seven formats, for the images you open",
    "Any of them, to a dozen image formats",
  ],
  ["MOV to MP4", "QuickTime exports MOV, not MP4", "MP4, WebM, MKV, AVI or GIF, with a size cap"],
  [
    "MP4, WAV or FLAC to MP3",
    "QuickTime then Music; no FLAC",
    "One step, any of the three, set bitrate",
  ],
  ["PDF pages to PNG", "Preview, one page at a time", "Every page or a range, at the DPI you pick"],
  [
    "Word, Excel, PowerPoint to PDF",
    "Open each file and print or export",
    "One command per file or folder",
  ],
  [
    "A folder with subfolders",
    "Music imports a folder of songs; others take open files",
    "-r converts every subfolder in one go",
  ],
];

const formatFacts: [ReactNode, ReactNode][] = [
  [
    "Video",
    <>
      MP4, MOV, WebM, MKV and AVI to each other, plus GIF and the sound track alone (MP3, M4A, WAV).
      Cap the height with <Code>--video-height</Code>, pick H.264 or HEVC with{" "}
      <Code>--video-codec</Code>.
    </>,
  ],
  [
    "Audio",
    <>
      MP3, WAV, FLAC, AAC, M4A, OGG and Opus. Set the bitrate with <Code>--audio-bitrate</Code>.
    </>,
  ],
  [
    "Images",
    <>
      HEIC, JPEG, PNG, WebP, AVIF, GIF, TIFF, BMP, ICO, TGA, PPM, QOI and OpenEXR, and SVG to any of
      them. Shrink with <Code>--max-size</Code>, set quality with <Code>-q</Code>.
    </>,
  ],
  [
    "PDF",
    <>
      Pages to PNG, JPEG or another image format (<Code>--pages</Code>, <Code>--dpi</Code>).
    </>,
  ],
  [
    "Documents",
    <>
      DOCX, DOC, ODT, RTF, TXT and HTML; XLSX, XLS, ODS and CSV; PPTX, PPT and ODP. Each converts to
      PDF and to the other formats of its kind.
    </>,
  ],
];

export const Route = createFileRoute("/_site/mac-file-converter")({
  head: macFileConverterHead,
  component: MacFileConverter,
});

function MacFileConverter() {
  return (
    <article className={cx(siteColumn, "pt-10 pb-20 md:pt-14")}>
      <p className="font-mono text-xs/4 text-ink-2 uppercase">convt for Mac</p>
      <header className="mt-3 flex max-w-[720px] flex-col gap-4">
        <h1 className="text-[34px]/10 font-semibold tracking-[-0.03em] text-balance md:text-[44px]/12">
          {page.heading}
        </h1>
        <div className="flex flex-col gap-4 text-[17px]/[27px] text-pretty text-ink-2">
          <p>
            convt converts video, audio, images, PDFs and Office documents on your Mac. Right-click
            a file in Finder, pick a format, and the new file appears next to the original. The same
            install gives you the <Code>convt</Code> command for folders and scripts. Local
            conversions never leave your computer.
          </p>
          <p>
            It runs on Macs with Apple silicon and macOS 13 or later, and costs $39 once. It's also
            on Windows and Linux.
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
            Get convt for Mac
          </a>
          <TextLink href={routes.formats}>See every format</TextLink>
        </div>
      </header>

      <div className="mt-12 grid gap-12 lg:grid-cols-[minmax(0,720px)_220px] lg:justify-between">
        <div className="flex min-w-0 flex-col gap-16">
          <GuideSection
            id="built-in"
            title="What your Mac converts without extra apps"
            source={
              <Checked how="docs">
                From Apple's guides{" "}
                <TextLink href="https://support.apple.com/guide/mac-help/perform-quick-actions-in-the-finder-on-mac-mchl97ff9142/mac">
                  Perform quick actions in the Finder
                </TextLink>
                ,{" "}
                <TextLink href="https://support.apple.com/guide/preview/convert-image-file-types-prvw1012/mac">
                  Convert image file types using Preview
                </TextLink>
                ,{" "}
                <TextLink href="https://support.apple.com/guide/quicktime-player/export-movies-qtp20e395859/mac">
                  Export movies in QuickTime Player
                </TextLink>{" "}
                and{" "}
                <TextLink href="https://support.apple.com/en-us/108961">
                  Convert a song to a different file format
                </TextLink>
                .
              </Checked>
            }
          >
            <Prose>
              <p>Try these first. For one file, they're often all you need:</p>
              <ul className="flex list-disc flex-col gap-2 pl-5">
                <li>
                  <strong className="font-semibold">Finder:</strong> the Convert Image quick action
                  turns a photo into JPEG or PNG, at a smaller size if you like. Create PDF makes a
                  PDF from images.
                </li>
                <li>
                  <strong className="font-semibold">Preview:</strong> File › Export saves an image
                  or PDF as HEIC, JPEG, JPEG-2000, OpenEXR, PDF, PNG or TIFF.
                </li>
                <li>
                  <strong className="font-semibold">QuickTime Player:</strong> File › Export As
                  saves a video at 4K, 1080p, 720p or 480p, or just its sound as an M4A file. The
                  video stays a MOV.
                </li>
                <li>
                  <strong className="font-semibold">Music:</strong> makes MP3, AAC, AIFF, WAV or
                  Apple Lossless copies of songs in your library. It doesn't open FLAC.
                </li>
              </ul>
              <p>
                Each app covers one kind of file, and only Music imports a whole folder. convt fills
                those gaps:
              </p>
            </Prose>
            <div className="overflow-x-auto rounded-2xl shadow-[inset_0_0_0_1px_var(--line)] [scrollbar-width:thin]">
              <table className="w-full min-w-[560px] border-collapse text-left text-[14px]/5">
                <thead className="bg-sunken text-ink-2">
                  <tr>
                    <th scope="col" className="px-4 py-3 font-medium">
                      Job
                    </th>
                    <th scope="col" className="px-4 py-3 font-medium">
                      Built into macOS
                    </th>
                    <th scope="col" className="px-4 py-3 font-medium">
                      convt
                    </th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-divider">
                  {comparison.map(([job, mac, convt]) => (
                    <tr key={job}>
                      <th scope="row" className="px-4 py-3 font-medium">
                        {job}
                      </th>
                      <td className="px-4 py-3 text-ink-2">{mac}</td>
                      <td className="px-4 py-3">{convt}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </GuideSection>

          <GuideSection
            id="finder"
            title="Convert files with a right-click in Finder"
            source={
              <Checked how="docs">
                How the convt Finder menu works, from its source. We haven't recorded it on a Mac
                for this page.
              </Checked>
            }
          >
            <Prose>
              <p>
                convt adds a <strong className="font-semibold">Convert with convt</strong> menu to
                Finder. Select one file or several, Control-click, and the menu lists the formats
                they can all become, grouped by kind. Pick one and convt writes the new file next to
                the original without opening a window.{" "}
                <strong className="font-semibold">More options…</strong> opens convt's Quick convert
                window for quality, size and the output folder.
              </p>
              <p>
                macOS asks you to turn on Finder extensions yourself. The first time you open convt,
                it shows a step that opens System Settings at the right place and tells you when the
                menu is on. To change which formats the menu offers, use Settings › Menu in the app,
                or <Code>convt menu</Code> in Terminal.
              </p>
              <p>
                If you use <TextLink href={RAYCAST_URL}>Raycast</TextLink>, its convt extension
                converts the files selected in Finder too.
              </p>
            </Prose>
          </GuideSection>

          <GuideSection
            id="formats"
            title="What convt converts on a Mac"
            source={<Checked how="tested">Run on convt 0.5.0, the current release.</Checked>}
          >
            <Prose>
              <p>
                One app handles each of these, so you don't need a separate video converter, audio
                converter and image converter. The full list, with what each format turns into, is
                on the <TextLink href={routes.formats}>formats page</TextLink>.
              </p>
            </Prose>
            <FactList facts={formatFacts} />
            <Prose>
              <p>
                In Terminal, it's one pattern for every kind of file: the file, then{" "}
                <Code>--to</Code> and the format.
              </p>
            </Prose>
            <Commands
              label="Terminal"
              commands={[
                "convt clip.mov --to mp4",
                "convt clip.mov --to mp4 --video-height 720 -o small",
                "convt song.wav --to mp3",
                "convt photo.heic --to jpg",
                "convt report.docx --to pdf",
              ]}
            />
            <Output label="Output, convt 0.5.0">
              {`clip.mov -> ./clip.mp4
clip.mov -> small/clip.mp4
song.wav -> ./song.mp3
photo.heic -> ./photo.jpg
report.docx -> ./report.pdf`}
            </Output>
            <Prose>
              <p>
                Our 3-second 1080p ProRes MOV was 28.8 MB. As an MP4 it came out at 3.1 MB, and at
                1.0 MB with <Code>--video-height 720</Code>. To see what a file can become before
                you convert it, run <Code>convt targets clip.mov</Code>.
              </p>
            </Prose>
          </GuideSection>

          <GuideSection
            id="folders"
            title="Convert whole folders from Terminal"
            source={<Checked how="tested">Run on convt 0.5.0, the current release.</Checked>}
          >
            <Prose>
              <p>
                Give convt a folder instead of a file. <Code>-r</Code> takes the subfolders too, and{" "}
                <Code>-o</Code> puts the results in a separate folder with the same layout, so the
                originals stay where they are:
              </p>
            </Prose>
            <Commands label="Terminal" commands={["convt Photos --to webp -r -o webp"]} />
            <Output label="Output, convt 0.5.0">
              {`Photos/a.png -> webp/a.webp
Photos/2026/b.png -> webp/2026/b.webp
2 converted, 0 failed`}
            </Output>
            <Prose>
              <p>
                convt converts several files at once (<Code>-j</Code> sets how many) and runs videos
                one at a time. Files already in the target format are skipped. Add{" "}
                <Code>--json</Code> to get one progress event per line for a script, and run{" "}
                <Code>convt --help</Code> for every option.
              </p>
              <p>
                Homebrew puts <Code>convt</Code> in Terminal for you. If you installed from the disk
                image, type <Code>/Applications/convt.app/Contents/MacOS/convt</Code> instead of{" "}
                <Code>convt</Code>.
              </p>
            </Prose>
          </GuideSection>

          <GuideSection id="limits" title="What convt doesn't do yet">
            <Prose>
              <ul className="flex list-disc flex-col gap-2 pl-5">
                <li>
                  It needs a Mac with Apple silicon (M1 or later). There is no build for Intel Macs.
                </li>
                <li>
                  It can't shrink a file without changing its format. MP4 to smaller MP4 or JPEG to
                  smaller JPEG isn't possible in 0.5.0, so for that use HandBrake or Preview.
                </li>
                <li>
                  HEIC to JPEG drops the photo's EXIF data: the date taken, the camera and the
                  location.
                </li>
                <li>An animated WebP becomes one still image, the first frame.</li>
                <li>
                  Word, Excel and PowerPoint files need the document pack, a one-time download the
                  app offers during setup (<Code>convt pack install documents</Code> in Terminal).
                </li>
              </ul>
            </Prose>
          </GuideSection>

          <GuideSection id="price" title="Price and install">
            <Prose>
              <p>
                A Desktop license is <strong className="font-semibold">$39, paid once</strong>, with
                every future desktop update. There's no subscription. Desktop + Cloud is $69 once
                and adds a year of cloud conversions that doesn't renew. Cloud jobs upload the file
                to convt's servers, and the app only does that when you pick Cloud for a job.
              </p>
              <p>
                Get the disk image from the <TextLink href={downloadUrl}>download page</TextLink>{" "}
                (you'll sign in first), or install with Homebrew:
              </p>
            </Prose>
            <Commands label="Homebrew" commands={homebrewCommands} />
            <aside className="flex flex-col gap-5 rounded-2xl bg-panel p-6 shadow-[inset_0_0_0_1px_var(--line)] sm:flex-row sm:items-center sm:justify-between sm:gap-8">
              <div className="flex flex-col gap-1.5 text-[14px]/[22px] text-ink-2">
                <p className="text-[16px]/6 font-semibold text-ink">
                  convt for Macs with Apple silicon
                </p>
                <p>
                  Video, audio, images and documents in one app, from Finder or Terminal. Local
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

          <GuideSection id="guides" title="Step-by-step guides">
            <Prose>
              <p>
                Each guide does one job on a Mac, built-in tools first, with every command tested:
              </p>
            </Prose>
            <GuideList items={guides} />
          </GuideSection>

          <GuideSection id="faq" title="Questions">
            <dl className="flex flex-col gap-6">
              <Question q="Is convt free?">
                No. The source code is open (AGPL-3.0), but the signed Mac app is $39 once. The
                built-in tools at the top of this page cost nothing, and for one photo or one video
                they're often enough.
              </Question>
              <Question q="Does it upload my files?">
                Not for local conversions, which is everything on this page. The only uploads are
                cloud jobs, which you pick per job and which need Desktop + Cloud.
              </Question>
              <Question q="How is it different from Permute or HandBrake?">
                HandBrake is a free video encoder with many settings. It makes video files only, not
                images, audio files or documents. Permute is a paid Mac app that converts video,
                audio, images and PDFs. convt also converts Office documents, has a command-line
                tool for folders and scripts, and runs on Windows and Linux too.
              </Question>
              <Question q="Does it work on Windows?">
                Yes, the app and the <Code>convt</Code> command run on Windows. The Explorer
                right-click menu isn't in the Windows installer yet.
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

function GuideList({ items }: { items: typeof guides }) {
  return (
    <ul className="flex flex-col divide-y divide-divider border-y border-line">
      {items.map((guide) => (
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
