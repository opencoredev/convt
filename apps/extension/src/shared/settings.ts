// User settings, kept in chrome.storage.sync so they follow the user's Chrome profile.

export const QUALITIES = ["high", "medium", "small"] as const;
export type Quality = (typeof QUALITIES)[number];

export const qualityInfo = {
  high: { label: "High", encoderQuality: 0.92 },
  medium: { label: "Medium", encoderQuality: 0.82 },
  small: { label: "Small", encoderQuality: 0.68 },
} as const satisfies Record<Quality, { label: string; encoderQuality: number }>;

export type Settings = {
  /** Encoder quality for JPG and WebP. PNG is lossless. */
  quality: Quality;
  /** Show Chrome's Save As dialog for every file instead of saving to Downloads. */
  askWhereToSave: boolean;
};

export const defaultSettings: Settings = { quality: "high", askWhereToSave: false };

/** Storage holds whatever an older or newer version wrote; keep what's valid. */
export function parseSettings(stored: unknown): Settings {
  if (typeof stored !== "object" || stored === null) return defaultSettings;
  const quality = "quality" in stored ? stored.quality : undefined;
  const ask = "askWhereToSave" in stored ? stored.askWhereToSave : undefined;
  return {
    quality: QUALITIES.find((q) => q === quality) ?? defaultSettings.quality,
    askWhereToSave: typeof ask === "boolean" ? ask : defaultSettings.askWhereToSave,
  };
}
