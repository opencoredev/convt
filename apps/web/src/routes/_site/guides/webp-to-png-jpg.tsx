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
// Linux CLI on WebP files we made: a transparent 1600 × 1000 graphic, a 2400 × 1600 photo
// and a 5-frame animation, in nested folders (CNVM-7). The image engine is the same code on
// Mac and Windows. The Preview and Paint steps come from Apple's guide and Microsoft's forum
// and say so on the page.

const slug = "webp-to-png-jpg";
const guide = guideBySlug(slug);

const convtExe = String.raw`& "$env:LOCALAPPDATA\Programs\convt\convt.exe"`;

const sections = [
  { id: "which", title: "PNG or JPG?" },
  { id: "preview", title: "Preview on a Mac" },
  { id: "paint", title: "Paint on Windows" },
  { id: "convt", title: "Whole folders with convt" },
  { id: "faq", title: "Questions" },
];

const results: [string, string][] = [
  ["Size", "1600 × 1000 and 2400 × 1600 in, the same out. Nothing cropped or scaled."],
  ["Transparency in PNG", "Kept. Transparent pixels stayed fully transparent."],
  [
    "Transparency in JPG",
    "Filled with white, because JPG has no transparency. --background black filled it with black.",
  ],
  ["Subfolders", "With -r, an image in sub/ was converted into png/sub/."],
  ["Animated WebP", "Only the first frame was written, to PNG and to GIF alike."],
];

export const Route = createFileRoute("/_site/guides/webp-to-png-jpg")({
  head: () => guideHead(slug),
  component: WebpToPngJpg,
});

function WebpToPngJpg() {
  return (
    <GuideArticle
      guide={guide}
      sections={sections}
      lead={
        <p>
          Websites serve WebP because it's small, so a saved image often arrives as a .webp that
          older apps and upload forms won't take. To save one WebP as a PNG or JPG, the apps already
          on your computer do it: Preview on a Mac, Paint on Windows. For a folder of them, one
          convt command converts everything.
        </p>
      }
    >
      <div className="flex flex-col gap-3 rounded-2xl bg-green-tint/60 px-5 py-4 shadow-[inset_0_0_0_1px_var(--green-line)]">
        <p className="font-mono text-[11px]/4 text-ink-2 uppercase">The short version</p>
        <p className="text-[16px]/[26px]">
          <strong className="font-semibold">Mac:</strong> open the WebP in Preview, choose File ›
          Export, set Format to PNG or JPEG and click Save.{" "}
          <strong className="font-semibold">Windows:</strong> open it in Paint, choose File › Save
          as and pick PNG picture or JPEG picture.
        </p>
      </div>

      <GuideSection id="which" title="PNG or JPG?">
        <FactList
          facts={[
            [
              "PNG",
              "Logos, screenshots, graphics with text, anything with a transparent background. Lossless, so the file is often bigger than the WebP.",
            ],
            [
              "JPG",
              "Photos. Smaller than PNG and accepted everywhere, but it has no transparency: see-through areas get a solid color.",
            ],
          ]}
        />
      </GuideSection>

      <GuideSection
        id="preview"
        title="Save a WebP as PNG or JPG in Preview"
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
            <>Double-click the WebP image. It opens in Preview.</>,
            <>
              Choose <strong className="font-semibold">File › Export</strong>.
            </>,
            <>
              Open the <strong className="font-semibold">Format</strong> menu and choose PNG or
              JPEG. JPEG shows a quality slider.
            </>,
            <>Choose where to save it and click Save.</>,
          ]}
        </Steps>
        <Prose>
          <p>
            Apple's guide says you can convert several images at once: open them in one Preview
            window, select them all in the sidebar, then export. Preview doesn't look inside
            folders, so for a downloads folder full of WebPs, use{" "}
            <TextLink href="#convt">convt</TextLink>.
          </p>
        </Prose>
      </GuideSection>

      <GuideSection
        id="paint"
        title="Save a WebP as PNG or JPG in Paint"
        source={
          <Checked how="docs">
            From answers on{" "}
            <TextLink href="https://techcommunity.microsoft.com/t5/windows-11/how-do-i-convert-webp-to-png-on-windows-11/m-p/4249658">
              Microsoft's Windows 11 forum
            </TextLink>
            . We haven't run it on Windows.
          </Checked>
        }
      >
        <Steps>
          {[
            <>
              Right-click the WebP and choose{" "}
              <strong className="font-semibold">Open with › Paint</strong>.
            </>,
            <>
              Choose <strong className="font-semibold">File › Save as</strong>.
            </>,
            <>Pick PNG picture or JPEG picture, choose a folder and click Save.</>,
          ]}
        </Steps>
        <Prose>
          <p>
            Paint converts one picture at a time. For more than a few, it's quicker to convert the
            folder in one go with convt.
          </p>
        </Prose>
      </GuideSection>

      <GuideSection
        id="convt"
        title="Convert a folder of WebP images with convt"
        source={
          <Checked how="tested">
            Run with the released convt 0.5.0 command-line tool on Linux, on WebP files we made in
            nested folders. The image conversion is the same code on Mac and Windows.
          </Checked>
        }
      >
        <Prose>
          <p>
            convt is a desktop app for Mac and Windows that converts files on your computer, with a
            command-line tool in the same install. Local conversions never upload your images.
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
            In Terminal, go to the folder that holds your images folder and run one of these. If you
            installed from the disk image, type{" "}
            <Code>/Applications/convt.app/Contents/MacOS/convt</Code> instead of <Code>convt</Code>.
          </p>
        </Prose>
        <Commands
          label="convt"
          commands={["convt images --to png -r -o png", "convt images --to jpg -r -o jpg"]}
        />
        <Output label="Output">{`images/sub/logo.webp -> png/sub/logo.png
images/photo.webp -> png/photo.png
2 converted, 0 failed`}</Output>

        <h3 className="mt-4 text-[19px]/7 font-semibold tracking-[-0.01em]">On Windows</h3>
        <Prose>
          <p>The installer doesn't add convt to your PATH, so PowerShell needs its full path:</p>
        </Prose>
        <Commands
          label="PowerShell"
          commands={[String.raw`${convtExe} images --to png -r -o png`]}
        />

        <Prose>
          <p>
            <Code>-r</Code> includes subfolders and <Code>-o png</Code> writes the copies into a new
            folder with the same subfolders. Leave out <Code>-o</Code> and each copy is written next
            to its WebP. A folder input converts every image convt can turn into the target, so
            other images in it get copies too; with <Code>-o</Code> they land in the output folder,
            and the originals stay as they are.
          </p>
        </Prose>
        <FactList
          facts={[
            [<Code>-q 80</Code>, "JPEG quality from 1 to 100. Without it, convt uses 90."],
            [<Code>--max-size 1200</Code>, "Limits the longest side to 1,200 pixels."],
            [
              <Code>--background black</Code>,
              "What transparent areas become in a JPG: white by default, black, or a color like #ff8800.",
            ],
          ]}
        />

        <h3 className="mt-4 text-[19px]/7 font-semibold tracking-[-0.01em]">
          What came out of our test
        </h3>
        <FactList facts={results} />
        <Prose>
          <p>
            For an animated WebP, convt 0.5.0 keeps the first frame only, even when the target is
            GIF. To keep the animation, use a tool that writes animated GIFs.
          </p>
        </Prose>

        <GuideCta slug={slug}>
          <p className="text-[16px]/6 font-semibold text-ink">
            convt for Macs with Apple silicon, and for Windows
          </p>
          <p>
            Start a 7-day convt Pro trial from your convt account, then buy a $29 Desktop license,
            paid once. Local conversions never leave your computer.
          </p>
        </GuideCta>
      </GuideSection>

      <GuideSection id="faq" title="Questions">
        <dl className="flex flex-col gap-6">
          <div className="flex flex-col gap-1.5">
            <dt className="text-[16px]/6 font-semibold">Can I just rename .webp to .png?</dt>
            <dd className="text-[16px]/[26px] text-ink-2">
              No. Renaming changes the name, not the data, so the file is still WebP inside and many
              apps will refuse it. It has to be decoded and saved again.
            </dd>
          </div>
          <div className="flex flex-col gap-1.5">
            <dt className="text-[16px]/6 font-semibold">
              Does converting WebP to PNG lose quality?
            </dt>
            <dd className="text-[16px]/[26px] text-ink-2">
              No. PNG is lossless, so the PNG holds exactly the pixels the WebP decodes to. It can't
              undo compression the WebP already had. Converting to JPG compresses the image again,
              which loses a little.
            </dd>
          </div>
          <div className="flex flex-col gap-1.5">
            <dt className="text-[16px]/6 font-semibold">Why is the PNG so much bigger?</dt>
            <dd className="text-[16px]/[26px] text-ink-2">
              WebP is usually lossy and PNG is lossless, so the same picture takes more space. In
              our test a 16 KB WebP graphic became a 135 KB PNG. For photos, JPG is the smaller
              choice.
            </dd>
          </div>
          <div className="flex flex-col gap-1.5">
            <dt className="text-[16px]/6 font-semibold">Does convt upload my images?</dt>
            <dd className="text-[16px]/[26px] text-ink-2">
              Not for local conversions. The app and the command-line tool convert on your computer.
              The app uploads a file only if you pick Cloud for a job, which is part of convt Pro.
              For iPhone photos, see{" "}
              <TextLink href={guidePath("heic-to-jpg-mac")}>HEIC to JPG on a Mac</TextLink>, or
              every format convt handles on the{" "}
              <TextLink href={routes.formats}>formats page</TextLink>.
            </dd>
          </div>
        </dl>
      </GuideSection>
    </GuideArticle>
  );
}
