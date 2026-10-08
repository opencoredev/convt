// Settings, messages, notes and toast copy: the parts that decide what users see.

import { describe, expect, test } from "bun:test";

import { parseDataUrl, toBase64, fromBase64 } from "../../src/shared/bytes.ts";
import { toastCopy } from "../../src/shared/copy.ts";
import {
  actionFromMenuId,
  menuId,
  noteFor,
  parseRecent,
  type Action,
  type JobId,
  type Problem,
} from "../../src/shared/jobs.ts";
import { siteUrl } from "../../src/shared/links.ts";
import {
  parseJob,
  parseToBackground,
  parseToastView,
  parseTranscodeResult,
} from "../../src/shared/messages.ts";
import { defaultSettings, parseSettings } from "../../src/shared/settings.ts";
import { relativeTime } from "../../src/shared/time.ts";

const jobId = "3b241101-e2bb-4255-8caf-4136c566a962" as JobId;

describe("settings", () => {
  test("keeps valid values and replaces the rest", () => {
    expect(parseSettings(undefined)).toEqual(defaultSettings);
    expect(parseSettings({ quality: "small", askWhereToSave: true })).toEqual({
      quality: "small",
      askWhereToSave: true,
    });
    expect(parseSettings({ quality: "ultra", askWhereToSave: "yes" })).toEqual(defaultSettings);
  });
});

describe("menu ids", () => {
  test("round-trip every action", () => {
    const actions: Action[] = [
      { kind: "save", target: "png" },
      { kind: "save", target: "jpg" },
      { kind: "save", target: "webp" },
      { kind: "copy" },
    ];
    for (const action of actions) expect(actionFromMenuId(menuId(action))).toEqual(action);
    expect(actionFromMenuId("save:gif")).toBeNull();
    expect(actionFromMenuId("convt")).toBeNull();
  });
});

describe("message parsing", () => {
  const job = {
    id: jobId,
    srcUrl: "https://a.com/x.webp",
    pageUrl: "https://a.com/",
    tabId: 4,
    frameId: 0,
    action: { kind: "save", target: "png" },
  };

  test("accepts well-formed messages", () => {
    expect(parseJob(job)).toEqual(job as never);
    expect(parseToBackground({ kind: "show-download", downloadId: 7 })).toEqual({
      kind: "show-download",
      downloadId: 7,
    });
    expect(parseToBackground({ kind: "access-granted", jobId })).toEqual({
      kind: "access-granted",
      jobId,
    });
  });

  test("rejects anything else", () => {
    expect(parseJob({ ...job, tabId: "4" })).toBeNull();
    expect(parseJob({ ...job, action: { kind: "save", target: "bmp" } })).toBeNull();
    expect(parseToBackground({ kind: "open-access", jobId: "nope" })).toBeNull();
    expect(parseToBackground({ kind: "delete-everything" })).toBeNull();
    expect(parseToBackground(null)).toBeNull();
  });

  test("toast views round-trip through JSON", () => {
    const views = [
      { phase: "working", jobId, thumb: "t", action: { kind: "copy" } },
      {
        phase: "saved",
        jobId,
        thumb: "t",
        name: "a.png",
        bytes: 10,
        from: "webp",
        target: "png",
        downloadId: 3,
        note: { kind: "animated", from: "gif" },
      },
      { phase: "copied", jobId, thumb: "t", bytes: 10, from: "png", note: null },
      {
        phase: "failed",
        jobId,
        thumb: "t",
        action: { kind: "copy" },
        problem: { kind: "blocked", host: "cdn.a.com" },
      },
      { phase: "dismissed", jobId },
    ];
    for (const view of views)
      expect(parseToastView(JSON.parse(JSON.stringify(view)))).toEqual(view as never);
    expect(parseToastView({ ...views[1], note: { kind: "ad" } })).toBeNull();
  });

  test("transcode results", () => {
    const url = {
      ok: true,
      output: { kind: "url", url: "blob:chrome-extension://x/1" },
      bytes: 1,
      thumb: "d",
    };
    expect(parseTranscodeResult(url)).toEqual(url as never);
    const bytes = { ok: true, output: { kind: "bytes", data: "AA==" }, bytes: 1, thumb: "d" };
    expect(parseTranscodeResult(bytes)).toEqual(bytes as never);
    // Only blob: URLs from the converter are downloadable results.
    expect(
      parseTranscodeResult({ ...url, output: { kind: "url", url: "https://evil.example/x.png" } }),
    ).toBeNull();
    expect(parseTranscodeResult({ ok: false, problem: "too-large" })).toEqual({
      ok: false,
      problem: "too-large",
    });
    expect(parseTranscodeResult({ ok: false, problem: "boom" })).toBeNull();
  });

  test("recent items drop malformed entries", () => {
    const good = {
      kind: "saved",
      jobId,
      at: 1,
      name: "a.png",
      bytes: 1,
      target: "png",
      thumb: "d",
      downloadId: 1,
    };
    expect(parseRecent([good, { ...good, target: "tiff" }, "x", null])).toEqual([good as never]);
    expect(parseRecent("nope")).toEqual([]);
  });
});

describe("notes", () => {
  test("animated sources always get a note", () => {
    expect(noteFor({ animated: true, from: "gif", count: 1 })).toEqual({
      kind: "animated",
      from: "gif",
    });
  });

  test("the desktop tip shows on a few milestones only", () => {
    const shown = Array.from({ length: 120 }, (_, i) => i + 1).filter(
      (count) => noteFor({ animated: false, from: "png", count }) !== null,
    );
    expect(shown).toEqual([3, 25, 100]);
  });
});

describe("toast copy", () => {
  const src = "https://cdn.example.com/photos/miso.webp";

  test("progress names the file it will save", () => {
    const copy = toastCopy(
      { phase: "working", jobId, thumb: src, action: { kind: "save", target: "jpg" } },
      src,
    );
    expect(copy.title).toBe("Saving as JPG…");
    expect(copy.detail).toEqual({ text: "miso.jpg", mono: true });
    // Long enough for any real conversion; only a dead worker's toast hits it.
    expect(copy.lingerMs).toBe(120_000);
  });

  test("a saved file shows the conversion and offers its folder", () => {
    const copy = toastCopy(
      {
        phase: "saved",
        jobId,
        thumb: src,
        name: "miso (1).png",
        bytes: 612_000,
        from: "webp",
        target: "png",
        downloadId: 9,
        note: null,
      },
      src,
    );
    expect(copy.title).toBe("miso (1).png");
    expect(copy.detail.text).toBe("WebP → PNG · 612 KB");
    expect(copy.action).toEqual({ kind: "show-download", label: "Show in folder", downloadId: 9 });
  });

  test("only GIFs get the MP4 offer", () => {
    const saved = (from: "gif" | "webp") =>
      toastCopy(
        {
          phase: "copied",
          jobId,
          thumb: src,
          bytes: 1,
          from,
          note: { kind: "animated", from },
        },
        src,
      ).note;
    expect(saved("gif")?.link?.href).toContain("utm_content=toast-animated");
    expect(saved("webp")?.link).toBeNull();
  });

  test("every problem has a title and detail, and only blocking waits for the user", () => {
    const problems: Problem[] = [
      { kind: "blocked", host: "cdn.example.com" },
      { kind: "network" },
      { kind: "http", status: 403 },
      { kind: "not-image" },
      { kind: "unreadable", from: "heic" },
      { kind: "too-large" },
      { kind: "save-failed", reason: "FILE_NO_SPACE" },
      { kind: "save-failed", reason: "SERVER_FORBIDDEN" },
      { kind: "clipboard" },
      { kind: "copy-unavailable" },
      { kind: "in-frame" },
      { kind: "internal" },
    ];
    for (const problem of problems) {
      const copy = toastCopy(
        { phase: "failed", jobId, thumb: src, action: { kind: "copy" }, problem },
        src,
      );
      expect(copy.tone).toBe("failed");
      expect(copy.title.length).toBeGreaterThan(0);
      expect(copy.detail.text.length).toBeGreaterThan(0);
      expect(copy.lingerMs === null).toBe(problem.kind === "blocked");
    }
    const unreadable = toastCopy(
      {
        phase: "failed",
        jobId,
        thumb: src,
        action: { kind: "copy" },
        problem: { kind: "unreadable", from: "heic" },
      },
      src,
    );
    expect(unreadable.title).toBe("Chrome can't read HEIC images");
    expect(unreadable.action?.kind).toBe("link");
  });
});

describe("links", () => {
  test("carry UTM parameters and nothing about the page", () => {
    const url = new URL(siteUrl("/download", "popup-footer"));
    expect(url.origin).toBe("https://convt.app");
    expect(Object.fromEntries(url.searchParams)).toEqual({
      utm_source: "chrome-extension",
      utm_medium: "extension",
      utm_campaign: "image-converter",
      utm_content: "popup-footer",
    });
  });
});

describe("bytes", () => {
  test("base64 round-trips large buffers", () => {
    const data = new Uint8Array(200_000).map((_, i) => i % 251);
    expect(fromBase64(toBase64(data))).toEqual(data);
  });

  test("data URLs, base64 and percent-encoded", () => {
    expect(parseDataUrl("data:image/png;base64,iVBO")?.mime).toBe("image/png");
    expect(new TextDecoder().decode(parseDataUrl("data:image/svg+xml,%3Csvg%3E")?.bytes)).toBe(
      "<svg>",
    );
    expect(parseDataUrl("data:image/png;base64,%%%")).toBeNull();
    expect(parseDataUrl("https://a.com")).toBeNull();
  });
});

describe("relativeTime", () => {
  const now = Date.UTC(2026, 9, 8, 12);
  test("reads naturally", () => {
    expect(relativeTime(now - 10_000, now)).toBe("just now");
    expect(relativeTime(now - 4 * 60_000, now)).toBe("4 min ago");
    expect(relativeTime(now - 3 * 3_600_000, now)).toBe("3 h ago");
    expect(relativeTime(now - 26 * 3_600_000, now)).toBe("yesterday");
    expect(relativeTime(now - 3 * 86_400_000, now)).toBe("3 days ago");
  });
});
