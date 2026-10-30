import { createFileRoute } from "@tanstack/react-router";

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
// Linux CLI on HEIC files in nested folders (CNVM-6). Windows reads HEIC through the same
// libheif engine, bundled in the MSI. The Photos, extension and iPhone steps come from
// Microsoft's Store, Microsoft Q&A and Apple's documentation and say so on the page.

const slug = "batch-heic-to-jpg-windows";
const guide = guideBySlug(slug);

const convtExe = String.raw`& "$env:LOCALAPPDATA\Programs\convt\convt.exe"`;

const sections = [
  { id: "pick", title: "Pick a method" },
  { id: "extensions", title: "Open HEIC on Windows" },
  { id: "photos", title: "One photo in Photos" },
  { id: "convt", title: "Whole folders with convt" },
  { id: "iphone", title: "Get JPGs from the iPhone" },
  { id: "faq", title: "Questions" },
];

const methods = [
  {
    name: "Photos app",
    bestFor: "One photo at a time",
    cost: "Free app, extensions from the Store",
    checked: "Microsoft Q&A",
  },
  {
    name: "convt",
    bestFor: "Folders with subfolders, resizing, repeat jobs",
    cost: "$39 once",
    checked: "Run on HEIC files",
  },
  {
    name: "iPhone settings",
    bestFor: "Stopping HEIC before it reaches the PC",
    cost: "Free",
    checked: "Apple's guide",
  },
];

const results: [string, string][] = [
  ["Subfolders", "With -r, every HEIC in 2024, 2024\\Summer trip and 2025 was converted."],
  ["Output folder", "-o jpg recreated the same subfolders under jpg, and left the HEICs alone."],
  ["Resolution", "4032 × 3024 in, 4032 × 3024 out. A portrait photo stayed 3024 × 4032."],
  ["File names", ".HEIC and .heic both worked, and so did a folder name with a space in it."],
  ["Running it twice", "The second run wrote IMG_0001 (1).jpg instead of replacing IMG_0001.jpg."],
  ["Date taken, camera, location", "Not copied. convt 0.5.0 writes JPGs without EXIF data."],
];

export const Route = createFileRoute("/_site/guides/batch-heic-to-jpg-windows")({
  head: () => guideHead(slug),
  component: BatchHeicToJpgWindows,
});

function BatchHeicToJpgWindows() {
  return (
    <GuideArticle
      guide={guide}
      sections={sections}
      lead={
        <p>
          Windows 11 can open iPhone HEIC photos once two extensions from the Microsoft Store are
          installed, and the Photos app saves them as JPG one at a time. For a whole folder, with
          subfolders, one convt command converts everything and keeps the folder layout.
        </p>
      }
    >
      <div className="flex flex-col gap-3 rounded-2xl bg-green-tint/60 px-5 py-4 shadow-[inset_0_0_0_1px_var(--green-line)]">
        <p className="font-mono text-[11px]/4 text-ink-2 uppercase">The short version</p>
        <p className="text-[16px]/[26px]">
          To convert a folder and every folder inside it, install convt, open PowerShell in the
          folder that holds your photos folder and run:
        </p>
        <Commands
          label="PowerShell"
          commands={[String.raw`${convtExe} iPhone --to jpg -r -o jpg`]}
        />
        <p className="text-[16px]/[26px]">
          The JPGs land in a new <Code>jpg</Code> folder with the same subfolders. The HEIC files
          stay where they are.
        </p>
      </div>

      <GuideSection id="pick" title="Pick a method">
        <ul className="divide-y divide-divider overflow-hidden rounded-2xl bg-raised shadow-[inset_0_0_0_1px_var(--line)] dark:bg-panel">
          {methods.map((m) => (
            <li
              key={m.name}
              className="flex flex-col gap-1 px-4 py-3.5 text-[14px]/5 md:grid md:grid-cols-[150px_1fr_200px_140px] md:items-baseline md:gap-4"
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
        <Prose>
          <p>
            Windows has no built-in way to convert a folder of HEIC photos in one go. The free route
            is one photo at a time; for more than a handful, use a batch tool like convt.
          </p>
        </Prose>
      </GuideSection>

      <GuideSection
        id="extensions"
        title="Let Windows open HEIC photos"
        source={
          <Checked how="docs">
            From the Microsoft Store listings, checked October 29, 2026. We haven't installed them
            ourselves.
          </Checked>
        }
      >
        <Prose>
          <p>
            HEIC photos are HEVC-compressed images in a HEIF container, so Windows needs two
            extensions before Photos can show them:
          </p>
        </Prose>
        <FactList
          facts={[
            [
              <TextLink href="https://apps.microsoft.com/detail/9pmmsr1cgpwg">
                HEIF Image Extension
              </TextLink>,
              "Reads the HEIF container. Free.",
            ],
            [
              <TextLink href="https://apps.microsoft.com/detail/9nmzlz57r3t7">
                HEVC Video Extensions
              </TextLink>,
              "Decodes the image inside. $0.99 in the US Store. Some PCs already have the free version from the device maker.",
            ],
          ]}
        />
        <Prose>
          <p>
            convt doesn't need either one: it brings its own HEIC decoder, so you can skip this step
            if you go straight to <TextLink href="#convt">convt</TextLink>.
          </p>
        </Prose>
      </GuideSection>

      <GuideSection
        id="photos"
        title="Save one photo as JPG in Photos"
        source={
          <Checked how="docs">
            From answers on{" "}
            <TextLink href="https://learn.microsoft.com/en-us/answers/questions/5533391/i-cant-open-heic-images-on-windows-10-should-i-con">
              Microsoft Q&A
            </TextLink>
            . We haven't run it on Windows.
          </Checked>
        }
      >
        <Steps>
          {[
            <>
              Right-click the HEIC photo and choose{" "}
              <strong className="font-semibold">Open with › Photos</strong>.
            </>,
            <>
              Click the <strong className="font-semibold">…</strong> menu at the top and choose{" "}
              <strong className="font-semibold">Save as</strong>.
            </>,
            <>Pick JPG as the file type, choose a folder and click Save.</>,
          ]}
        </Steps>
        <Prose>
          <p>
            That's fine for a few photos. For a camera roll's worth, it's one dialog per photo, and
            Photos won't walk through subfolders for you.
          </p>
        </Prose>
      </GuideSection>

      <GuideSection
        id="convt"
        title="Batch convert whole folders with convt"
        source={
          <Checked how="tested">
            Run with the released convt 0.5.0 command-line tool on HEIC files in nested folders. We
            ran it on Linux; the Windows build reads HEIC through the same libheif decoder, bundled
            in its installer.
          </Checked>
        }
      >
        <Prose>
          <p>
            convt is a desktop app for Windows and Mac that converts files on your computer, with a
            command-line tool in the same install. Local conversions never upload your photos.
          </p>
          <p>
            Install it from the <TextLink href={guideDownloadUrl(slug)}>download page</TextLink> or{" "}
            <TextLink href={`${GITHUB_URL}/releases/latest`}>GitHub releases</TextLink>. The
            installer puts convt in <Code>%LOCALAPPDATA%\Programs\convt</Code> for your user, so it
            doesn't need an administrator. The installer isn't signed yet, so Windows may warn you
            before it runs. It doesn't add convt to your PATH, so the commands below use its full
            path.
          </p>
        </Prose>

        <h3 className="mt-4 text-[19px]/7 font-semibold tracking-[-0.01em]">
          Convert a folder and its subfolders
        </h3>
        <Prose>
          <p>
            Say your photos are in <Code>Pictures\iPhone</Code>, sorted into folders by year. Open
            PowerShell (right-click Start, then Terminal) and run:
          </p>
        </Prose>
        <Commands
          label="PowerShell"
          commands={["cd $HOME\\Pictures", String.raw`${convtExe} iPhone --to jpg -r -o jpg`]}
        />
        <Output label="Output from our Linux run">{`iPhone/2024/IMG_0950.heic -> jpg/2024/IMG_0950.jpg
iPhone/2024/Summer trip/IMG_1040.HEIC -> jpg/2024/Summer trip/IMG_1040.jpg
iPhone/2025/IMG_2001.HEIC -> jpg/2025/IMG_2001.jpg
iPhone/IMG_0001.HEIC -> jpg/IMG_0001.jpg
iPhone/IMG_0002.HEIC -> jpg/IMG_0002.jpg
5 converted, 0 failed`}</Output>
        <Prose>
          <p>
            <Code>-r</Code> includes subfolders and <Code>-o jpg</Code> writes the copies into a new
            folder, rebuilding the same subfolders there. Leave out <Code>-o</Code> and each JPG is
            written next to its HEIC instead. convt converts several files at once, one per CPU core
            by default.
          </p>
          <p>
            One thing to know: convt converts every file in the folder that can become a JPG, not
            just the HEICs. PNG screenshots in the folder get a JPG copy too. With <Code>-o</Code>{" "}
            those copies end up in the output folder, where you can delete them, and nothing in the
            original folder changes.
          </p>
        </Prose>
        <FactList
          facts={[
            [<Code>-q 80</Code>, "Sets JPEG quality from 1 to 100. Without it, convt uses 90."],
            [<Code>--max-size 2048</Code>, "Limits the longest side to 2,048 pixels."],
            [<Code>-j 2</Code>, "Converts two files at a time, if you want to keep the PC free."],
          ]}
        />

        <h3 className="mt-4 text-[19px]/7 font-semibold tracking-[-0.01em]">
          What came out of our test
        </h3>
        <FactList facts={results} />
        <Prose>
          <p>
            The last row matters for photo libraries. convt 0.5.0 doesn't copy EXIF data from HEIC
            to JPG, so the date taken, the camera and the GPS location are missing from the copies.
            Photo apps may sort the copies differently because the date taken is missing. If you
            need that data, keep the HEIC originals, or have the iPhone hand over JPGs in the first
            place (next section).
          </p>
        </Prose>

        <h3 className="mt-4 text-[19px]/7 font-semibold tracking-[-0.01em]">
          Or use the convt window
        </h3>
        <Checked how="docs">
          From convt's source code. We haven't clicked through it on Windows for this guide.
        </Checked>
        <Prose>
          <p>
            Open convt from the Start menu, select the HEIC photos in File Explorer (Ctrl+A selects
            a whole folder) and drag them onto the window, or click{" "}
            <strong className="font-semibold">Choose files…</strong> and pick them there. Then
            choose JPEG and convert. The command above is faster for folders inside folders. convt's
            right-click menu in File Explorer isn't part of the Windows installer yet.
          </p>
        </Prose>

        <GuideCta slug={slug}>
          <p className="text-[16px]/6 font-semibold text-ink">
            convt for Windows, and for Macs with Apple silicon
          </p>
          <p>
            A Desktop license is $39, paid once, with every future update. Local conversions never
            leave your computer.
          </p>
        </GuideCta>
      </GuideSection>

      <GuideSection
        id="iphone"
        title="Get JPGs from the iPhone instead"
        source={
          <Checked how="docs">
            From Apple's guide,{" "}
            <TextLink href="https://support.apple.com/en-us/116944">
              Using HEIF or HEVC media on Apple devices
            </TextLink>
            . We haven't run it ourselves.
          </Checked>
        }
      >
        <Prose>
          <p>Two iPhone settings decide whether HEIC files reach your PC at all:</p>
        </Prose>
        <FactList
          facts={[
            [
              "Settings › Apps › Photos",
              "Apple says photos imported to a PC may be converted to JPEG on the way. Keep Originals turns that off, so leave it off if you want JPGs.",
            ],
            [
              "Settings › Camera › Formats",
              "Most Compatible saves new photos as JPEG from then on. They take more space on the phone.",
            ],
          ]}
        />
        <Prose>
          <p>
            Neither setting changes photos that are already on your PC. For those, use one of the
            methods above.
          </p>
        </Prose>
      </GuideSection>

      <GuideSection id="faq" title="Questions">
        <dl className="flex flex-col gap-6">
          <div className="flex flex-col gap-1.5">
            <dt className="text-[16px]/6 font-semibold">Can I just rename .HEIC to .jpg?</dt>
            <dd className="text-[16px]/[26px] text-ink-2">
              No. Renaming changes the name, not the data inside, so most apps will refuse to open
              the file or show an error. The photo has to be decoded and saved again as JPEG.
            </dd>
          </div>
          <div className="flex flex-col gap-1.5">
            <dt className="text-[16px]/6 font-semibold">
              Is there a free way to batch convert on Windows?
            </dt>
            <dd className="text-[16px]/[26px] text-ink-2">
              Not built into Windows. Photos converts one photo at a time. Free third-party tools
              exist, and many online converters do batches too, but most of them upload your photos
              to their servers.
            </dd>
          </div>
          <div className="flex flex-col gap-1.5">
            <dt className="text-[16px]/6 font-semibold">
              Does converting HEIC to JPG lose quality?
            </dt>
            <dd className="text-[16px]/[26px] text-ink-2">
              A little. JPEG is a lossy format, so the photo is compressed again. The resolution
              stays the same, and the JPG is usually bigger than the HEIC.
            </dd>
          </div>
          <div className="flex flex-col gap-1.5">
            <dt className="text-[16px]/6 font-semibold">Does convt upload my photos?</dt>
            <dd className="text-[16px]/[26px] text-ink-2">
              Not for local conversions. The app and the command-line tool convert on your PC. The
              app uploads a file only if you pick Cloud for a job, which is part of convt Pro. On a
              Mac? See <TextLink href={guidePath("heic-to-jpg-mac")}>HEIC to JPG on a Mac</TextLink>
              , or every format convt handles on the{" "}
              <TextLink href={routes.formats}>formats page</TextLink>.
            </dd>
          </div>
        </dl>
      </GuideSection>
    </GuideArticle>
  );
}
