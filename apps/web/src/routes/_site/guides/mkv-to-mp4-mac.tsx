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

// Every FFmpeg command and result on this page comes from a run of the FFmpeg 7.0.2 that the
// released convt 0.5.0 Linux build bundles, on 8-second MKV test files: H.264 + AAC + SRT
// subtitles, H.264 + AAC + styled ASS subtitles (with and without an attached font),
// H.264 + FLAC, HEVC + two AC-3 tracks (English and Japanese) and VP9 + Opus (CNVM-23).
// The convt results come from running convt 0.5.0's own MP4 arguments (crates/convt-engines/
// src/ffmpeg.rs at v0.5.0) with that FFmpeg; the folder command is the one the MOV guide ran
// on 0.5.0. The HandBrake steps come from HandBrake's documentation and say so on the page.

const slug = "mkv-to-mp4-mac";
const guide = guideBySlug(slug);

const remuxCommand =
  'ffmpeg -i input.mkv -map 0:v -map 0:a -map "0:s?" -c copy -c:s mov_text -movflags +faststart output.mp4';

const sections = [
  { id: "why", title: "Why MKV won't open" },
  { id: "pick", title: "Pick a method" },
  { id: "ffmpeg", title: "FFmpeg: remux every track" },
  { id: "handbrake", title: "HandBrake" },
  { id: "convt", title: "Whole folders with convt" },
  { id: "windows", title: "On Windows" },
  { id: "faq", title: "Questions" },
];

const methods = [
  {
    name: "FFmpeg remux",
    bestFor: "Keeping quality, every audio track and subtitles",
    cost: "Free, Terminal",
    checked: "Run on test files",
  },
  {
    name: "HandBrake",
    bestFor: "One video at a time in an app, with presets",
    cost: "Free, open source",
    checked: "HandBrake's docs",
  },
  {
    name: "convt",
    bestFor: "Folders with subfolders, any codec in",
    cost: "$39 once",
    checked: "Its FFmpeg arguments run",
  },
];

const plainCopyResults: [string, string][] = [
  [
    "H.264, AAC, SRT subtitles",
    "Video and audio copied. The subtitle track was left out without a warning.",
  ],
  [
    "HEVC, English and Japanese AC-3",
    "Only the English track came across. The Japanese one was left out.",
  ],
  [
    "H.264, FLAC",
    "Copied as is: an MP4 with FLAC audio. If an app plays it without sound, convert the audio to AAC.",
  ],
  [
    "VP9, Opus",
    "Copied as is: an MP4 with VP9 video and Opus audio. If an app won't open it, re-encode it.",
  ],
];

const keepAllResults: [string, string][] = [
  [
    "H.264, AAC, SRT subtitles",
    "All three tracks kept. The subtitles became MP4 text subtitles (mov_text). 8.6 MB in, 8.6 MB out.",
  ],
  [
    "H.264, AAC, styled ASS subtitles",
    "All tracks kept, but the subtitles lost their fonts, colors and positions: MP4 text subtitles are plain.",
  ],
  ["HEVC, two AC-3 tracks", "Both language tracks kept, with their language tags."],
  [
    "ASS subtitles plus an attached font",
    "Worked with the command above. With -map 0 instead, FFmpeg stopped because MP4 can't hold the font file.",
  ],
];

export const Route = createFileRoute("/_site/guides/mkv-to-mp4-mac")({
  head: () => guideHead(slug),
  component: MkvToMp4Mac,
});

function MkvToMp4Mac() {
  return (
    <GuideArticle
      guide={guide}
      sections={sections}
      lead={
        <p>
          MKV and MP4 are both containers. The video inside an MKV is usually H.264 or HEVC, which
          MP4 can hold as it is, so most MKV files become MP4 in seconds without re-encoding a
          frame. The catch is the extras MKV files carry: several audio languages and subtitle
          tracks, which a careless conversion quietly drops.
        </p>
      }
    >
      <div className="flex flex-col gap-3 rounded-2xl bg-green-tint/60 px-5 py-4 shadow-[inset_0_0_0_1px_var(--green-line)]">
        <p className="font-mono text-[11px]/4 text-ink-2 uppercase">The short version</p>
        <p className="text-[16px]/[26px]">
          With FFmpeg installed, this copies the video, every audio track and every text subtitle
          into an MP4, with no quality loss:
        </p>
        <Commands label="FFmpeg" commands={[remuxCommand]} />
        <p className="text-[16px]/[26px]">
          If the video is HEVC, add <Code>-tag:v hvc1</Code> before the output name so Apple's apps
          recognise it. If FFmpeg stops with an error about a codec or subtitles, see{" "}
          <TextLink href="#ffmpeg">the FFmpeg section</TextLink>.
        </p>
      </div>

      <GuideSection
        id="why"
        title="Why an MKV won't open on a Mac"
        source={
          <Checked how="docs">
            From Apple's support article{" "}
            <TextLink href="https://support.apple.com/en-us/102644">
              If a media file doesn't open or play on your Apple device
            </TextLink>
            .
          </Checked>
        }
      >
        <Prose>
          <p>
            Apple's apps work with many media formats, but Apple names <Code>.mkv</Code> as an
            example of a file that might need a different app. Finder shows such a file as{" "}
            <q>Matroska Video File</q> in its Info window (select it and press Command-I). Turning
            it into an MP4 lets QuickTime Player, Photos, iMovie and most upload forms take it.
          </p>
        </Prose>
      </GuideSection>

      <GuideSection id="pick" title="Pick a method">
        <ul className="divide-y divide-divider overflow-hidden rounded-2xl bg-raised shadow-[inset_0_0_0_1px_var(--line)] dark:bg-panel">
          {methods.map((m) => (
            <li
              key={m.name}
              className="flex flex-col gap-1 px-4 py-3.5 text-[14px]/5 md:grid md:grid-cols-[150px_1fr_150px_170px] md:items-baseline md:gap-4"
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
            Renaming an MKV to <Code>.mp4</Code> doesn't convert it: the file underneath is still
            Matroska, so apps that can't read MKV still can't open it.
          </p>
        </Prose>
      </GuideSection>

      <GuideSection
        id="ffmpeg"
        title="Convert MKV to MP4 with FFmpeg"
        source={
          <Checked how="tested">
            Run with FFmpeg 7.0.2 on 8-second MKV files with H.264, HEVC and VP9 video, AAC, AC-3,
            FLAC and Opus audio, and SRT and ASS subtitles.
          </Checked>
        }
      >
        <Prose>
          <p>
            FFmpeg is the free command-line tool most video apps are built on. Install it with{" "}
            <TextLink href="https://brew.sh">Homebrew</TextLink> (<Code>brew install ffmpeg</Code>),
            then open Terminal in the folder with your MKV.
          </p>
        </Prose>

        <h3 className="mt-4 text-[19px]/7 font-semibold tracking-[-0.01em]">
          Why the usual command loses tracks
        </h3>
        <Prose>
          <p>
            Most answers online give <Code>ffmpeg -i input.mkv -c copy output.mp4</Code>. It runs
            without errors, but FFmpeg only takes one video and one audio track unless you tell it
            otherwise. Here's what it made from our test files:
          </p>
        </Prose>
        <FactList facts={plainCopyResults} />

        <h3 className="mt-4 text-[19px]/7 font-semibold tracking-[-0.01em]">
          Keep every audio track and subtitle
        </h3>
        <Commands label="FFmpeg" commands={[remuxCommand]} />
        <FactList
          facts={[
            [<Code>-map 0:v -map 0:a</Code>, "Takes every video and audio track, not just one."],
            [
              <Code>-map "0:s?"</Code>,
              "Takes every subtitle track. The ? means files without subtitles still work, and the quotes stop the Mac's zsh shell from reading ? as a filename pattern.",
            ],
            [<Code>-c copy</Code>, "Copies the video and audio as they are: no quality loss."],
            [
              <Code>-c:s mov_text</Code>,
              "Turns SRT or ASS subtitles into MP4's own text subtitles, which MP4 can hold.",
            ],
            [
              <Code>-movflags +faststart</Code>,
              "Puts the index first, so the MP4 starts playing in a browser before it's fully loaded.",
            ],
          ]}
        />
        <Prose>
          <p>
            The command maps video, audio and subtitles on purpose, not everything (
            <Code>-map 0</Code>). MKV files with styled subtitles often carry their fonts as
            attachments, and MP4 has nowhere to put them.
          </p>
        </Prose>
        <h3 className="mt-4 text-[19px]/7 font-semibold tracking-[-0.01em]">
          What came out of our test
        </h3>
        <FactList facts={keepAllResults} />
        <Prose>
          <p>
            Each remux took a fraction of a second, and the MP4s were the same size as the MKVs give
            or take a few kilobytes.
          </p>
        </Prose>

        <h3 className="mt-4 text-[19px]/7 font-semibold tracking-[-0.01em]">
          HEVC: add the hvc1 tag
        </h3>
        <Commands
          label="FFmpeg"
          commands={[
            'ffmpeg -i input.mkv -map 0:v -map 0:a -map "0:s?" -c copy -c:s mov_text -tag:v hvc1 -movflags +faststart output.mp4',
          ]}
        />
        <Prose>
          <p>
            FFmpeg copies HEVC video into MP4 with the tag <Code>hev1</Code>. QuickTime and Apple
            devices only play HEVC tagged <Code>hvc1</Code>, so add <Code>-tag:v hvc1</Code>. Use it
            only for HEVC: on an H.264 video it would label the stream wrongly.
          </p>
        </Prose>

        <h3 className="mt-4 text-[19px]/7 font-semibold tracking-[-0.01em]">
          FLAC, Opus or AC-3 audio: convert just the sound
        </h3>
        <Commands
          label="FFmpeg"
          commands={[
            'ffmpeg -i input.mkv -map 0:v -map 0:a -map "0:s?" -c:v copy -c:a aac -b:a 192k -c:s mov_text -movflags +faststart output.mp4',
          ]}
        />
        <Prose>
          <p>
            MP4 can hold FLAC, Opus and AC-3, and FFmpeg copies them without complaint, but plenty
            of players, editors and upload forms expect AAC. This copies the video untouched and
            encodes only the audio, which takes seconds.
          </p>
        </Prose>

        <h3 className="mt-4 text-[19px]/7 font-semibold tracking-[-0.01em]">
          VP9 or AV1 video: re-encode it
        </h3>
        <Commands
          label="FFmpeg"
          commands={[
            'ffmpeg -i input.mkv -map 0:v:0 -map 0:a -map "0:s?" -c:v libx264 -crf 20 -preset medium -pix_fmt yuv420p -c:a aac -b:a 192k -c:s mov_text -movflags +faststart output.mp4',
          ]}
        />
        <Prose>
          <p>
            MKVs saved from the web often hold VP9 or AV1 video. A remux keeps it, and not every
            player or editor opens that inside an MP4. This makes new H.264 video, which plays
            almost everywhere. <Code>-crf 20</Code> sets the quality: lower looks better and makes
            bigger files. <Code>-map 0:v:0</Code> takes only the first video track, so a cover image
            some MKVs carry isn't encoded as a second video (FFmpeg reads it as one).
          </p>
        </Prose>

        <h3 className="mt-4 text-[19px]/7 font-semibold tracking-[-0.01em]">
          Subtitles that won't convert
        </h3>
        <Prose>
          <p>
            Subtitles ripped from a Blu-ray or DVD are pictures, not text, and can't become MP4 text
            subtitles. FFmpeg then stops with this error:
          </p>
        </Prose>
        <Output label="FFmpeg error">
          Subtitle encoding currently only possible from text to text or bitmap to bitmap
        </Output>
        <Prose>
          <p>
            Replace <Code>-map "0:s?" -c:s mov_text</Code> with <Code>-sn</Code> to leave the
            subtitles out, or keep the MKV for watching with subtitles.
          </p>
        </Prose>

        <h3 className="mt-4 text-[19px]/7 font-semibold tracking-[-0.01em]">
          Every MKV in a folder
        </h3>
        <Prose>
          <p>
            To remux every MKV in the current folder, whether it ends in .mkv or .MKV, run this in
            Terminal:
          </p>
        </Prose>
        <Commands
          label="FFmpeg"
          commands={[
            `find . -maxdepth 1 -iname "*.mkv" -exec sh -c 'ffmpeg -i "$1" -map 0:v -map 0:a -map "0:s?" -c copy -c:s mov_text -movflags +faststart "\${1%.*}.mp4"' _ {} \\;`,
          ]}
        />
        <Prose>
          <p>
            It skips subfolders and stops on any file whose codecs or subtitles need the fixes
            above. For a mixed folder, convt below re-encodes whatever it finds.
          </p>
        </Prose>
      </GuideSection>

      <GuideSection
        id="handbrake"
        title="Convert MKV to MP4 with HandBrake"
        source={
          <Checked how="docs">
            From HandBrake's{" "}
            <TextLink href="https://handbrake.fr/docs/en/latest/introduction/quick-start.html">
              quick start
            </TextLink>
            . We haven't run it ourselves.
          </Checked>
        }
      >
        <Steps>
          {[
            <>
              Open HandBrake and click <strong className="font-semibold">Open Source</strong>, or
              drag the MKV onto its window.
            </>,
            <>
              Pick a preset. HandBrake's docs suggest the default{" "}
              <strong className="font-semibold">Fast 1080p30</strong> as fast and compatible with
              most devices.
            </>,
            <>
              Check that the format on the Summary tab is MP4, and choose where to save the new
              video.
            </>,
            <>
              Click <strong className="font-semibold">Start</strong>.
            </>,
          ]}
        </Steps>
        <Prose>
          <p>
            HandBrake is free and open source. It always re-encodes the video, so it takes far
            longer than a remux and the copy is a new encode, but it fixes any codec in one go.
            Check its Audio and Subtitles tabs if you need more than one language: a preset decides
            which tracks it takes.
          </p>
        </Prose>
      </GuideSection>

      <GuideSection
        id="convt"
        title="Convert whole folders with convt"
        source={
          <Checked how="tested">
            We ran the FFmpeg arguments convt 0.5.0 uses for MP4, taken from its source, with the
            FFmpeg it ships, on the same test files. The folder command is the one we ran with the
            0.5.0 command-line tool for the{" "}
            <TextLink href={guidePath("mov-to-mp4-mac")}>MOV guide</TextLink>.
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
        <Prose>
          <p>
            <Code>-r</Code> includes subfolders and <Code>-o mp4</Code> rebuilds the same folders
            under <Code>mp4</Code>. Leave out <Code>-o</Code> and each MP4 is written next to its
            MKV. convt converts videos one at a time.
          </p>
          <p>
            convt 0.5.0 always re-encodes to H.264 video and AAC audio, so VP9, HEVC, FLAC and Opus
            files all come out as MP4s that play everywhere. It's slower than a remux, and it keeps
            less:
          </p>
        </Prose>
        <FactList
          facts={[
            [
              "Video",
              "H.264 at CRF 20 (the x264 encoder's medium preset), same resolution and length, from every codec we tried.",
            ],
            [
              "Audio",
              "One track, as AAC at 192 kbit/s: FFmpeg's default pick, the track with the most channels. Our second language track was left out.",
            ],
            ["Subtitles", "Left out."],
          ]}
        />
        <Prose>
          <p>
            So for a film with several languages or subtitles, use the FFmpeg remux above. For a
            folder of ordinary videos in mixed codecs, convt does it in one command. To make the
            MP4s smaller, add <Code>--video-height 720</Code>, <Code>-q 60</Code> or{" "}
            <Code>--video-codec hevc</Code>; the{" "}
            <TextLink href={`${guidePath("mov-to-mp4-mac")}#smaller`}>MOV guide</TextLink> shows
            what each did to a test clip.
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
            Open convt once, then turn on its Finder extension in System Settings › General › Login
            Items & Extensions › Finder. After that, Control-click your MKV files, choose{" "}
            <strong className="font-semibold">Convert with convt</strong>, then MP4.
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

      <GuideSection
        id="windows"
        title="Convert MKV to MP4 on Windows"
        source={
          <Checked how="tested">
            The commands are the ones we ran on Linux, with Windows paths. We haven't run them on
            Windows.
          </Checked>
        }
      >
        <Prose>
          <p>
            The single-file FFmpeg commands above work the same in PowerShell. With convt installed,
            its command-line tool sits in your user folder and isn't on your PATH, so give its full
            path:
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
            Or open convt from the Start menu, drag the MKV files onto its window and choose MP4.
            convt's right-click menu in File Explorer isn't part of the Windows installer yet.
          </p>
        </Prose>
      </GuideSection>

      <GuideSection id="faq" title="Questions">
        <dl className="flex flex-col gap-6">
          <div className="flex flex-col gap-1.5">
            <dt className="text-[16px]/6 font-semibold">
              Does converting MKV to MP4 lose quality?
            </dt>
            <dd className="text-[16px]/[26px] text-ink-2">
              Not with a remux: the video and audio are copied, not encoded again, and the file
              stays about the same size. Re-encoding, which HandBrake and convt do, loses a little,
              and at sensible settings it's hard to see.
            </dd>
          </div>
          <div className="flex flex-col gap-1.5">
            <dt className="text-[16px]/6 font-semibold">
              Why did my subtitles disappear after converting?
            </dt>
            <dd className="text-[16px]/[26px] text-ink-2">
              A plain <Code>ffmpeg -i input.mkv -c copy output.mp4</Code> leaves them out, and so
              does convt 0.5.0. Use the command at the top of this page, which maps the video, audio
              and subtitles. Picture subtitles from discs can't be converted to MP4 text subtitles
              at all.
            </dd>
          </div>
          <div className="flex flex-col gap-1.5">
            <dt className="text-[16px]/6 font-semibold">Why does my MP4 only have one language?</dt>
            <dd className="text-[16px]/[26px] text-ink-2">
              FFmpeg picks a single audio track unless you map them yourself with{" "}
              <Code>-map 0:v -map 0:a</Code>. With it, every language track comes across with its
              language tag, and players that support MP4 audio tracks let you switch between them.
            </dd>
          </div>
          <div className="flex flex-col gap-1.5">
            <dt className="text-[16px]/6 font-semibold">Is MP4 better than MKV?</dt>
            <dd className="text-[16px]/[26px] text-ink-2">
              Not in quality: it's the same video in a different container. MKV holds more kinds of
              tracks, such as styled subtitles and fonts. MP4 plays in more places, including
              Apple's apps, browsers and most upload forms.
            </dd>
          </div>
          <div className="flex flex-col gap-1.5">
            <dt className="text-[16px]/6 font-semibold">Does convt upload my videos?</dt>
            <dd className="text-[16px]/[26px] text-ink-2">
              Not for local conversions. The app and the command-line tool convert on your computer.
              The app uploads a file only if you pick Cloud for a job, which is part of Desktop +
              Cloud. See every format it handles on the{" "}
              <TextLink href={routes.formats}>formats page</TextLink>.
            </dd>
          </div>
        </dl>
      </GuideSection>
    </GuideArticle>
  );
}
