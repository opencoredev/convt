import formats from "../../content/formats.json";

import { GITHUB_URL, RAYCAST_URL, SITE_ORIGIN, routes } from "#/lib/site";

// /llms.txt (llmstxt.org): a plain summary of convt and its public pages for AI agents and
// answer engines. Keep every line true for the current release; the docs Worker publishes
// its own /docs/llms.txt for the API.

const page = (title: string, path: string, about: string) =>
  `- [${title}](${SITE_ORIGIN}${path}): ${about}`;

export const llmsTxt = `# convt

> convt converts images, video, audio and documents on your own computer. Right-click a file, pick a format, and the result is written next to the original. Nothing is uploaded.

- Desktop app for macOS (Apple silicon), Windows (x86_64) and Linux (x86_64: AppImage, .deb, .rpm and tarball).
- A "Convert with convt" menu in Finder on macOS and in Linux file managers. The Windows Explorer menu is not in the installer yet.
- The \`convt\` command-line tool ships in the same install: \`convt clip.mov --to mp4\`, \`convt photos/ --to webp -r\`, \`convt lease.pdf --to png --pages 1-3\`.
- ${formats.formats.length} formats, converted with FFmpeg, PDFium, LibreOffice, libheif and native image libraries. Word, Excel and PowerPoint need a one-time document pack download.
- Install on macOS with Homebrew: \`brew tap opencoredev/convt https://github.com/opencoredev/convt\` then \`brew install --cask convt\`.
- Packaged builds start a 7-day trial on the first conversion; after that convt needs a paid Desktop license. There is no free tier. The source code is AGPL-3.0.
- A separate, paid cloud API converts files on convt's servers, so files sent to it are uploaded. The desktop app and CLI never upload files.

## Pages

${[
  page("Home", "/", "what convt does, how the right-click menu works, and pricing."),
  page("Supported formats", routes.formats, "every format convt reads and what each converts to."),
  page(
    "Download",
    routes.download,
    "installers for macOS, Windows and Linux (asks you to sign in first).",
  ),
  page("Changelog", routes.changelog, "what changed in each release."),
  page("Contact and help", routes.contact, "support email, license keys, billing and bug reports."),
  page("Brand kit", routes.brand, "logo, wordmark, colors and type."),
].join("\n")}

## API docs

- [convt API docs](${SITE_ORIGIN}/docs): convert files from your own code with the cloud API.
- [API docs for agents](${SITE_ORIGIN}/docs/llms.txt): the docs' own llms.txt.

## Optional

- [Source code on GitHub](${GITHUB_URL})
- [Raycast extension](${RAYCAST_URL})
- [Privacy policy](${SITE_ORIGIN}${routes.privacy})
- [Terms of service](${SITE_ORIGIN}${routes.terms})
`;
