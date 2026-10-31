import { createFileRoute } from "@tanstack/react-router";

import { homebrewCommands } from "#/components/app/command-block";
import { TextLink } from "#/components/site/layout";
import {
  Checked,
  Code,
  Commands,
  FactList,
  GuideArticle,
  GuideCta,
  GuideSection,
  Output,
  Prose,
  Steps,
} from "#/components/site/guide";
import { guideBySlug, guideDownloadUrl, guideHead, guidePath } from "#/lib/guides";
import { GITHUB_URL, routes } from "#/lib/site";

// Every convt command and result on this page comes from a run of the released convt 0.5.0
// Linux CLI on PDFs we made: a 3-page US Letter report, copied into a nested folder
// (CNVM-9). convt renders PDFs with PDFium, which ships in the Mac and Windows installs too.
// The Preview and Snipping Tool steps come from Apple's guide and Microsoft's forum and say
// so on the page.

const slug = "pdf-to-png-mac-windows";
const guide = guideBySlug(slug);

const convtExe = String.raw`& "$env:LOCALAPPDATA\Programs\convt\convt.exe"`;

const sections = [
  { id: "preview", title: "Preview on a Mac" },
  { id: "windows", title: "Windows" },
  { id: "convt", title: "Every page with convt" },
  { id: "options", title: "Pages and resolution" },
  { id: "faq", title: "Questions" },
];

const results: [string, string][] = [
  [
    "Every page",
    "A 3-page PDF became report.png, report-2.png and report-3.png. The first page has no number.",
  ],
  ["Default size", "A US Letter page came out 2000 × 2588 pixels."],
  ["--dpi 300", "2550 × 3300 pixels, print resolution for Letter. --dpi 150 gave 1275 × 1650."],
  ["--max-size 1000", "773 × 1000: the longest side capped at 1,000 pixels."],
  [
    "Background",
    "White, as PDF viewers show it. --background transparent left it see-through in PNG.",
  ],
  ["Subfolders", "With -r, a PDF in 2026/ was converted into png/2026/."],
];

export const Route = createFileRoute("/_site/guides/pdf-to-png-mac-windows")({
  head: () => guideHead(slug),
  component: PdfToPngMacWindows,
});

function PdfToPngMacWindows() {
  return (
    <GuideArticle
      guide={guide}
      sections={sections}
      lead={
        <p>
          Upload forms, slides and chat apps often want an image, not a PDF. On a Mac, Preview saves
          a PDF as a PNG. Windows has no built-in export, so people take screenshots. To get every
          page as its own sharp PNG, or only the pages you pick, one convt command does it on both,
          without uploading the PDF anywhere.
        </p>
      }
    >
      <div className="flex flex-col gap-3 rounded-2xl bg-green-tint/60 px-5 py-4 shadow-[inset_0_0_0_1px_var(--green-line)]">
        <p className="font-mono text-[11px]/4 text-ink-2 uppercase">The short version</p>
        <p className="text-[16px]/[26px]">
          <strong className="font-semibold">Mac:</strong> open the PDF in Preview, choose File ›
          Export, set Format to PNG and click Save.{" "}
          <strong className="font-semibold">Every page, Mac or Windows:</strong>{" "}
          <Code>convt report.pdf --to png</Code> writes one PNG per page.
        </p>
      </div>

      <GuideSection
        id="preview"
        title="Save a PDF as PNG in Preview"
        source={
          <Checked how="docs">
            From Apple's guide,{" "}
            <TextLink href="https://support.apple.com/guide/preview/convert-image-file-types-prvw1012/mac">
              Convert image file types in Preview
            </TextLink>
            . We haven't run it ourselves.
          </Checked>
        }
      >
        <Steps>
          {[
            <>
              Open the PDF in Preview: right-click it and choose{" "}
              <strong className="font-semibold">Open With › Preview</strong>.
            </>,
            <>
              Choose <strong className="font-semibold">File › Export</strong>.
            </>,
            <>
              Open the <strong className="font-semibold">Format</strong> menu and choose PNG. To get
              a JPG instead, choose JPEG, which adds a quality slider.
            </>,
            <>Choose where to save it and click Save.</>,
          ]}
        </Steps>
        <Prose>
          <p>
            Apple's guide is written for images and doesn't say what Preview does with a PDF that
            has several pages, so check that you got the page you wanted. For every page as its own
            file, or a set resolution, use <TextLink href="#convt">convt</TextLink>.
          </p>
        </Prose>
      </GuideSection>

      <GuideSection
        id="windows"
        title="PDF to PNG on Windows"
        source={
          <Checked how="docs">
            From answers on{" "}
            <TextLink href="https://techcommunity.microsoft.com/discussions/windows11/look-for-a-simple-way-to-convert-pdf-to-image-pngjpggif-on-windows-pc/4155823">
              Microsoft's Windows 11 forum
            </TextLink>
            . We haven't run it on Windows.
          </Checked>
        }
      >
        <Prose>
          <p>
            Windows 11 can open a PDF in Edge but can't save its pages as images. The usual
            workaround is a screenshot of each page:
          </p>
        </Prose>
        <Steps>
          {[
            <>
              Open the PDF in Microsoft Edge and zoom until the whole page fits and reads clearly.
            </>,
            <>
              Press <strong className="font-semibold">Windows + Shift + S</strong> and drag over the
              page.
            </>,
            <>
              Open the capture in Snipping Tool, choose{" "}
              <strong className="font-semibold">Save as</strong> and pick PNG.
            </>,
          ]}
        </Steps>
        <Prose>
          <p>
            A screenshot is only as sharp as your screen, and it's one page at a time. For small
            print, many pages or a whole folder of PDFs, convert them with{" "}
            <TextLink href="#convt">convt</TextLink> instead.
          </p>
        </Prose>
      </GuideSection>

      <GuideSection
        id="convt"
        title="Convert every page of a PDF to PNG with convt"
        source={
          <Checked how="tested">
            Run with the released convt 0.5.0 command-line tool on Linux, on a 3-page PDF we made.
            convt renders PDFs with PDFium, which ships in the Mac and Windows installs too.
          </Checked>
        }
      >
        <Prose>
          <p>
            convt is a desktop app for Mac and Windows that converts files on your computer, with a
            command-line tool in the same install. Local conversions never upload your PDFs.
          </p>
          <p>
            Install it from the <TextLink href={guideDownloadUrl(slug)}>download page</TextLink> or{" "}
            <TextLink href={`${GITHUB_URL}/releases/latest`}>GitHub releases</TextLink>. On a Mac,
            Homebrew also puts the <Code>convt</Code> command in Terminal:
          </p>
        </Prose>
        <Commands label="Homebrew" commands={homebrewCommands} />

        <h3 className="mt-4 text-[19px]/7 font-semibold tracking-[-0.01em]">On a Mac</h3>
        <Prose>
          <p>
            In Terminal, go to the folder with the PDF and run this. If you installed from the disk
            image, type <Code>/Applications/convt.app/Contents/MacOS/convt</Code> instead of{" "}
            <Code>convt</Code>.
          </p>
        </Prose>
        <Commands label="convt" commands={["convt report.pdf --to png"]} />
        <Output label="Output">{`report.pdf -> report.png, report-2.png, report-3.png`}</Output>

        <h3 className="mt-4 text-[19px]/7 font-semibold tracking-[-0.01em]">On Windows</h3>
        <Prose>
          <p>The installer doesn't add convt to your PATH, so PowerShell needs its full path:</p>
        </Prose>
        <Commands label="PowerShell" commands={[String.raw`${convtExe} report.pdf --to png`]} />

        <h3 className="mt-4 text-[19px]/7 font-semibold tracking-[-0.01em]">A folder of PDFs</h3>
        <Prose>
          <p>
            Give convt a folder instead of a file. <Code>-r</Code> includes subfolders and{" "}
            <Code>-o png</Code> writes the images into a new folder with the same subfolders. A
            folder input converts every file convt can turn into PNG, so a JPG in it gets a PNG copy
            too. Files that are already PNG are skipped.
          </p>
        </Prose>
        <Commands label="convt" commands={["convt docs --to png -r -o png"]} />
        <Output label="Output">{`docs/report.pdf -> png/report.png, png/report-2.png, png/report-3.png
docs/2026/invoice.pdf -> png/2026/invoice.png, png/2026/invoice-2.png, png/2026/invoice-3.png
2 converted, 0 failed`}</Output>
        <Prose>
          <p>
            Use <Code>--to jpg</Code> instead of <Code>--to png</Code> for scans and pages full of
            photos, where JPG is usually smaller. For text and line art, PNG stays crisp and was the
            smaller file in our test.
          </p>
        </Prose>
      </GuideSection>

      <GuideSection id="options" title="Pick the pages and the resolution">
        <FactList
          facts={[
            [
              <Code>--pages 2</Code>,
              "Only page 2. Ranges work too: --pages 1-3, or --pages 4- for page 4 to the end.",
            ],
            [
              <Code>--dpi 300</Code>,
              "Render at 300 dots per inch, for printing or zooming in. 150 is plenty for a screen.",
            ],
            [
              <Code>--max-size 1200</Code>,
              "Limits the longest side of each image to 1,200 pixels.",
            ],
            [
              <Code>--background transparent</Code>,
              "Keeps the page background see-through in PNG. The default is white.",
            ],
          ]}
        />
        <Commands label="convt" commands={["convt report.pdf --to png --pages 2 --dpi 300"]} />
        <Output label="Output">{`report.pdf -> report-2.png`}</Output>

        <h3 className="mt-4 text-[19px]/7 font-semibold tracking-[-0.01em]">
          What came out of our test
        </h3>
        <FactList facts={results} />
        <Prose>
          <p>
            Asking for a page the PDF doesn't have stops with a clear message instead of writing
            nothing: <Code>the document has 3 pages, so pages 5 select nothing</Code>.
          </p>
        </Prose>

        <GuideCta slug={slug}>
          <p className="text-[16px]/6 font-semibold text-ink">
            convt for Macs with Apple silicon, and for Windows
          </p>
          <p>
            A Desktop license is $39, paid once, with every future update. Local conversions never
            leave your computer.
          </p>
        </GuideCta>
      </GuideSection>

      <GuideSection id="faq" title="Questions">
        <dl className="flex flex-col gap-6">
          <div className="flex flex-col gap-1.5">
            <dt className="text-[16px]/6 font-semibold">What resolution should I use?</dt>
            <dd className="text-[16px]/[26px] text-ink-2">
              For a screen, a slide or an upload form, the default is enough: a Letter page came out
              2,000 pixels wide. Use <Code>--dpi 300</Code> for printing or for small text you need
              to zoom into.
            </dd>
          </div>
          <div className="flex flex-col gap-1.5">
            <dt className="text-[16px]/6 font-semibold">Can I convert a PDF to JPG instead?</dt>
            <dd className="text-[16px]/[26px] text-ink-2">
              Yes. Choose JPEG in Preview, or use <Code>--to jpg</Code> with convt. The page and
              resolution options work the same way.
            </dd>
          </div>
          <div className="flex flex-col gap-1.5">
            <dt className="text-[16px]/6 font-semibold">Does convt upload my PDF?</dt>
            <dd className="text-[16px]/[26px] text-ink-2">
              Not for local conversions. The app and the command-line tool convert on your computer.
              The app uploads a file only if you pick Cloud for a job, which is part of convt Pro.
              For other images, see{" "}
              <TextLink href={guidePath("webp-to-png-jpg")}>WebP to PNG or JPG</TextLink>, or every
              format convt handles on the <TextLink href={routes.formats}>formats page</TextLink>.
            </dd>
          </div>
        </dl>
      </GuideSection>
    </GuideArticle>
  );
}
