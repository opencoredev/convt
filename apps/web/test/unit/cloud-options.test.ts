import { expect, test } from "bun:test";

import { jobOptions, qualityApplies, sizeControl } from "../../src/lib/cloud-options";

test("each target offers only the controls its engine applies", () => {
  expect(qualityApplies("jpeg")).toBe(true);
  expect(qualityApplies("mp4")).toBe(true);
  expect(qualityApplies("png")).toBe(false);
  expect(qualityApplies("heic")).toBe(true);
  expect(qualityApplies("webp")).toBe(false);
  expect(qualityApplies("pdf")).toBe(false);
  expect(sizeControl({ from: "jpeg", to: "png" })?.field).toBe("max_size");
  expect(sizeControl({ from: "mov", to: "mp4" })?.field).toBe("video_height");
  expect(sizeControl({ from: "png", to: "ico" })).toBeNull();
  expect(sizeControl({ from: "wav", to: "mp3" })).toBeNull();
});

test("a GIF made from video is sized by height, one made from an image by its longest edge", () => {
  expect(sizeControl({ from: "mp4", to: "gif" })?.field).toBe("video_height");
  expect(sizeControl({ from: "mov", to: "gif" })?.field).toBe("video_height");
  expect(sizeControl({ from: "png", to: "gif" })?.field).toBe("max_size");
  expect(sizeControl({ from: undefined, to: "gif" })?.field).toBe("max_size");
  // A frame grabbed from video still takes the image size.
  expect(sizeControl({ from: "mp4", to: "png" })?.field).toBe("max_size");
  expect(jobOptions({ from: "mp4", to: "gif" }, { quality: null, size: 480 })).toEqual({
    video_height: 480,
  });
});

test("only set options that apply are sent", () => {
  expect(jobOptions({ from: "png", to: "jpeg" }, { quality: null, size: null })).toBeUndefined();
  expect(jobOptions({ from: "png", to: "jpeg" }, { quality: 60, size: 1024 })).toEqual({
    quality: 60,
    max_size: 1024,
  });
  expect(jobOptions({ from: "jpeg", to: "png" }, { quality: 60, size: 512 })).toEqual({
    max_size: 512,
  });
  expect(jobOptions({ from: "mov", to: "mp4" }, { quality: 95, size: 720 })).toEqual({
    quality: 95,
    video_height: 720,
  });
  expect(jobOptions({ from: "docx", to: "pdf" }, { quality: 60, size: 512 })).toBeUndefined();
});

test("a size the conversion's control doesn't offer is not sent", () => {
  // 1024 px picked for an image GIF, then the file changed to a video.
  expect(jobOptions({ from: "mp4", to: "gif" }, { quality: null, size: 1024 })).toBeUndefined();
  expect(jobOptions({ from: "png", to: "gif" }, { quality: null, size: 1024 })).toEqual({
    max_size: 1024,
  });
});
