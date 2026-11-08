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
} from "#/components/site/guide";
import { guideBySlug, guideHead, guidePath, guideDownloadUrl } from "#/lib/guides";
import { commandLineConverter } from "#/lib/landing-pages";
import { GITHUB_URL, routes } from "#/lib/site";

// Every convt command and result on this page comes from a run of the released convt 0.5.0
// Linux CLI (CNVM-16) on SVGs we wrote by hand: a 120 × 80 logo with a circle and a line of
// text, and 24 × 24 icons in a folder with a subfolder (one named .SVG). convt renders SVG with
// resvg, the same engine in the Mac and Windows installs. The qlmanage steps come from its
// manual page and say so on the page; we didn't run them on a Mac.

const slug = "svg-to-png-mac";
const guide = guideBySlug(slug);

const convtExe = String.raw`& "$env:LOCALAPPDATA\Programs\convt\convt.exe"`;

const sections = [
  { id: "built-in", title: "With Quick Look in Terminal" },
  { id: "convt", title: "One SVG with convt" },
  { id: "size", title: "Pick the size" },
  { id: "background", title: "Transparent or solid" },
  { id: "folder", title: "A folder of SVGs" },
  { id: "faq", title: "Questions" },
];

const results: [string, string][] = [
  [
    "Size",
    "Without options, the PNG has the width and height the SVG sets: our 120 × 80 logo became a 120 × 80 PNG.",
  ],
  ["Transparency", "Kept. Areas the SVG doesn't paint stay transparent in the PNG."],
  ["Subfolders", "With -r and -o, icons/sub/b.svg became png/sub/b.png."],
  ["Capital letters", "A file named c.SVG converted like the others."],
  ["Existing PNGs", "A PNG already in the folder was skipped, not converted again."],
  [
    "Name clashes",
    "Running the same command twice wrote logo (1).png next to logo.png. It doesn't overwrite the first one.",
  ],
];

export const Route = createFileRoute("/_site/guides/svg-to-png-mac")({
  head: () => guideHead(slug),
  component: SvgToPngMac,
});

function SvgToPngMac() {
  return (
    <GuideArticle
      guide={guide}
      sections={sections}
      lead={
        <p>
          Slides, documents, app stores and a lot of upload forms want PNG, not SVG. macOS can make
          one from Terminal with Quick Look, the same thing that previews files in Finder. convt
          converts one SVG or a whole folder of them at the size you pick, keeps transparency, and
          does it on your Mac without uploading anything.
        </p>
      }
    >
      <div className="flex flex-col gap-3 rounded-2xl bg-green-tint/60 px-5 py-4 shadow-[inset_0_0_0_1px_var(--green-line)]">
        <p className="font-mono text-[11px]/4 text-ink-2 uppercase">The short version</p>
        <p className="text-[16px]/[26px]">
          <strong className="font-semibold">Built in:</strong>{" "}
          <Code>qlmanage -t -s 1000 -o . logo.svg</Code> in Terminal.{" "}
          <strong className="font-semibold">At a set size, or a whole folder:</strong>{" "}
          <Code>convt icons --to png -r -o png --dpi 192</Code>.
        </p>
      </div>

      <GuideSection
        id="built-in"
        title="Convert SVG to PNG with Quick Look in Terminal"
        source={
          <Checked how="docs">
            From the qlmanage manual page (type <Code>man qlmanage</Code> in Terminal). We haven't
            run it on a Mac ourselves.
          </Checked>
        }
      >
        <Prose>
          <p>
            macOS ships <Code>qlmanage</Code>, a Terminal tool for Quick Look, the preview you get
            when you press Space on a file in Finder. Its <Code>-t</Code> option saves a thumbnail
            as a PNG file. Open Terminal, go to the folder with the SVG and run:
          </p>
        </Prose>
        <Commands label="Terminal" commands={["qlmanage -t -s 1000 -o . logo.svg"]} />
        <Prose>
          <p>
            <Code>-s 1000</Code> asks for a 1000-pixel thumbnail and <Code>-o .</Code> saves it in
            the current folder. The file is named <Code>logo.svg.png</Code>, so rename it if the
            extra <Code>.svg</Code> gets in the way.
          </p>
          <p>
            It works for a quick copy, but it's a thumbnail tool: the manual describes it as the
            "Quick Look Server debug and management tool". It makes one thumbnail per file you name,
            and the manual lists no option for the background or for subfolders. For those, use
            convt.
          </p>
        </Prose>
      </GuideSection>

      <GuideSection
        id="convt"
        title="Convert an SVG to PNG with convt"
        source={
          <Checked how="tested">
            Run with the released convt 0.5.0 command-line tool on Linux, on SVGs we made. convt
            renders SVG with resvg, the same engine in the Mac and Windows installs.
          </Checked>
        }
      >
        <Prose>
          <p>
            convt is a desktop app for Mac and Windows that converts files on your computer, with a
            command-line tool in the same install. Local conversions never upload your files.
          </p>
          <p>
            Install it from the <TextLink href={guideDownloadUrl(slug)}>download page</TextLink> or{" "}
            <TextLink href={`${GITHUB_URL}/releases/latest`}>GitHub releases</TextLink>. Homebrew
            also puts the <Code>convt</Code> command in Terminal:
          </p>
        </Prose>
        <Commands label="Homebrew" commands={homebrewCommands} />
        <Prose>
          <p>
            In Terminal, go to the folder with the SVG and run this. If you installed from the disk
            image, type <Code>/Applications/convt.app/Contents/MacOS/convt</Code> instead of{" "}
            <Code>convt</Code>.
          </p>
        </Prose>
        <Commands label="convt" commands={["convt logo.svg --to png"]} />
        <Output label="Output">{`logo.svg -> ./logo.png`}</Output>
        <Prose>
          <p>
            The PNG is written next to the SVG, which stays as it was. The same command makes a JPG,
            WebP or another image format: <Code>--to jpg</Code> or <Code>--to webp</Code>. Run{" "}
            <Code>convt targets logo.svg</Code> to list them all.
          </p>
        </Prose>
      </GuideSection>

      <GuideSection id="size" title="Pick the size of the PNG">
        <Prose>
          <p>
            An SVG is drawn from shapes, so it can become a PNG of any size without going blurry.
            convt starts from the width and height written in the SVG, counted at 96 pixels per
            inch, the way a browser shows it. Two options change that:
          </p>
        </Prose>
        <FactList
          facts={[
            [
              <Code>--dpi 192</Code>,
              "Twice the size the SVG sets, which is the @2x copy for Retina screens. 288 is three times. Our 120 × 80 logo came out at 375 × 250 with --dpi 300. convt accepts 18 to 1200.",
            ],
            [
              <Code>--max-size 512</Code>,
              "Caps the longest edge at 512 pixels and keeps the shape. It only makes the PNG smaller, never bigger.",
            ],
          ]}
        />
        <Prose>
          <p>
            Together they give a set size: raise the resolution, then cap it. A 24 × 24 icon came
            out at exactly 256 × 256 with this command:
          </p>
        </Prose>
        <Commands label="convt" commands={["convt icon.svg --to png --dpi 1200 --max-size 256"]} />
        <Prose>
          <p>
            1200 is the most convt takes, which is 12.5 times the SVG's own size. A 24-pixel icon
            can reach 300 pixels that way, so for a bigger PNG from a small icon, set a larger width
            and height in the SVG file first. With a capped size the edge can land a pixel over: our
            logo capped at 1000 came out 1001 × 667.
          </p>
        </Prose>
      </GuideSection>

      <GuideSection id="background" title="Keep it transparent, or fill the background">
        <Prose>
          <p>
            By default the PNG keeps the SVG's transparent areas, which is what you want for a logo
            on a colored slide. To fill them, add <Code>--background</Code> with white, black or a
            color code:
          </p>
        </Prose>
        <Commands
          label="convt"
          commands={[
            "convt logo.svg --to png --background white",
            'convt logo.svg --to png --background "#ff8800"',
            "convt logo.svg --to jpg --dpi 300",
          ]}
        />
        <Prose>
          <p>
            JPG has no transparency, so a JPG gets a white background unless you pick another color.
          </p>
        </Prose>
      </GuideSection>

      <GuideSection id="folder" title="Convert a folder of SVGs to PNG">
        <Prose>
          <p>
            Give convt a folder instead of a file. <Code>-r</Code> includes subfolders and{" "}
            <Code>-o png</Code> writes the PNGs into a new folder with the same subfolders, so an
            icon set keeps its layout. Here every icon comes out at twice its size:
          </p>
        </Prose>
        <Commands label="convt" commands={["convt icons --to png -r -o png --dpi 192"]} />
        <Output label="Output">{`icons/sub/c.SVG -> png/sub/c.png
icons/a.svg -> png/a.png
icons/sub/b.svg -> png/sub/b.png
3 converted, 0 failed`}</Output>
        <Prose>
          <p>
            The 24 × 24 icons came out at 48 × 48. A folder input converts every file convt can turn
            into PNG, so photos in the folder get converted too. To shrink a set to one size, use{" "}
            <Code>--max-size</Code> instead.
          </p>
        </Prose>

        <h3 className="mt-4 text-[19px]/7 font-semibold tracking-[-0.01em]">
          What came out of our test
        </h3>
        <FactList facts={results} />

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
            <dt className="text-[16px]/6 font-semibold">Why is the text missing from my PNG?</dt>
            <dd className="text-[16px]/[26px] text-ink-2">
              An SVG that names a font draws its text with that font from your computer. If the font
              isn't installed, the text can drop out: in our Linux test, a line of text in a font
              the computer didn't have was missing from the PNG, and the same line in an installed
              font came out. Install the font, or turn the text into outlines in the app that made
              the SVG.
            </dd>
          </div>
          <div className="flex flex-col gap-1.5">
            <dt className="text-[16px]/6 font-semibold">Does this work on Windows?</dt>
            <dd className="text-[16px]/[26px] text-ink-2">
              Yes, with the same options. The Windows installer doesn't add convt to your PATH, so
              in PowerShell type its full path:{" "}
              <Code>{String.raw`${convtExe} icons --to png -r -o png --dpi 192`}</Code>.
            </dd>
          </div>
          <div className="flex flex-col gap-1.5">
            <dt className="text-[16px]/6 font-semibold">Can convt turn a PNG into an SVG?</dt>
            <dd className="text-[16px]/[26px] text-ink-2">
              No. Tracing a picture into shapes is a different job, and convt 0.5.0 doesn't offer
              it. It doesn't make a PDF from an SVG either. For photos, see{" "}
              <TextLink href={guidePath("webp-to-png-jpg")}>WebP to PNG or JPG</TextLink>.
            </dd>
          </div>
          <div className="flex flex-col gap-1.5">
            <dt className="text-[16px]/6 font-semibold">Does convt upload my files?</dt>
            <dd className="text-[16px]/[26px] text-ink-2">
              Not for local conversions. The app and the command-line tool convert on your computer.
              The app uploads a file only if you pick Cloud for a job, which is part of Desktop +
              Cloud. For scripts and the other commands, see the{" "}
              <TextLink href={commandLineConverter.path}>command-line page</TextLink> and the{" "}
              <TextLink href={routes.formats}>formats page</TextLink>.
            </dd>
          </div>
        </dl>
      </GuideSection>
    </GuideArticle>
  );
}
