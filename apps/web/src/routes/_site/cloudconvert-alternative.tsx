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
import { guidePath } from "#/lib/guides";
import {
  cloudconvertAlternative,
  cloudconvertAlternativeDownloadUrl,
  cloudconvertAlternativeHead,
  commandLineConverter,
  handbrakeAlternative,
  macFileConverter,
} from "#/lib/landing-pages";
import { routes } from "#/lib/site";

// A comparison page for "cloudconvert alternative(s)". Every CloudConvert fact (free tier,
// credits, prices, formats, how it handles files) was read on cloudconvert.com/pricing,
// /security and the homepage on 2026-11-25; the other tools' facts come from their own pricing
// pages, sites or READMEs on the same day and are linked from each block. Every convt command and
// output here was run on the released convt 0.5.0 Linux CLI for earlier pages (the command-line
// page and the HandBrake page). The cloud API facts come from the live /api page.

const CHECKED = "November 25, 2026";

const page = cloudconvertAlternative;
const downloadUrl = cloudconvertAlternativeDownloadUrl;

const ccPricing = "https://cloudconvert.com/pricing";
const ccSecurity = "https://cloudconvert.com/security";
const ccPrivacy = "https://cloudconvert.com/privacy";

const sections = [
  { id: "compare", title: "Side by side" },
  { id: "offline", title: "Offline alternatives" },
  { id: "switch", title: "When to switch" },
  { id: "tools", title: "The alternatives" },
  { id: "cloudconvert-better", title: "Where CloudConvert is better" },
  { id: "convt", title: "The same jobs in convt" },
  { id: "api", title: "If you use the API" },
  { id: "faq", title: "Questions" },
];

const rows: [string, string, string, string][] = [
  [
    "CloudConvert",
    "Free for 10 conversion credits a day; 1,000 credits for $17, or $9 a month",
    "Any browser, plus an API",
    "Uploaded to its servers",
  ],
  [
    "convt Desktop 0.5.0",
    "$39 once",
    "Mac (Apple silicon), Windows, Linux",
    "Local conversions stay on your computer",
  ],
  [
    "VERT",
    "Free, open source",
    "Any browser",
    "Converted in your browser tab; some video goes to VERT's server",
  ],
  ["File Converter 2.2", "Free, open source", "Windows only", "Stay on your computer"],
  [
    "Convertio",
    "Free for 10 credits a day; paid from $6.99 a month, billed yearly",
    "Any browser, plus an API",
    "Uploaded to its servers",
  ],
  [
    "FreeConvert",
    "Free for 20 conversion minutes a day; paid from $12.99 a month",
    "Any browser, plus an API",
    "Uploaded to its servers",
  ],
];

type Tool = {
  name: string;
  href: string;
  bestFor: ReactNode;
  price: ReactNode;
  platforms: ReactNode;
  files: ReactNode;
  batch: ReactNode;
  shortfalls: ReactNode;
};

const tools: Tool[] = [
  {
    name: "convt",
    href: "/",
    bestFor:
      "Converting video, audio, images and documents on your own computer, from Finder's right-click menu or one command over a folder.",
    price:
      "$39 once for Desktop, with every future desktop update. Desktop + Cloud is $69 once and adds a year of cloud conversions. No free tier.",
    platforms: "macOS 13 or later on Apple silicon, Windows x86_64, Linux x86_64.",
    files: (
      <>
        Local conversions never leave your computer. Desktop + Cloud adds cloud conversions, from
        the app or from a phone or browser, and those upload the file. Version 0.5.0 converts MP4,
        MOV, WebM, MKV and other video, seven audio formats, a dozen image formats, PDFs and Office
        documents; the <TextLink href={routes.formats}>current format list</TextLink> may include
        newer ones.
      </>
    ),
    batch: (
      <>
        A folder in one command, subfolders with <Code>-r</Code>; a Convert with convt menu in
        Finder and in Linux file managers. The Windows Explorer menu isn&apos;t in the installer
        yet.
      </>
    ),
    shortfalls:
      "No free tier. Converting from a phone or browser needs Desktop + Cloud and uploads the file. Far fewer formats than CloudConvert: no ebooks, archives, CAD drawings or camera RAW files in 0.5.0.",
  },
  {
    name: "VERT",
    href: "https://vert.sh/",
    bestFor: "A free converter in the browser that doesn't upload your images, audio or documents.",
    price: "Free. Open source under AGPL-3.0.",
    platforms: "Any modern browser, on any system. You can also host it yourself.",
    files: (
      <>
        Converts on your device with WebAssembly. Its{" "}
        <TextLink href="https://github.com/VERT-sh/VERT">README</TextLink> says some video
        conversions need server-side processing, which the official site does on its own server
        unless you host its video daemon yourself.
      </>
    ),
    batch:
      "No limit on the number or size of files, per its README; you add them in the browser tab.",
    shortfalls:
      "Video can leave your device on the official site, and work happens in a browser tab, so there is no right-click menu or command line.",
  },
  {
    name: "File Converter",
    href: "https://file-converter.io/",
    bestFor: "Windows users who want a free right-click menu in File Explorer.",
    price: "Free, with no ads or data collection. Open source under GPL-3.0.",
    platforms: "Windows 7 to 11, 64-bit. No Mac or Linux build.",
    files: (
      <>
        Converts on your computer. It writes MP3, FLAC, AAC, OGG and WAV; MP4, MKV, WebM and GIF;
        PNG, JPG, WebP and AVIF; and PDF. Word, Excel and PowerPoint files need Microsoft Office
        installed and activated.
      </>
    ),
    batch: "Converts the files you select in File Explorer, from the context menu.",
    shortfalls: "Windows only, and Office conversions depend on a paid Office install.",
  },
  {
    name: "Convertio",
    href: "https://convertio.co/",
    bestFor: "People who want a browser converter that feels like CloudConvert.",
    price: (
      <>
        Free for 10 credits per 24 hours, files up to 1 GB and 10 conversions at once, per its{" "}
        <TextLink href="https://support.convertio.co/hc/en-us/articles/360004386774-Free-tier-limit-for-file-conversions">
          help center
        </TextLink>
        . Paid web plans start at $6.99 a month billed yearly ($11.99 month to month) for 1,000
        credits, per its <TextLink href="https://convertio.co/pricing/">pricing page</TextLink>. The
        API is priced separately.
      </>
    ),
    platforms: "Any browser, plus an API.",
    files: "Uploaded to its servers over SSL. It says output files are bound to your IP address.",
    batch: "Up to 50 conversions at once on the cheapest paid plan.",
    shortfalls: "Every file leaves your computer, and the free tier stops at 10 credits a day.",
  },
  {
    name: "FreeConvert",
    href: "https://www.freeconvert.com/",
    bestFor: "A browser converter with a bigger free allowance for quick jobs.",
    price: (
      <>
        20 conversion minutes a day free, at most 5 per file, then from $12.99 a month for 1,500
        minutes and files up to 1.5 GB, per its{" "}
        <TextLink href="https://www.freeconvert.com/pricing">pricing page</TextLink>.
      </>
    ),
    platforms: "Any browser, plus an API.",
    files: "Uploaded to its servers.",
    batch: "Up to 100 conversions at a time on paid plans.",
    shortfalls:
      "Every file leaves your computer, plans are billed in conversion minutes, and one-time credits expire after 30 days.",
  },
];

export const Route = createFileRoute("/_site/cloudconvert-alternative")({
  head: cloudconvertAlternativeHead,
  component: CloudconvertAlternative,
});

function CloudconvertAlternative() {
  return (
    <article className={cx(siteColumn, "pt-10 pb-20 md:pt-14")}>
      <p className="font-mono text-xs/4 text-ink-2 uppercase">Compared</p>
      <header className="mt-3 flex max-w-[720px] flex-col gap-4">
        <h1 className="text-[34px]/10 font-semibold tracking-[-0.03em] text-balance md:text-[44px]/12">
          {page.heading}
        </h1>
        <div className="flex flex-col gap-4 text-[17px]/[27px] text-pretty text-ink-2">
          <p>
            Keep CloudConvert if you convert a few files a day in a browser, need one of its rarer
            formats such as ebooks or CAD drawings, or connect conversions to cloud storage through
            its API. Pick convt if your files shouldn&apos;t leave your computer, or you convert
            whole folders often enough that daily limits and credits get in the way.
          </p>
          <p>
            CloudConvert uploads every file it converts. Below: the offline options, when switching
            makes sense, five alternatives in the same format, and where CloudConvert still wins.
          </p>
        </div>
        <p className="text-[13px]/5 text-ink-2">
          Prices and limits checked on <time dateTime="2026-11-25">{CHECKED}</time>
        </p>
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
          <TextLink href="#compare">Compare the six</TextLink>
        </div>
      </header>

      <div className="mt-12 grid gap-12 lg:grid-cols-[minmax(0,720px)_220px] lg:justify-between">
        <div className="flex min-w-0 flex-col gap-16">
          <GuideSection
            id="compare"
            title="CloudConvert and five alternatives side by side"
            source={
              <Checked how="docs">
                From each tool&apos;s own site, pricing page or README on {CHECKED}; links in each
                block below.
              </Checked>
            }
          >
            <div
              role="region"
              aria-label="CloudConvert and five alternatives compared"
              tabIndex={0}
              className={cx(
                "overflow-x-auto rounded-2xl shadow-[inset_0_0_0_1px_var(--line)] [scrollbar-width:thin]",
                focusRing,
              )}
            >
              <table className="w-full min-w-[640px] border-collapse text-left text-[14px]/5">
                <thead className="bg-sunken text-ink-2">
                  <tr>
                    <th scope="col" className="px-4 py-3 font-medium">
                      Tool
                    </th>
                    <th scope="col" className="px-4 py-3 font-medium">
                      Price
                    </th>
                    <th scope="col" className="px-4 py-3 font-medium">
                      Runs on
                    </th>
                    <th scope="col" className="px-4 py-3 font-medium">
                      Your files
                    </th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-divider">
                  {rows.map(([tool, price, platforms, files]) => (
                    <tr key={tool}>
                      <th scope="row" className="px-4 py-3 font-medium">
                        {tool}
                      </th>
                      <td className="px-4 py-3 text-ink-2">{price}</td>
                      <td className="px-4 py-3 text-ink-2">{platforms}</td>
                      <td className="px-4 py-3">{files}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </GuideSection>

          <GuideSection id="offline" title="Is there an offline alternative to CloudConvert?">
            <Prose>
              <p>
                Yes. convt Desktop (Mac, Windows and Linux) and File Converter (Windows) are apps
                that convert files on your own computer, so local conversions never leave it.
                Convertio and FreeConvert upload your files, like CloudConvert.
              </p>
              <p>
                convt runs on Mac, Windows and Linux and handles video, audio, images, PDFs and
                Office documents from one app and one command. File Converter is free but Windows
                only. VERT sits in between: a website that converts most files inside your browser
                tab, though on its official site some video conversions run on VERT&apos;s server.
              </p>
            </Prose>
          </GuideSection>

          <GuideSection
            id="switch"
            title="When should you switch from CloudConvert?"
            source={
              <Checked how="docs">
                From CloudConvert&apos;s <TextLink href={ccPricing}>pricing page</TextLink>,{" "}
                <TextLink href={ccSecurity}>security page</TextLink> and{" "}
                <TextLink href={ccPrivacy}>privacy policy</TextLink> on {CHECKED}.
              </Checked>
            }
          >
            <Prose>
              <p>Switch when one of these is true for the files you convert most:</p>
              <ul className="flex list-disc flex-col gap-2 pl-5">
                <li>
                  <strong className="font-semibold">
                    The files can&apos;t leave your computer.
                  </strong>{" "}
                  CloudConvert converts on its servers. It says each conversion runs in an isolated
                  container, and its privacy policy says files are deleted when you press delete, or
                  automatically within 24 hours. Every file is still uploaded first, which an NDA
                  that bans third-party processing, or a company rule that keeps files on the
                  device, rules out.
                </li>
                <li>
                  <strong className="font-semibold">You keep hitting the free limits.</strong> Free
                  accounts get 10 conversion credits a day, files up to 1 GB and 5 minutes of
                  processing per conversion, with 5 conversions running at once.
                </li>
                <li>
                  <strong className="font-semibold">
                    Long videos and Office files cost more than one credit.
                  </strong>{" "}
                  Paid conversions take at least one credit, plus one for every extra minute of
                  conversion time. Office to PDF starts at 2 credits and PDF to Office at 4. A
                  package of 1,000 credits is $17; a subscription is $9 a month for 1,000.
                </li>
                <li>
                  <strong className="font-semibold">You convert folders, not single files.</strong>{" "}
                  A web converter takes the files you drop in and hands back downloads. A desktop
                  converter can take a folder, subfolders included, and write the results next to
                  it.
                </li>
              </ul>
              <p>
                If none of these apply, CloudConvert does the job, and its free tier costs nothing.
              </p>
            </Prose>
          </GuideSection>

          <GuideSection id="tools" title="Five CloudConvert alternatives">
            <Prose>
              <p>
                Each block has the same fields, so you can compare them line by line. If you only
                use a Mac, the <TextLink href={macFileConverter.path}>Mac file converter</TextLink>{" "}
                page also covers what Finder, Preview and QuickTime convert for free.
              </p>
            </Prose>
            {tools.map((tool) => (
              <ToolBlock key={tool.name} tool={tool} />
            ))}
          </GuideSection>

          <GuideSection
            id="cloudconvert-better"
            title="Where CloudConvert is better"
            source={
              <Checked how="docs">
                From <TextLink href="https://cloudconvert.com/">cloudconvert.com</TextLink>, its
                pricing page and its security page on {CHECKED}.
              </Checked>
            }
          >
            <Prose>
              <ul className="flex list-disc flex-col gap-2 pl-5">
                <li>
                  It has a free tier: 10 conversion credits a day with nothing to install. convt has
                  no free tier.
                </li>
                <li>
                  It lists 213 formats in 11 categories, including camera RAW, CAD drawings,
                  archives and ebooks. convt 0.5.0 covers video, audio, images, PDFs and Office
                  documents.
                </li>
                <li>
                  It works free from any device with a browser, phones and Chromebooks included.
                  convt&apos;s local conversions need a Mac with Apple silicon, or a Windows or
                  Linux PC; converting from a phone or browser needs Desktop + Cloud.
                </li>
                <li>
                  Its API imports from and exports to your own storage, such as Amazon S3, and the
                  company behind it holds ISO 27001 certification and signs data processing
                  agreements.
                </li>
              </ul>
            </Prose>
          </GuideSection>

          <GuideSection
            id="convt"
            title="The same jobs in convt"
            source={<Checked how="tested">Run on convt 0.5.0, the current release.</Checked>}
          >
            <Prose>
              <p>
                The job a web converter handles worst is a folder of folders. In convt it&apos;s one
                command: every file, subfolders included, into a separate <Code>webp</Code> folder
                with the same layout, nothing uploaded:
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
                The same pattern converts video, audio, PDFs and Word, Excel and PowerPoint files
                (those need LibreOffice on a Mac or Linux, or the document pack on Windows). One
                example for each kind of file is on the{" "}
                <TextLink href={commandLineConverter.path}>command-line page</TextLink>, and the{" "}
                <TextLink href={guidePath("batch-heic-to-jpg-windows")}>
                  batch HEIC to JPG guide
                </TextLink>{" "}
                covers a folder of iPhone photos on Windows. For video-only jobs, the{" "}
                <TextLink href={handbrakeAlternative.path}>HandBrake alternatives</TextLink> page
                compares encoders.
              </p>
              <p>
                On a Mac, Homebrew installs the app and the <Code>convt</Code> command:
              </p>
            </Prose>
            <Commands label="Homebrew" commands={homebrewCommands} />
            <aside className="flex flex-col gap-5 rounded-2xl bg-panel p-6 shadow-[inset_0_0_0_1px_var(--line)] sm:flex-row sm:items-center sm:justify-between sm:gap-8">
              <div className="flex flex-col gap-1.5 text-[14px]/[22px] text-ink-2">
                <p className="text-[16px]/6 font-semibold text-ink">
                  convt for Mac, Windows and Linux
                </p>
                <p>
                  Video, audio, images and documents, from Finder or Terminal. $39 once. Local
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

          <GuideSection
            id="api"
            title="What if you use CloudConvert's API?"
            source={
              <Checked how="docs">
                From the <TextLink href={routes.api}>convt API page</TextLink> on {CHECKED}.
              </Checked>
            }
          >
            <Prose>
              <p>
                convt has a cloud API too, priced per file instead of per minute: $0.01 for each
                conversion that succeeds, whatever the format or how long it takes, with no monthly
                fee and no free tier. Files up to 25 MB convert in one request and up to 2 GB
                through an upload URL.
              </p>
              <p>
                Like CloudConvert&apos;s, it runs on servers, so files are uploaded; convt deletes a
                job&apos;s files 24 hours after the job is created. The{" "}
                <TextLink href={routes.api}>API page</TextLink> has a credit-by-credit comparison
                and the <TextLink href={routes.apiDocs}>API docs</TextLink> have every endpoint.
              </p>
            </Prose>
          </GuideSection>

          <GuideSection id="faq" title="Questions">
            <dl className="flex flex-col gap-6">
              <Question q="Is there a free CloudConvert alternative?">
                Yes. VERT is free in any browser and File Converter is free on Windows. Convertio
                and FreeConvert have free tiers but upload your files. convt isn&apos;t free: the
                source is open (AGPL-3.0), but the signed app is $39 once, after a 7-day trial.
              </Question>
              <Question q="Is CloudConvert safe to use?">
                It says each conversion runs in an isolated container and its company holds ISO
                27001 certification; its privacy policy says files are deleted automatically within
                24 hours, or at once when you press delete. Your files still leave your computer, so
                for files that mustn&apos;t, use a converter that runs locally.
              </Question>
              <Question q="Is there a CloudConvert alternative for Mac?">
                convt runs on macOS 13 or later on Apple silicon, with a Convert with convt menu in
                Finder and the <Code>convt</Code> command in Terminal. VERT works in Safari or any
                other browser. File Converter is Windows only.
              </Question>
              <Question q="Does convt upload my files?">
                Not for local conversions in the desktop app or the <Code>convt</Code> command. The
                only uploads are cloud conversions, which you pick per job or start from a phone or
                browser and which need Desktop + Cloud, and API requests.
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

function ToolBlock({ tool }: { tool: Tool }) {
  const id = `tool-${tool.name.toLowerCase().replaceAll(" ", "-")}`;
  return (
    <section aria-labelledby={id} className="flex flex-col gap-3">
      <h3 id={id} className="text-[19px]/7 font-semibold tracking-[-0.01em]">
        <TextLink href={tool.href}>{tool.name}</TextLink>
      </h3>
      <FactList
        facts={[
          [`${tool.name} is best for`, tool.bestFor],
          [`${tool.name} price`, tool.price],
          [`${tool.name} runs on`, tool.platforms],
          [`What ${tool.name} does with your files`, tool.files],
          [`${tool.name} batches`, tool.batch],
          [`Where ${tool.name} falls short`, tool.shortfalls],
        ]}
      />
    </section>
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
