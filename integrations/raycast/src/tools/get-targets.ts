import { DOCUMENT_PACK_HINT, getPresets, inspectFiles } from "../lib/convt";
import { getBinary } from "../lib/preferences";

type Input = {
  /** Absolute paths to existing local files or folders. All returned targets work for every file named directly and for at least one file in the folders. */
  files: string[];
  /** Also look in subfolders of folder inputs. Pass the same value to convert-files. */
  recursive?: boolean;
};

/** Identify file types by extension and return supported targets, suggested target IDs, and saved presets from Convt. */
export default async function tool(input: Input) {
  const binary = await getBinary();
  const inspection = await inspectFiles(binary, input.files, { recursive: input.recursive });
  // Presets are optional; a broken presets folder should not hide the targets.
  const presets = await getPresets(binary).catch(() => []);
  return {
    ...inspection,
    presets,
    ...(inspection.documentPackMissing ? { hint: DOCUMENT_PACK_HINT } : {}),
  };
}
