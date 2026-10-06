// The conversion pages under /convert: one page per popular conversion, written for
// people searching "heic to jpg" and the like. Every pair must be a route the registry
// offers (content/formats.json, regenerated from the convt CLI); the check at the
// bottom fails the build otherwise, so the site never promises a conversion convt
// cannot do.

import registry from "../../content/formats.json";

export type Category = "images" | "video" | "audio" | "documents";

export const categories: { id: Category; title: string }[] = [
  { id: "images", title: "Images" },
  { id: "video", title: "Video" },
  { id: "audio", title: "Audio" },
  { id: "documents", title: "Documents" },
];

type FormatInfo = {
  /** What people call it, and what goes in the URL: "JPG", not "JPEG". */
  label: string;
  ext: string;
  about: string;
};

// Plain descriptions of each format these pages mention. Keys are registry ids.
export const formats: Record<string, FormatInfo> = {
  heic: {
    label: "HEIC",
    ext: "heic",
    about:
      "The photo format iPhones and iPads save by default. It keeps about the same quality as JPG in roughly half the space, but many Windows apps, older software and upload forms don't open it.",
  },
  jpeg: {
    label: "JPG",
    ext: "jpg",
    about:
      "The most widely supported photo format. Every phone, browser, editor and upload form opens it. It's lossy and has no transparency.",
  },
  png: {
    label: "PNG",
    ext: "png",
    about:
      "A lossless image format with transparency. It's the usual choice for screenshots, logos and graphics, and it makes large files for photos.",
  },
  webp: {
    label: "WebP",
    ext: "webp",
    about:
      "Google's image format for the web, with lossy and lossless modes and transparency. Browsers handle it well; some desktop apps, editors and upload forms still don't.",
  },
  avif: {
    label: "AVIF",
    ext: "avif",
    about:
      "A newer image format based on the AV1 video codec. Files are very small, but support is still patchy outside modern browsers.",
  },
  svg: {
    label: "SVG",
    ext: "svg",
    about:
      "A vector format: shapes described in text, sharp at any size. Plenty of apps, documents and social sites only take pixel images.",
  },
  ico: {
    label: "ICO",
    ext: "ico",
    about:
      "The Windows icon format, also used for website favicons. One file can hold the icon image the system or browser shows.",
  },
  gif: {
    label: "GIF",
    ext: "gif",
    about:
      "An old animated image format with up to 256 colors and no sound. It plays everywhere, which is why it's still used for short loops.",
  },
  mov: {
    label: "MOV",
    ext: "mov",
    about:
      "Apple's QuickTime video format. iPhones and Macs record to it, and plenty of Windows apps, editors and upload forms handle it poorly.",
  },
  mp4: {
    label: "MP4",
    ext: "mp4",
    about:
      "The most compatible video format. Phones, TVs, browsers, editors and every major site play it.",
  },
  mkv: {
    label: "MKV",
    ext: "mkv",
    about:
      "Matroska, an open video container that can hold several audio and subtitle tracks. Many TVs, phones and editors won't play it.",
  },
  webm: {
    label: "WebM",
    ext: "webm",
    about:
      "An open video format for the web, often what screen recorders and browser downloads produce. Editors and Apple devices are less keen on it.",
  },
  mp3: {
    label: "MP3",
    ext: "mp3",
    about:
      "The audio format everything plays: phones, cars, browsers, speakers and old MP3 players.",
  },
  wav: {
    label: "WAV",
    ext: "wav",
    about:
      "Uncompressed audio. It's exact and simple, and about ten times the size of an MP3 of the same recording.",
  },
  m4a: {
    label: "M4A",
    ext: "m4a",
    about:
      "AAC audio in an MP4 container. Voice Memos, iTunes and many recorders save it; some older players and upload forms only take MP3.",
  },
  flac: {
    label: "FLAC",
    ext: "flac",
    about:
      "Lossless compressed audio: the original sound at about half the size of WAV. Many phones, cars and apps don't play it.",
  },
  pdf: {
    label: "PDF",
    ext: "pdf",
    about:
      "The format for documents that should look the same everywhere: forms, contracts, statements and scans.",
  },
  docx: {
    label: "DOCX",
    ext: "docx",
    about: "Microsoft Word's document format since Word 2007.",
  },
  doc: {
    label: "DOC",
    ext: "doc",
    about:
      "The older Word format from Word 97 to 2003, still common in archives and email attachments.",
  },
  odt: {
    label: "ODT",
    ext: "odt",
    about:
      "The OpenDocument text format that LibreOffice and other open source office suites save.",
  },
  pptx: {
    label: "PPTX",
    ext: "pptx",
    about: "Microsoft PowerPoint's presentation format.",
  },
  xlsx: {
    label: "XLSX",
    ext: "xlsx",
    about: "Microsoft Excel's spreadsheet format.",
  },
};

export type EngineId = "image" | "resvg" | "libheif" | "ffmpeg" | "pdfium" | "libreoffice";

export const engines: Record<EngineId, { name: string; about: string }> = {
  image: {
    name: "Rust image libraries",
    about:
      "Photos are decoded and encoded by pure Rust image libraries built into convt. Nothing extra to install.",
  },
  resvg: {
    name: "resvg",
    about: "SVG files are drawn by resvg, a Rust SVG renderer built into convt.",
  },
  libheif: {
    name: "macOS or libheif",
    about:
      "On a Mac, convt reads HEIC and AVIF with the system's own image decoder. On Windows and Linux it uses libheif, the open source HEIF library, which ships with convt.",
  },
  ffmpeg: {
    name: "FFmpeg",
    about:
      "Video and audio go through FFmpeg, the tool behind most video software, bundled with convt and driven for you.",
  },
  pdfium: {
    name: "PDFium",
    about:
      "PDF pages are drawn by PDFium, the PDF renderer inside Chrome, so pages look the way they do in your browser.",
  },
  libreoffice: {
    name: "LibreOffice",
    about:
      "Office documents are opened and saved by LibreOffice running in the background. convt uses the copy already on your computer, or installs its own document pack once with `convt pack install documents`.",
  },
};

export type Conversion = {
  from: string;
  to: string;
  category: Category;
  engine: EngineId;
  /** Why someone would make this conversion: the opening of the page. */
  why: string;
  /**
   * False when the target is not in the right-click menu's shortlist for this source
   * (Registry::menu_targets in crates/convt-core/src/registry.rs), so the steps point
   * to More options… instead.
   */
  menu?: false;
  /** Things worth knowing, specific to this pair. */
  notes: string[];
  /** Options that matter for this pair, as `convt` flags. */
  options?: { flag: string; does: string }[];
  /** Pair-specific questions; the page adds the general ones. */
  faq?: { q: string; a: string }[];
};

export const conversions: Conversion[] = [
  // Images
  {
    from: "heic",
    to: "jpeg",
    category: "images",
    engine: "libheif",
    why: "iPhone photos arrive as HEIC, and a lot of software still won't open them: Windows photo viewers, older editors, print shops and many upload forms. JPG works everywhere.",
    notes: [
      "Expect the JPG to be larger than the HEIC original at the same quality.",
      "Pick the quality yourself with `-q`. Higher keeps more detail and makes bigger files.",
      "Convert a whole camera-roll export at once by passing the folder.",
    ],
    options: [{ flag: "-q 90", does: "JPG quality from 1 to 100" }],
    faq: [
      {
        q: "Can I stop my iPhone saving HEIC?",
        a: "Yes. In Settings, Camera, Formats, choose Most Compatible and new photos save as JPG. Photos you already took stay HEIC, which is where convt helps.",
      },
    ],
  },
  {
    from: "heic",
    to: "png",
    category: "images",
    engine: "libheif",
    why: "When you want an iPhone photo in a lossless format to edit, annotate or drop into a design tool, PNG keeps every pixel of the decoded image.",
    notes: [
      "PNG is lossless, so files are much larger than HEIC or JPG. For sharing photos, JPG is usually the better pick.",
      "Convert a whole folder at once by passing the folder.",
    ],
  },
  {
    from: "webp",
    to: "png",
    category: "images",
    engine: "image",
    why: "Images saved from websites often turn out to be WebP, and plenty of desktop apps, editors and upload forms still refuse them. PNG opens everywhere and keeps transparency.",
    notes: [
      "Transparency carries over to PNG.",
      "PNG is lossless, so expect a bigger file than the WebP.",
    ],
  },
  {
    from: "webp",
    to: "jpeg",
    category: "images",
    engine: "image",
    why: "Saved a WebP from a website and need it somewhere that only takes JPG? Converting gives you a photo every app and form accepts.",
    notes: [
      "JPG has no transparency. If the WebP has see-through areas, convert to PNG instead.",
      "Set the JPG quality with `-q`.",
    ],
    options: [{ flag: "-q 85", does: "JPG quality from 1 to 100" }],
  },
  {
    from: "png",
    to: "jpeg",
    category: "images",
    engine: "image",
    why: "Screenshots and exports often come out as PNG, which makes big files for anything photographic. JPG is a fraction of the size and fine for sharing or uploading.",
    notes: [
      "JPG has no transparency. Keep PNG or use WebP if you need see-through areas.",
      "Shrink large images in the same step with `--max-size`.",
    ],
    options: [
      { flag: "-q 85", does: "JPG quality from 1 to 100" },
      { flag: "--max-size 2000", does: "Longest edge in pixels" },
    ],
  },
  {
    from: "jpeg",
    to: "png",
    category: "images",
    engine: "image",
    why: "When a tool or template asks for PNG, or you're about to edit an image repeatedly, PNG stops quality dropping a little more with every save.",
    notes: [
      "Converting doesn't bring back detail the JPG already lost; it just stops further loss.",
      "PNG files of photos are several times larger than the JPG.",
    ],
  },
  {
    from: "png",
    to: "webp",
    category: "images",
    engine: "image",
    why: "WebP makes web images smaller than PNG and keeps transparency, which helps page speed scores.",
    notes: [
      "convt writes lossless WebP, so nothing changes visually and transparency carries over.",
      "Resize in the same step with `--max-size`.",
    ],
    options: [{ flag: "--max-size 1600", does: "Longest edge in pixels" }],
  },
  {
    from: "jpeg",
    to: "webp",
    category: "images",
    engine: "image",
    why: "Preparing photos for a website? WebP is the format modern browsers load fastest, and converting a whole folder takes one command.",
    notes: [
      "convt writes lossless WebP. For photos that are already JPG, resizing with `--max-size` usually saves more than the format change.",
    ],
    options: [{ flag: "--max-size 1600", does: "Longest edge in pixels" }],
  },
  {
    from: "avif",
    to: "jpeg",
    category: "images",
    engine: "libheif",
    why: "AVIF images are tiny, but a lot of apps, editors and upload forms don't open them yet. JPG works everywhere.",
    notes: [
      "Set the JPG quality with `-q`.",
      "Expect the JPG to be noticeably larger than the AVIF.",
    ],
    options: [{ flag: "-q 90", does: "JPG quality from 1 to 100" }],
  },
  {
    from: "svg",
    to: "png",
    category: "images",
    engine: "resvg",
    why: "Logos and icons often come as SVG, but slide decks, documents, social sites and many apps need a pixel image. PNG keeps the transparent background.",
    notes: [
      "By default the PNG is the size the SVG declares. Render it larger with `--dpi`: 96 is the SVG's own size, so 192 doubles it.",
      "Transparent areas stay transparent.",
    ],
    options: [{ flag: "--dpi 384", does: "Render at four times the SVG's own size" }],
  },
  {
    from: "png",
    to: "ico",
    category: "images",
    engine: "image",
    menu: false,
    why: "Websites still need a favicon.ico, and Windows shortcuts and apps use ICO icons. Turn a PNG logo into one in a single step.",
    notes: [
      "Start from a square image. convt scales larger images to fit 256 pixels and keeps the proportions.",
      "Transparency carries over.",
    ],
  },
  // Video
  {
    from: "mov",
    to: "mp4",
    category: "video",
    engine: "ffmpeg",
    why: "iPhone and Mac recordings save as MOV, which many Windows apps, editors and upload forms handle badly. MP4 plays everywhere.",
    notes: [
      "convt writes H.264 video by default, the most compatible choice. Use `--video-codec hevc` for smaller files on newer devices.",
      "Cap the resolution with `--video-height` to make files smaller for sharing.",
    ],
    options: [
      { flag: "--video-codec hevc", does: "HEVC instead of H.264" },
      { flag: "--video-height 1080", does: "Maximum height; never enlarges" },
    ],
  },
  {
    from: "mkv",
    to: "mp4",
    category: "video",
    engine: "ffmpeg",
    why: "MKV files play fine on a computer but not on many TVs, phones or in video editors. MP4 is what they expect.",
    notes: [
      "convt writes H.264 video by default, or HEVC with `--video-codec hevc`.",
      "Leave the audio out with `--no-audio`.",
    ],
    options: [
      { flag: "--video-codec hevc", does: "HEVC instead of H.264" },
      { flag: "--video-height 720", does: "Maximum height; never enlarges" },
    ],
  },
  {
    from: "webm",
    to: "mp4",
    category: "video",
    engine: "ffmpeg",
    why: "Screen recorders and browser downloads often produce WebM, which video editors and iPhones don't like. MP4 works in all of them.",
    notes: ["convt writes H.264 video by default, the most compatible choice."],
    options: [{ flag: "--video-height 1080", does: "Maximum height; never enlarges" }],
  },
  {
    from: "mp4",
    to: "gif",
    category: "video",
    engine: "ffmpeg",
    why: "GIFs play inline in chats, docs, READMEs and issue trackers where a video file won't autoplay.",
    notes: [
      "GIFs have no sound and up to 256 colors, and they get big fast. Short clips work best.",
      "Make the GIF smaller with `--video-height`.",
    ],
    options: [{ flag: "--video-height 360", does: "Maximum height; never enlarges" }],
  },
  // Audio
  {
    from: "mp4",
    to: "mp3",
    category: "audio",
    engine: "ffmpeg",
    why: "Keep just the sound of a video: a talk, a lecture, a song or an interview, as an MP3 any player handles.",
    notes: ["Set the bitrate with `--audio-bitrate`. 192 kbit/s is plenty for most listening."],
    options: [{ flag: "--audio-bitrate 192", does: "MP3 bitrate in kbit/s" }],
  },
  {
    from: "wav",
    to: "mp3",
    category: "audio",
    engine: "ffmpeg",
    why: "WAV recordings are huge. MP3 makes them about a tenth of the size for sharing, uploading or listening on the go.",
    notes: [
      "Set the bitrate with `--audio-bitrate`. Keep the WAV if you'll edit the recording again.",
    ],
    options: [{ flag: "--audio-bitrate 320", does: "MP3 bitrate in kbit/s" }],
  },
  {
    from: "m4a",
    to: "mp3",
    category: "audio",
    engine: "ffmpeg",
    why: "Voice Memos and many recorders save M4A, and some players, car stereos and upload forms only take MP3.",
    notes: ["Set the bitrate with `--audio-bitrate`."],
    options: [{ flag: "--audio-bitrate 192", does: "MP3 bitrate in kbit/s" }],
  },
  {
    from: "flac",
    to: "mp3",
    category: "audio",
    engine: "ffmpeg",
    why: "FLAC keeps every detail but many phones, cars and apps won't play it. MP3 copies take a fraction of the space.",
    notes: [
      "Keep the FLAC files as your originals; MP3 is lossy.",
      "Convert a whole album folder at once.",
    ],
    options: [{ flag: "--audio-bitrate 320", does: "MP3 bitrate in kbit/s" }],
  },
  // Documents
  {
    from: "pdf",
    to: "jpeg",
    category: "documents",
    engine: "pdfium",
    why: "Need a page of a PDF as a picture, for a slide, a message or an upload form that only takes images? convt turns each page into a JPG.",
    notes: [
      "You get one JPG per page. Pick pages with `--pages`, like `--pages 1-3`.",
      "Set the resolution with `--dpi`.",
    ],
    options: [
      { flag: "--pages 1-3", does: "Which pages to render" },
      { flag: "--dpi 200", does: "Render resolution" },
    ],
  },
  {
    from: "pdf",
    to: "png",
    category: "documents",
    engine: "pdfium",
    why: "PNG keeps text and line art in PDF pages crisp, which suits diagrams, charts and pages you'll annotate.",
    notes: ["You get one PNG per page. Pick pages with `--pages` and the resolution with `--dpi`."],
    options: [
      { flag: "--pages 2", does: "Which pages to render" },
      { flag: "--dpi 300", does: "Render resolution" },
    ],
  },
  {
    from: "docx",
    to: "pdf",
    category: "documents",
    engine: "libreoffice",
    why: "Send a Word document as a PDF so it looks the same for everyone and can't be edited by accident, without opening Word.",
    notes: [
      "Layout follows the fonts on your computer. If a document uses a font you don't have, the PDF uses a substitute.",
      "Convert a folder of documents at once.",
    ],
  },
  {
    from: "doc",
    to: "pdf",
    category: "documents",
    engine: "libreoffice",
    why: "Old Word files from archives and email attachments turn into PDFs that open on any device.",
    notes: ["Layout follows the fonts on your computer, so missing fonts are substituted."],
  },
  {
    from: "odt",
    to: "pdf",
    category: "documents",
    engine: "libreoffice",
    why: "Share a LibreOffice document with people who use Word or only read PDFs.",
    notes: ["Convert a folder of documents at once."],
  },
  {
    from: "pptx",
    to: "pdf",
    category: "documents",
    engine: "libreoffice",
    why: "Send slides as a PDF so they look right on any device, even without PowerPoint.",
    notes: [
      "Each slide becomes a page. Animations and transitions don't carry over to PDF.",
      "Missing fonts are substituted with ones on your computer.",
    ],
  },
  {
    from: "xlsx",
    to: "pdf",
    category: "documents",
    engine: "libreoffice",
    why: "Share a spreadsheet as a PDF for invoices, reports and printouts that shouldn't change.",
    notes: ["Page breaks and print areas set in the spreadsheet decide how the PDF pages split."],
  },
];

export const slugOf = (c: Pick<Conversion, "from" | "to">) =>
  `${formats[c.from].ext}-to-${formats[c.to].ext}`;

export const titleOf = (c: Pick<Conversion, "from" | "to">) =>
  `${formats[c.from].label} to ${formats[c.to].label}`;

export const conversionBySlug = new Map(conversions.map((c) => [slugOf(c), c]));

/** Paths of every page under /convert, for prerendering and the sitemap. */
export const conversionPaths = ["/convert", ...conversions.map((c) => `/convert/${slugOf(c)}`)];

/** Other pages to link from a conversion: same source or target first, then the category. */
export function relatedTo(c: Conversion, count = 6): Conversion[] {
  // By slug, not identity: route loader data reaches the page as a serialized copy.
  const others = conversions.filter((o) => slugOf(o) !== slugOf(c));
  const close = others.filter((o) => o.from === c.from || o.to === c.to || o.from === c.to);
  const sameCategory = others.filter((o) => o.category === c.category && !close.includes(o));
  return [...close, ...sameCategory].slice(0, count);
}

/** The `convt` example command for a conversion. */
export const commandFor = (c: Conversion, name = "file") =>
  `convt ${name}.${formats[c.from].ext} --to ${formats[c.to].ext}`;

// Fail the build if a page promises a conversion the registry does not offer.
const registryById = new Map(registry.formats.map((f) => [f.id, f]));
for (const c of conversions) {
  const source = registryById.get(c.from);
  if (!formats[c.from] || !formats[c.to]) {
    throw new Error(`conversions.ts: describe ${c.from} and ${c.to} in formats`);
  }
  if (!source || !source.targets.includes(c.to)) {
    throw new Error(
      `conversions.ts: convt cannot convert ${c.from} to ${c.to} (content/formats.json)`,
    );
  }
}
if (conversionBySlug.size !== conversions.length) {
  throw new Error("conversions.ts: two conversions share a slug");
}
