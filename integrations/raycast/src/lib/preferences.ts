import { getPreferenceValues } from "@raycast/api";
import { resolveBinary, type ConversionRequest } from "./convt";

export function getBinary() {
  return resolveBinary(getPreferenceValues<Preferences>().binaryPath);
}

/** Files converted at once, from the Parallel Conversions preference. */
export function getJobs(): NonNullable<ConversionRequest["jobs"]> {
  const value = getPreferenceValues<Preferences>().jobs;
  return value === "auto" ? "auto" : Number(value) || 1;
}
