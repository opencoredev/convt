import { expect, test } from "bun:test";

import { jobOptions, qualityApplies, sizeControl } from "../../src/lib/cloud-options";

test("each target offers only the controls its engine applies", () => {
  expect(qualityApplies("jpeg")).toBe(true);
  expect(qualityApplies("mp4")).toBe(true);
  expect(qualityApplies("png")).toBe(false);
  expect(qualityApplies("pdf")).toBe(false);
  expect(sizeControl("png")?.field).toBe("max_size");
  expect(sizeControl("mp4")?.field).toBe("video_height");
  expect(sizeControl("ico")).toBeNull();
  expect(sizeControl("mp3")).toBeNull();
});

test("only set options that apply are sent", () => {
  expect(jobOptions("jpeg", { quality: null, size: null })).toBeUndefined();
  expect(jobOptions("jpeg", { quality: 60, size: 1024 })).toEqual({ quality: 60, max_size: 1024 });
  expect(jobOptions("png", { quality: 60, size: 512 })).toEqual({ max_size: 512 });
  expect(jobOptions("mp4", { quality: 95, size: 720 })).toEqual({ quality: 95, video_height: 720 });
  expect(jobOptions("pdf", { quality: 60, size: 512 })).toBeUndefined();
});
