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
import { guideBySlug, guideHead } from "#/lib/guides";
import { GITHUB_URL, routes } from "#/lib/site";

// Every convt command and result on this page comes from a run of the convt 0.5.0 CLI on
// real iPhone XR and iPhone 13 Pro HEIC photos (CNVM-2). The Finder, Preview, Photos and
// sips steps come from Apple's documentation and say so on the page.

const slug = "heic-to-jpg-mac";
const guide = guideBySlug(slug);

const sections = [
  { id: "pick", title: "Pick a method" },
  { id: "finder", title: "Finder Quick Actions" },
  { id: "preview", title: "Preview" },
  { id: "photos", title: "Photos" },
  { id: "terminal", title: "Terminal with sips" },
  { id: "convt", title: "Whole folders with convt" },
  { id: "faq", title: "Questions" },
];

const methods = [
  {
    name: "Finder Quick Actions",
    bestFor: "A few photos at once",
    cost: "Free, built in",
    checked: "Apple's guide",
  },
  {
    name: "Preview",
    bestFor: "One photo, with a quality slider",
    cost: "Free, built in",
    checked: "Apple's guide",
  },
  {
    name: "Photos",
    bestFor: "Pictures already in your library",
    cost: "Free, built in",
    checked: "Apple's guide",
  },
  {
    name: "sips in Terminal",
    bestFor: "One folder of files",
    cost: "Free, built in",
    checked: "Apple's manual",
  },
  {
    name: "convt",
    bestFor: "Folders with subfolders, resizing, repeat jobs",
    cost: "7-day trial, then paid",
    checked: "Run on iPhone photos",
  },
];

const results: [string, string][] = [
  ["Resolution", "4032 × 3024 in, 4032 × 3024 out. Nothing cropped or scaled."],
  ["Portrait photo", "Came out 3024 × 4032 and upright."],
  [
    "File size",
    "A 1.1 MB HEIC became a 1.5 MB JPG at the default quality of 90, and 1.0 MB with -q 80.",
  ],
  ["Subfolders", "With -r, photos in trip/ were converted into trip/."],
  ["Date taken, camera, location", "Not copied. The JPGs have no EXIF data."],
];

export const Route = createFileRoute("/_site/guides/heic-to-jpg-mac")({
  head: () => guideHead(slug),
  component: HeicToJpgMac,
});

function HeicToJpgMac() {
  return (
    <GuideArticle
      guide={guide}
      sections={sections}
      lead={
        <p>
          For one photo or a handful, you don't need to install anything: macOS converts HEIC to JPG
          from Finder, Preview and Photos. For a whole folder, sips in Terminal does it for free.
          convt adds subfolders, quality and resizing options, and a right-click menu for when you
          do this every week.
        </p>
      }
    >
      <div className="flex flex-col gap-3 rounded-2xl bg-green-tint/60 px-5 py-4 shadow-[inset_0_0_0_1px_var(--green-line)]">
        <p className="font-mono text-[11px]/4 text-ink-2 uppercase">The short version</p>
        <p className="text-[16px]/[26px]">
          Select the photos in Finder, Control-click them and choose{" "}
          <strong className="font-semibold">Quick Actions › Convert Image</strong>. Set Format to
          JPEG and click Convert to JPEG. The JPGs appear next to the originals.
        </p>
      </div>

      <GuideSection id="pick" title="Pick a method">
        <ul className="divide-y divide-divider overflow-hidden rounded-2xl bg-raised shadow-[inset_0_0_0_1px_var(--line)] dark:bg-panel">
          {methods.map((m) => (
            <li
              key={m.name}
              className="flex flex-col gap-1 px-4 py-3.5 text-[14px]/5 md:grid md:grid-cols-[170px_1fr_150px_160px] md:items-baseline md:gap-4"
            >
              <span className="font-medium">{m.name}</span>
              <span className="text-ink-2">{m.bestFor}</span>
              <span className="text-ink-2">
                {m.cost}
                <span className="md:hidden"> · {m.checked}</span>
              </span>
              <span className="text-ink-2 max-md:hidden">{m.checked}</span>
            </li>
          ))}
        </ul>
      </GuideSection>

      <GuideSection
        id="finder"
        title="Convert a few photos with Finder Quick Actions"
        source={
          <Checked how="docs">
            From Apple's guide,{" "}
            <TextLink href="https://support.apple.com/guide/mac-help/perform-quick-actions-in-the-finder-on-mac-mchl97ff9142/mac">
              Perform quick actions in the Finder
            </TextLink>
            . We haven't run it ourselves.
          </Checked>
        }
      >
        <Prose>
          <p>Quick Actions is the fastest free way, and it works on a whole selection at once.</p>
        </Prose>
        <Steps>
          {[
            <>In Finder, select the HEIC photos. Command-click to pick several.</>,
            <>
              Control-click the selection and choose{" "}
              <strong className="font-semibold">Quick Actions</strong>, then{" "}
              <strong className="font-semibold">Convert Image</strong>. On recent versions of macOS
              it sits under More.
            </>,
            <>Set Format to JPEG and choose an image size. Actual Size keeps every pixel.</>,
            <>
              Click <strong className="font-semibold">Convert to JPEG</strong>. The JPGs are saved
              next to the originals, which stay as they were.
            </>,
          ]}
        </Steps>
        <Prose>
          <p>
            The dialog also has a Preserve Metadata checkbox. Leave it on if you want the date taken
            and location to carry over to the JPGs.
          </p>
        </Prose>
      </GuideSection>

      <GuideSection
        id="preview"
        title="Convert one photo in Preview"
        source={
          <Checked how="docs">
            From Apple's guide,{" "}
            <TextLink href="https://support.apple.com/guide/preview/convert-image-file-types-prvw1012/mac">
              Convert image file types using Preview
            </TextLink>
            . We haven't run it ourselves.
          </Checked>
        }
      >
        <Prose>
          <p>Preview is the way to go when you want to choose the JPEG quality yourself.</p>
        </Prose>
        <Steps>
          {[
            <>Open the photo in Preview.</>,
            <>
              Choose <strong className="font-semibold">File › Export</strong>.
            </>,
            <>
              Choose JPEG from the Format menu and set the Quality slider. Higher quality means a
              bigger file.
            </>,
            <>Pick a name and a folder, then click Save.</>,
          ]}
        </Steps>
        <Prose>
          <p>
            For several photos, open them in one Preview window, select them all in the sidebar,
            then use File › Export the same way.
          </p>
        </Prose>
      </GuideSection>

      <GuideSection
        id="photos"
        title="Export from the Photos app"
        source={
          <Checked how="docs">
            From Apple's guide,{" "}
            <TextLink href="https://support.apple.com/guide/photos/export-photos-videos-slideshows-and-memories-pht6e157c5f/mac">
              Export photos, videos, slideshows, and memories
            </TextLink>
            . We haven't run it ourselves.
          </Checked>
        }
      >
        <Prose>
          <p>If the pictures came to your Mac through iCloud Photos, they're already in Photos.</p>
        </Prose>
        <Steps>
          {[
            <>Select the photos.</>,
            <>
              Choose{" "}
              <strong className="font-semibold">File › Export › Export [number] Photos</strong>.
            </>,
            <>
              Set Photo Kind to JPEG. You can also pick the JPEG quality, the size and a color
              profile.
            </>,
            <>Click Export, choose a folder, and click Export again.</>,
          ]}
        </Steps>
      </GuideSection>

      <GuideSection
        id="terminal"
        title="Batch convert in Terminal with sips"
        source={
          <Checked how="docs">
            From Apple's sips manual (<Code>man sips</Code> on your Mac). We wrote this guide on
            Linux, so we haven't run sips.
          </Checked>
        }
      >
        <Prose>
          <p>
            sips comes with every Mac and converts a folder of photos in one command. Open Terminal,
            go to the folder, and write the JPGs into a new folder called <Code>jpg</Code>:
          </p>
        </Prose>
        <Commands
          label="sips"
          commands={[
            "cd ~/Pictures/iphone",
            "mkdir jpg",
            "sips -s format jpeg -s formatOptions 85 *.HEIC --out jpg",
          ]}
        />
        <Prose>
          <p>
            <Code>formatOptions</Code> is the JPEG quality as a percentage. The <Code>*.HEIC</Code>{" "}
            pattern is case-sensitive, so use <Code>*.heic</Code> if your file names are lowercase.
            sips doesn't look inside subfolders. To convert those, run it in each folder or use the
            next method.
          </p>
        </Prose>
      </GuideSection>

      <GuideSection
        id="convt"
        title="Convert whole folders with convt"
        source={
          <Checked how="tested">
            Run with the convt 0.5.0 CLI on five HEIC photos from an iPhone XR and an iPhone 13 Pro.
          </Checked>
        }
      >
        <Prose>
          <p>
            convt is a Mac app that converts files on your computer, with a command-line tool in the
            same install. Local conversions never upload your photos. It's worth it when you convert
            photos often, have folders inside folders, or want every photo resized to fit.
          </p>
          <p>
            Install it with Homebrew, which puts the <Code>convt</Code> command in Terminal, or from{" "}
            <TextLink href={`${GITHUB_URL}/releases/latest`}>GitHub releases</TextLink>. Neither
            asks you to sign in.
          </p>
        </Prose>
        <Commands label="Homebrew" commands={homebrewCommands} />
        <Prose>
          <p>
            If you installed from the disk image, type{" "}
            <Code>/Applications/convt.app/Contents/MacOS/convt</Code> instead of <Code>convt</Code>.
          </p>
        </Prose>
        <Commands label="convt" commands={["convt photos/ --to jpg -r"]} />
        <Output label="Output">{`photos/IMG_3579.HEIC -> photos/IMG_3579.jpg
photos/IMG_3578.HEIC -> photos/IMG_3578.jpg
photos/trip/IMG_3605.HEIC -> photos/trip/IMG_3605.jpg
photos/IMG_8563.HEIC -> photos/IMG_8563.jpg
photos/trip/IMG_3616.HEIC -> photos/trip/IMG_3616.jpg
5 converted, 0 failed`}</Output>
        <Prose>
          <p>
            <Code>-r</Code> includes subfolders, and each JPG is written next to its HEIC. Run the
            command again and convt writes <Code>IMG_3578 (1).jpg</Code> rather than replacing the
            first copy. A few options cover most batches:
          </p>
        </Prose>
        <FactList
          facts={[
            [
              <Code>-o ~/Desktop/jpg</Code>,
              "Puts the JPGs in another folder and recreates the subfolders there.",
            ],
            [<Code>-q 80</Code>, "Sets JPEG quality from 1 to 100. Without it, convt uses 90."],
            [<Code>--max-size 2048</Code>, "Limits the longest side to 2,048 pixels."],
          ]}
        />

        <h3 className="mt-4 text-[19px]/7 font-semibold tracking-[-0.01em]">
          What came out of our test
        </h3>
        <FactList facts={results} />
        <Prose>
          <p>
            The last row matters. convt 0.5.0 doesn't copy EXIF data when it turns HEIC into JPG, so
            the date taken, the camera and the GPS location are missing from the copies. That's
            handy when you share photos with strangers, but photo apps will sort the JPGs by file
            date instead. If you need that data, use Finder with Preserve Metadata on, or export
            from Photos.
          </p>
          <p className="text-[14px]/[22px] text-ink-2">
            We ran the convt tests on Linux, where convt reads HEIC through libheif. On a Mac it
            uses Apple's own decoder; the JPEG step is the same code. We didn't compare colors
            between the two.
          </p>
        </Prose>

        <h3 className="mt-4 text-[19px]/7 font-semibold tracking-[-0.01em]">
          Or right-click in Finder
        </h3>
        <Checked how="docs">
          From convt's own documentation. We haven't clicked through it in Finder for this guide.
        </Checked>
        <Prose>
          <p>
            Open convt once so it can tell Finder which formats to offer, then turn on its Finder
            extension in System Settings › General › Login Items & Extensions › Finder. After that,
            Control-click your HEIC photos, choose{" "}
            <strong className="font-semibold">Convert with convt</strong>, then JPEG. The JPGs show
            up next to the originals without opening a window.
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
            <dt className="text-[16px]/6 font-semibold">
              Does converting HEIC to JPG lose quality?
            </dt>
            <dd className="text-[16px]/[26px] text-ink-2">
              A little. JPEG is a lossy format, so the photo is compressed again. The resolution
              stays the same, and the file usually gets bigger: in our test a 1.1 MB HEIC became a
              1.5 MB JPG at quality 90.
            </dd>
          </div>
          <div className="flex flex-col gap-1.5">
            <dt className="text-[16px]/6 font-semibold">
              Can my iPhone take photos as JPG instead?
            </dt>
            <dd className="text-[16px]/[26px] text-ink-2">
              Yes. Open Settings › Camera › Formats and choose Most Compatible. New photos are saved
              as JPEG from then on, and take more space. Source:{" "}
              <TextLink href="https://support.apple.com/en-us/116944">
                Using HEIF or HEVC media on Apple devices
              </TextLink>
              .
            </dd>
          </div>
          <div className="flex flex-col gap-1.5">
            <dt className="text-[16px]/6 font-semibold">
              Is there a free way to convert many at once?
            </dt>
            <dd className="text-[16px]/[26px] text-ink-2">
              Yes. Finder Quick Actions and Preview both take a selection, and sips converts a whole
              folder from Terminal.
            </dd>
          </div>
          <div className="flex flex-col gap-1.5">
            <dt className="text-[16px]/6 font-semibold">Does convt upload my photos?</dt>
            <dd className="text-[16px]/[26px] text-ink-2">
              Not for local conversions. The app and the command-line tool convert on your Mac. The
              app uploads a file only if you pick Cloud for a job, which is part of convt Pro. See
              every format it handles on the <TextLink href={routes.formats}>formats page</TextLink>
              .
            </dd>
          </div>
        </dl>
      </GuideSection>
    </GuideArticle>
  );
}
