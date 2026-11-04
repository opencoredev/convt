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
// Linux CLI (CNVM-11) on files we made with FFmpeg: tagged FLAC tracks (one with embedded cover
// art) in a subfolder, a 48 kHz WAV, an H.264/AAC MP4 and an MP3 already in the folder. convt
// encodes MP3 with FFmpeg's LAME encoder, which ships in the Mac and Windows installs too.
// The QuickTime and Music steps come from Apple's guides and say so on the page.

const slug = "mp4-flac-wav-to-mp3-mac";
const guide = guideBySlug(slug);

const convtExe = String.raw`& "$env:LOCALAPPDATA\Programs\convt\convt.exe"`;

const sections = [
  { id: "built-in", title: "With QuickTime and Music" },
  { id: "convt", title: "Any file with convt" },
  { id: "folder", title: "A whole folder" },
  { id: "quality", title: "Bitrate and quality" },
  { id: "faq", title: "Questions" },
];

const results: [string, string][] = [
  ["Tags", "Title, artist and album carried over from the FLAC files into the MP3s."],
  ["Cover art", "Dropped. A FLAC with an embedded cover came out as an MP3 without one."],
  ["Sample rate", "Kept: 44.1 kHz FLAC stayed 44.1 kHz, a 48 kHz WAV stayed 48 kHz."],
  ["MP4 video", "Only the sound track was written. The video is left out."],
  ["Existing MP3s", "An MP3 already in the folder was skipped, not converted again."],
  [
    "Name clashes",
    "Running the same command twice wrote talk (1).mp3 next to talk.mp3. It doesn't overwrite the first one.",
  ],
];

export const Route = createFileRoute("/_site/guides/mp4-flac-wav-to-mp3-mac")({
  head: () => guideHead(slug),
  component: Mp4FlacWavToMp3Mac,
});

function Mp4FlacWavToMp3Mac() {
  return (
    <GuideArticle
      guide={guide}
      sections={sections}
      lead={
        <p>
          Old car stereos, some podcast hosts and many upload forms still want MP3. A Mac can make
          one with QuickTime and the Music app, but it takes two apps for a video and Music won't
          open FLAC at all. convt turns an MP4, FLAC or WAV file, or a folder full of them, into MP3
          with one command, on your Mac, without uploading anything.
        </p>
      }
    >
      <div className="flex flex-col gap-3 rounded-2xl bg-green-tint/60 px-5 py-4 shadow-[inset_0_0_0_1px_var(--green-line)]">
        <p className="font-mono text-[11px]/4 text-ink-2 uppercase">The short version</p>
        <p className="text-[16px]/[26px]">
          <strong className="font-semibold">WAV:</strong> in Music, set Import Using to MP3 Encoder,
          select the song and choose File › Convert › Create MP3 Version.{" "}
          <strong className="font-semibold">MP4:</strong> export the sound from QuickTime first
          (File › Export As › Audio Only), then do the same.{" "}
          <strong className="font-semibold">FLAC, or a whole folder:</strong>{" "}
          <Code>convt Music --to mp3 -r -o mp3</Code>.
        </p>
      </div>

      <GuideSection
        id="built-in"
        title="Convert to MP3 with QuickTime and the Music app"
        source={
          <Checked how="docs">
            From Apple's guides{" "}
            <TextLink href="https://support.apple.com/en-us/108961">
              Convert a song to a different file format
            </TextLink>
            ,{" "}
            <TextLink href="https://support.apple.com/en-us/120493">
              Convert an M4A file to MP3 on Mac
            </TextLink>{" "}
            and{" "}
            <TextLink href="https://support.apple.com/guide/quicktime-player/export-movies-qtp20e395859/mac">
              Export movies in QuickTime Player
            </TextLink>
            . We haven't run them ourselves.
          </Checked>
        }
      >
        <Prose>
          <p>
            The Music app has an MP3 encoder. It works on songs in your library, so a WAV or M4A
            file goes in first and the MP3 copy lands next to it:
          </p>
        </Prose>
        <Steps>
          {[
            <>
              Open Music and choose <strong className="font-semibold">Music › Settings</strong>.
              Click <strong className="font-semibold">Files</strong>, then{" "}
              <strong className="font-semibold">Import Settings</strong>.
            </>,
            <>
              Set <strong className="font-semibold">Import Using</strong> to{" "}
              <strong className="font-semibold">MP3 Encoder</strong> and click OK.
            </>,
            <>Drag the WAV or M4A file from Finder into Music, then select it.</>,
            <>
              Choose <strong className="font-semibold">File › Convert › Create MP3 Version</strong>.
              To find the new file, select it and choose{" "}
              <strong className="font-semibold">File › Show in Finder</strong>.
            </>,
          ]}
        </Steps>
        <Prose>
          <p>
            <strong className="font-semibold">From an MP4 video:</strong> open it in QuickTime
            Player and choose{" "}
            <strong className="font-semibold">File › Export As › Audio Only</strong>. QuickTime
            saves an M4A (AAC) file, not an MP3, so run that M4A through the Music steps above.
          </p>
          <p>
            <strong className="font-semibold">From FLAC:</strong> Apple's guides list MP3, AAC,
            Apple Lossless, AIFF and WAV for Music. FLAC isn't one of them, and Music doesn't import
            FLAC files. Use <TextLink href="#convt">convt</TextLink> or another converter for those.
          </p>
          <p>
            Every converted song also stays in your Music library, and a folder of files means
            importing all of them. For more than a few files, the next section is faster.
          </p>
        </Prose>
      </GuideSection>

      <GuideSection
        id="convt"
        title="Convert MP4, FLAC or WAV to MP3 with convt"
        source={
          <Checked how="tested">
            Run with the released convt 0.5.0 command-line tool on Linux, on files we made. convt
            encodes MP3 with the same FFmpeg and LAME encoder in the Mac and Windows installs.
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
            In Terminal, go to the folder with the file and run one of these. The same{" "}
            <Code>--to mp3</Code> works for a video, a FLAC or a WAV. If you installed from the disk
            image, type <Code>/Applications/convt.app/Contents/MacOS/convt</Code> instead of{" "}
            <Code>convt</Code>.
          </p>
        </Prose>
        <Commands
          label="convt"
          commands={[
            "convt talk.mp4 --to mp3",
            "convt 01-opening.flac --to mp3",
            "convt demo.wav --to mp3",
          ]}
        />
        <Output label="Output">{`talk.mp4 -> ./talk.mp3`}</Output>
        <Prose>
          <p>
            The MP3 is written next to the original, which stays as it was. From an MP4, convt keeps
            only the sound. MOV, MKV, M4A, AAC, OGG and the other audio and video formats on the{" "}
            <TextLink href={routes.formats}>formats page</TextLink> convert the same way.
          </p>
        </Prose>
      </GuideSection>

      <GuideSection id="folder" title="Convert a whole folder to MP3">
        <Prose>
          <p>
            Give convt a folder instead of a file. <Code>-r</Code> includes subfolders and{" "}
            <Code>-o mp3</Code> writes the MP3s into a new folder with the same subfolders, so an
            album folder stays an album folder:
          </p>
        </Prose>
        <Commands label="convt" commands={["convt Music --to mp3 -r -o mp3"]} />
        <Output label="Output">{`Music/Live/02-encore.flac -> mp3/Live/02-encore.mp3
Music/Live/01-opening.flac -> mp3/Live/01-opening.mp3
Music/talk.mp4 -> mp3/talk.mp3
Music/demo.wav -> mp3/demo.mp3
4 converted, 0 failed`}</Output>
        <Prose>
          <p>
            A folder input converts every file convt can turn into MP3, so videos in the folder get
            an MP3 of their sound too. Files that are already MP3 are skipped.
          </p>
        </Prose>

        <h3 className="mt-4 text-[19px]/7 font-semibold tracking-[-0.01em]">
          What came out of our test
        </h3>
        <FactList facts={results} />
      </GuideSection>

      <GuideSection id="quality" title="Pick the bitrate">
        <Prose>
          <p>
            By default convt uses LAME's variable bitrate at quality 2 (<Code>-q:a 2</Code> in
            FFmpeg terms), which{" "}
            <TextLink href="https://trac.ffmpeg.org/wiki/Encode/MP3">FFmpeg's MP3 guide</TextLink>{" "}
            puts at about 170 to 210 kbit/s for music. The bitrate goes down on quiet or simple
            sound. To pick a fixed bitrate instead, add <Code>--audio-bitrate</Code>:
          </p>
        </Prose>
        <FactList
          facts={[
            [
              <Code>--audio-bitrate 320</Code>,
              "The highest MP3 allows, for music you want to keep. Higher numbers are capped at 320.",
            ],
            [
              <Code>--audio-bitrate 192</Code>,
              "A common setting for music players and car stereos.",
            ],
            [
              <Code>--audio-bitrate 128</Code>,
              "Fine for speech: lectures, meetings, voice memos and podcasts.",
            ],
          ]}
        />
        <Commands
          label="convt"
          commands={[
            "convt demo.wav --to mp3 --audio-bitrate 320",
            "convt Music --to mp3 -r -o mp3 --audio-bitrate 192",
          ]}
        />
        <Prose>
          <p>
            In our test, <Code>--audio-bitrate 320</Code> gave a 321 kbit/s file and{" "}
            <Code>128</Code> gave 129 kbit/s. MP3 is lossy: converting a FLAC or WAV to MP3 throws
            away detail you can't get back, so keep the originals.
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
            <dt className="text-[16px]/6 font-semibold">
              Does it keep my song tags and album art?
            </dt>
            <dd className="text-[16px]/[26px] text-ink-2">
              Tags yes, album art no. Title, artist and album came through from FLAC to MP3 in our
              test, but an embedded cover was dropped. If the art matters, add it back in Music
              afterwards.
            </dd>
          </div>
          <div className="flex flex-col gap-1.5">
            <dt className="text-[16px]/6 font-semibold">Does this work on Windows?</dt>
            <dd className="text-[16px]/[26px] text-ink-2">
              Yes, with the same options. The Windows installer doesn't add convt to your PATH, so
              in PowerShell type its full path:{" "}
              <Code>{String.raw`${convtExe} Music --to mp3 -r -o mp3`}</Code>.
            </dd>
          </div>
          <div className="flex flex-col gap-1.5">
            <dt className="text-[16px]/6 font-semibold">Should I pick MP3 or M4A?</dt>
            <dd className="text-[16px]/[26px] text-ink-2">
              M4A (AAC) usually sounds as good as MP3 at a lower bitrate, and Apple's apps play it.
              Pick MP3 when a device or website asks for it, since almost everything plays MP3.
              convt makes either: <Code>--to m4a</Code> instead of <Code>--to mp3</Code>.
            </dd>
          </div>
          <div className="flex flex-col gap-1.5">
            <dt className="text-[16px]/6 font-semibold">Does convt upload my files?</dt>
            <dd className="text-[16px]/[26px] text-ink-2">
              Not for local conversions. The app and the command-line tool convert on your computer.
              The app uploads a file only if you pick Cloud for a job, which is part of convt Pro.
              For video, see{" "}
              <TextLink href={guidePath("mov-to-mp4-mac")}>MOV to MP4 on a Mac</TextLink>.
            </dd>
          </div>
        </dl>
      </GuideSection>
    </GuideArticle>
  );
}
