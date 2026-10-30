import type { ReactNode } from "react";

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
import { guideBySlug, guideDownloadUrl, guideHead } from "#/lib/guides";
import { GITHUB_URL, routes } from "#/lib/site";

// Every FFmpeg and convt command and result on this page comes from a run of the released
// convt 0.5.0 Linux CLI and the FFmpeg 7.0.2 it bundles, on 8-second 1080p MOV test clips in
// H.264, HEVC and ProRes 422 (CNVM-7). The Mac and Windows builds run the same FFmpeg
// arguments. The rename and iMovie steps come from Apple's guide and say so on the page.

const slug = "mov-to-mp4-mac";
const guide = guideBySlug(slug);

const sections = [
  { id: "pick", title: "Pick a method" },
  { id: "rename", title: "Rename an H.264 MOV" },
  { id: "imovie", title: "Export from iMovie" },
  { id: "ffmpeg", title: "FFmpeg: remux or re-encode" },
  { id: "convt", title: "Whole folders with convt" },
  { id: "smaller", title: "Make the MP4 smaller" },
  { id: "windows", title: "On Windows" },
  { id: "faq", title: "Questions" },
];

const methods = [
  {
    name: "Rename to .mp4",
    bestFor: "H.264 videos that only need a new name",
    cost: "Free, built in",
    checked: "Apple's guide",
  },
  {
    name: "iMovie",
    bestFor: "One video, with a quality menu",
    cost: "Free from Apple",
    checked: "Apple's guide",
  },
  {
    name: "FFmpeg",
    bestFor: "Remuxing without quality loss",
    cost: "Free, Terminal",
    checked: "Run on test clips",
  },
  {
    name: "convt",
    bestFor: "Folders with subfolders, smaller files",
    cost: "7-day trial, then paid",
    checked: "Run on test clips",
  },
];

const remuxResults: [string, string][] = [
  ["H.264 MOV, 14.7 MB", "Remuxed in under a second to a 14.7 MB MP4. Same video, new container."],
  ["HEVC MOV, 10.8 MB", "Remuxed to a 10.8 MB MP4, still HEVC and tagged hvc1."],
  [
    "ProRes 422 MOV, 59.5 MB",
    "Remux failed: MP4 can't hold ProRes. Re-encoding to H.264 made a 6.4 MB MP4.",
  ],
];

const smallerResults: [ReactNode, ReactNode][] = [
  ["No options", "8.6 MB. 1080p H.264 at convt's default quality."],
  [<Code>--video-height 720</Code>, "2.7 MB. Scaled to 1280 × 720."],
  [<Code>-q 40</Code>, "2.4 MB. Still 1080p, with more visible compression."],
  [<Code>--video-codec hevc</Code>, "7.7 MB. HEVC at its default quality."],
  [
    <Code>--video-codec hevc --video-height 720 --audio-bitrate 96</Code>,
    "1.9 MB. All three together.",
  ],
];

export const Route = createFileRoute("/_site/guides/mov-to-mp4-mac")({
  head: () => guideHead(slug),
  component: MovToMp4Mac,
});

function MovToMp4Mac() {
  return (
    <GuideArticle
      guide={guide}
      sections={sections}
      lead={
        <p>
          MOV and MP4 are both containers, and the video inside an iPhone or screen recording MOV is
          usually H.264 or HEVC, which MP4 can hold as is. So the fastest conversion often copies
          the video into a new file without touching a frame. When that won't work, or you want a
          smaller file, the video has to be encoded again.
        </p>
      }
    >
      <div className="flex flex-col gap-3 rounded-2xl bg-green-tint/60 px-5 py-4 shadow-[inset_0_0_0_1px_var(--green-line)]">
        <p className="font-mono text-[11px]/4 text-ink-2 uppercase">The short version</p>
        <p className="text-[16px]/[26px]">
          With FFmpeg installed, this copies the video and audio into an MP4 without re-encoding, so
          nothing is lost and it takes seconds:
        </p>
        <Commands
          label="FFmpeg"
          commands={["ffmpeg -i input.mov -c copy -movflags +faststart output.mp4"]}
        />
        <p className="text-[16px]/[26px]">
          If FFmpeg says it <q>could not find tag for codec</q>, the MOV holds ProRes or another
          codec MP4 can't carry. Re-encode it instead, as shown in{" "}
          <TextLink href="#ffmpeg">the FFmpeg section</TextLink>.
        </p>
      </div>

      <GuideSection id="pick" title="Pick a method">
        <ul className="divide-y divide-divider overflow-hidden rounded-2xl bg-raised shadow-[inset_0_0_0_1px_var(--line)] dark:bg-panel">
          {methods.map((m) => (
            <li
              key={m.name}
              className="flex flex-col gap-1 px-4 py-3.5 text-[14px]/5 md:grid md:grid-cols-[150px_1fr_170px_150px] md:items-baseline md:gap-4"
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
            QuickTime Player isn't on the list: its Export As menu saves a new MOV at a different
            resolution, not an MP4.
          </p>
        </Prose>
      </GuideSection>

      <GuideSection
        id="rename"
        title="Rename an H.264 MOV to MP4"
        source={
          <Checked how="docs">
            From Apple's guide,{" "}
            <TextLink href="https://support.apple.com/en-us/120755">
              Convert a MOV file to MP4 on Mac
            </TextLink>
            . We haven't run it ourselves.
          </Checked>
        }
      >
        <Steps>
          {[
            <>
              Select the MOV in Finder and choose{" "}
              <strong className="font-semibold">File › Get Info</strong>. Under More Info, check
              that one of the codecs is H.264. If it isn't, use another method.
            </>,
            <>
              Control-click the file and choose <strong className="font-semibold">Duplicate</strong>{" "}
              if you want to keep the original.
            </>,
            <>
              Control-click the file, choose <strong className="font-semibold">Rename</strong>,
              change the extension to <Code>mp4</Code> and press Return.
            </>,
            <>
              Click <strong className="font-semibold">Use .mp4</strong> when macOS asks.
            </>,
          ]}
        </Steps>
        <Prose>
          <p>
            Apple says the resolution and size stay the same. This works because many players and
            upload forms read an H.264 MOV fine once it's called MP4. It isn't a real MP4 file
            underneath, so if a site still rejects it, remux it with FFmpeg or convert it properly.
          </p>
        </Prose>
      </GuideSection>

      <GuideSection
        id="imovie"
        title="Export an MP4 from iMovie"
        source={
          <Checked how="docs">
            From the same{" "}
            <TextLink href="https://support.apple.com/en-us/120755">Apple guide</TextLink>. We
            haven't run it ourselves.
          </Checked>
        }
      >
        <Steps>
          {[
            <>
              In iMovie's Projects view, click <strong className="font-semibold">Create New</strong>{" "}
              and choose Movie.
            </>,
            <>
              Click <strong className="font-semibold">Import</strong>, select the MOV and click
              Import Selected.
            </>,
            <>
              Select the clip and choose{" "}
              <strong className="font-semibold">Edit › Add to Movie</strong> (or press E).
            </>,
            <>
              Click the Share button and choose{" "}
              <strong className="font-semibold">Export File</strong>. Set Quality to Medium or Low,
              or lower the resolution, for a smaller file.
            </>,
            <>Click Next, choose where to save it and click Save.</>,
          ]}
        </Steps>
        <Prose>
          <p>
            iMovie is free from Apple and re-encodes the video, so it works for any MOV. It handles
            one project at a time, which gets slow for a folder of clips.
          </p>
        </Prose>
      </GuideSection>

      <GuideSection
        id="ffmpeg"
        title="Convert MOV to MP4 with FFmpeg"
        source={
          <Checked how="tested">
            Run with FFmpeg 7.0.2 on 8-second 1080p MOV clips in H.264, HEVC and ProRes 422.
          </Checked>
        }
      >
        <Prose>
          <p>
            FFmpeg is the free command-line tool most video apps are built on. Install it with{" "}
            <TextLink href="https://brew.sh">Homebrew</TextLink> (<Code>brew install ffmpeg</Code>),
            then pick one of two commands.
          </p>
        </Prose>

        <h3 className="mt-4 text-[19px]/7 font-semibold tracking-[-0.01em]">
          Remux: copy the video into an MP4
        </h3>
        <Commands
          label="FFmpeg"
          commands={["ffmpeg -i input.mov -c copy -movflags +faststart output.mp4"]}
        />
        <Prose>
          <p>
            <Code>-c copy</Code> copies the video and audio streams as they are, so there's no
            quality loss and it runs in about the time it takes to copy the file.{" "}
            <Code>-movflags +faststart</Code> moves the index to the front so the MP4 starts playing
            in a browser before it has fully downloaded. For HEVC videos from an iPhone, add{" "}
            <Code>-tag:v hvc1</Code> so QuickTime and Apple devices recognise the copy.
          </p>
        </Prose>

        <h3 className="mt-4 text-[19px]/7 font-semibold tracking-[-0.01em]">
          Re-encode: when remuxing fails
        </h3>
        <Commands
          label="FFmpeg"
          commands={[
            "ffmpeg -i input.mov -c:v libx264 -crf 23 -preset medium -c:a aac -b:a 160k -movflags +faststart output.mp4",
          ]}
        />
        <Prose>
          <p>
            This makes a new H.264 video, which plays almost everywhere. <Code>-crf</Code> sets the
            quality: lower numbers look better and make bigger files, and 18 to 28 is the usual
            range. It takes longer than a remux, because every frame is encoded again.
          </p>
        </Prose>

        <h3 className="mt-4 text-[19px]/7 font-semibold tracking-[-0.01em]">
          What came out of our test
        </h3>
        <FactList facts={remuxResults} />
        <Prose>
          <p>The ProRes error looked like this, and it means the codec has to change:</p>
        </Prose>
        <Output label="FFmpeg error">{`Could not find tag for codec prores in stream #0, codec not currently supported in container
Could not write header (incorrect codec parameters ?): Invalid argument`}</Output>
        <Prose>
          <p>
            To convert every MOV in a folder with FFmpeg, loop over them in Terminal:{" "}
            <Code>{'for f in *.mov; do ffmpeg -i "$f" -c copy "${f%.mov}.mp4"; done'}</Code>. That
            loop doesn't look inside subfolders and stops making sense once some files need
            re-encoding; that's where convt helps.
          </p>
        </Prose>
      </GuideSection>

      <GuideSection
        id="convt"
        title="Convert whole folders with convt"
        source={
          <Checked how="tested">
            Run with the released convt 0.5.0 command-line tool on Linux, on MOV files in nested
            folders. The Mac build bundles FFmpeg and passes it the same arguments.
          </Checked>
        }
      >
        <Prose>
          <p>
            convt is a Mac and Windows app that converts files on your computer, with a command-line
            tool in the same install. It brings its own FFmpeg, so you don't need to install it.
            Local conversions never upload your videos.
          </p>
          <p>
            Install it with Homebrew, which puts the <Code>convt</Code> command in Terminal, or from
            the <TextLink href={guideDownloadUrl(slug)}>download page</TextLink> or{" "}
            <TextLink href={`${GITHUB_URL}/releases/latest`}>GitHub releases</TextLink>.
          </p>
        </Prose>
        <Commands label="Homebrew" commands={homebrewCommands} />
        <Prose>
          <p>
            If you installed from the disk image, type{" "}
            <Code>/Applications/convt.app/Contents/MacOS/convt</Code> instead of <Code>convt</Code>.
            To convert a folder and every folder inside it into a separate <Code>mp4</Code> folder:
          </p>
        </Prose>
        <Commands label="convt" commands={["convt videos --to mp4 -r -o mp4"]} />
        <Output label="Output">{`videos/2025/trip/b c.mov -> mp4/2025/trip/b c.mp4
videos/2025/x.mov -> mp4/2025/x.mp4
videos/a.MOV -> mp4/a.mp4
3 converted, 0 failed`}</Output>
        <Prose>
          <p>
            <Code>-r</Code> includes subfolders and <Code>-o mp4</Code> rebuilds the same folders
            under <Code>mp4</Code>. Leave out <Code>-o</Code> and each MP4 is written next to its
            MOV. Running the command again writes <Code>a (1).mp4</Code> rather than replacing the
            first copy. convt converts videos one at a time.
          </p>
          <p>
            convt 0.5.0 always re-encodes: it makes H.264 video (CRF 20, the x264 encoder's medium
            preset) with AAC audio, whatever the MOV holds. That means ProRes and HEVC files work
            the same as H.264 ones, but it's slower than a remux, and the copy is a new encode. For
            a pure remux, use the FFmpeg command above.
          </p>
        </Prose>
        <FactList
          facts={[
            [
              "Resolution and length",
              "1920 × 1080 and 8 seconds in, the same out, for all three codecs.",
            ],
            [
              "Codec",
              "H.264 High profile and AAC audio in every MP4, including from HEVC and ProRes.",
            ],
            [
              "Recording date",
              "Not copied in our test, though a location tag was. FFmpeg's remux kept both.",
            ],
          ]}
        />

        <h3 className="mt-4 text-[19px]/7 font-semibold tracking-[-0.01em]">
          Or right-click in Finder
        </h3>
        <Checked how="docs">
          From convt's own documentation. We haven't clicked through it in Finder for this guide.
        </Checked>
        <Prose>
          <p>
            Open convt once, then turn on its Finder extension in System Settings › General › Login
            Items & Extensions › Finder. After that, Control-click your MOV files, choose{" "}
            <strong className="font-semibold">Convert with convt</strong>, then MP4.
          </p>
        </Prose>
      </GuideSection>

      <GuideSection
        id="smaller"
        title="Make the MP4 smaller"
        source={
          <Checked how="tested">
            Run with the convt 0.5.0 command-line tool on one 8-second 1080p H.264 MOV of 14.7 MB.
          </Checked>
        }
      >
        <Prose>
          <p>
            A converted MP4 is often smaller than the MOV, but not always. These options trade size
            for quality, and you can combine them:
          </p>
        </Prose>
        <Commands label="convt" commands={["convt clip.mov --to mp4 --video-height 720 -q 60"]} />
        <FactList
          facts={[
            [
              <Code>--video-height 720</Code>,
              "Scales the video down to at most 720 pixels tall. It never scales up.",
            ],
            [<Code>-q 60</Code>, "Quality from 1 to 100. The default matches 80."],
            [
              <Code>--video-codec hevc</Code>,
              "HEVC instead of H.264: smaller at the same quality, but slower to encode.",
            ],
            [<Code>--audio-bitrate 96</Code>, "Audio at 96 kbit/s instead of 192."],
            [<Code>--no-audio</Code>, "Leaves the sound out."],
          ]}
        />
        <h3 className="mt-4 text-[19px]/7 font-semibold tracking-[-0.01em]">
          What each option did to our 14.7 MB test clip
        </h3>
        <FactList facts={smallerResults} />
        <Prose>
          <p className="text-[14px]/[22px] text-ink-2">
            Our clip was a generated test pattern, which compresses differently from camera footage.
            Use the numbers to compare options, not to predict your file sizes.
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

      <GuideSection
        id="windows"
        title="Convert MOV to MP4 on Windows"
        source={
          <Checked how="tested">
            The commands are the ones we ran on Linux, with Windows paths. We haven't run them on
            Windows.
          </Checked>
        }
      >
        <Prose>
          <p>
            The FFmpeg commands above work the same on Windows. With convt installed, its
            command-line tool sits in your user folder and isn't on your PATH, so give its full path
            in PowerShell:
          </p>
        </Prose>
        <Commands
          label="PowerShell"
          commands={[
            String.raw`& "$env:LOCALAPPDATA\Programs\convt\convt.exe" Videos --to mp4 -r -o mp4`,
          ]}
        />
        <Prose>
          <p>
            Or open convt from the Start menu, drag the MOV files onto its window and choose MP4.
            convt's right-click menu in File Explorer isn't part of the Windows installer yet.
          </p>
        </Prose>
      </GuideSection>

      <GuideSection id="faq" title="Questions">
        <dl className="flex flex-col gap-6">
          <div className="flex flex-col gap-1.5">
            <dt className="text-[16px]/6 font-semibold">
              Does converting MOV to MP4 lose quality?
            </dt>
            <dd className="text-[16px]/[26px] text-ink-2">
              Not if you remux: the video is copied, not encoded again. Re-encoding loses a little,
              and at sensible settings it's hard to see. Converting the same file over and over adds
              up, so always start from the original.
            </dd>
          </div>
          <div className="flex flex-col gap-1.5">
            <dt className="text-[16px]/6 font-semibold">Is MP4 smaller than MOV?</dt>
            <dd className="text-[16px]/[26px] text-ink-2">
              Not by itself. A remux is the same size, because the video is the same. A re-encode is
              smaller or bigger depending on the settings: our ProRes clip went from 59.5 MB to 6.4
              MB, while an already-compressed H.264 clip barely changed until we lowered the
              resolution or quality.
            </dd>
          </div>
          <div className="flex flex-col gap-1.5">
            <dt className="text-[16px]/6 font-semibold">Why won't my MOV remux to MP4?</dt>
            <dd className="text-[16px]/[26px] text-ink-2">
              It holds a codec MP4 can't carry, most often ProRes from a camera or a screen
              recording app. Re-encode it with FFmpeg or convt.
            </dd>
          </div>
          <div className="flex flex-col gap-1.5">
            <dt className="text-[16px]/6 font-semibold">Does convt upload my videos?</dt>
            <dd className="text-[16px]/[26px] text-ink-2">
              Not for local conversions. The app and the command-line tool convert on your computer.
              The app uploads a file only if you pick Cloud for a job, which is part of convt Pro.
              See every format it handles on the{" "}
              <TextLink href={routes.formats}>formats page</TextLink>.
            </dd>
          </div>
        </dl>
      </GuideSection>
    </GuideArticle>
  );
}
