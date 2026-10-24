import {
  Action,
  ActionPanel,
  Color,
  Detail,
  Form,
  getSelectedFinderItems,
  Icon,
  List,
  openExtensionPreferences,
  showToast,
  Toast,
  useNavigation,
} from "@raycast/api";
import { showFailureToast, usePromise } from "@raycast/utils";
import { useEffect, useRef, useState } from "react";
import path from "node:path";
import {
  AUDIO_BITRATES,
  CODEC_TARGET_IDS,
  convertFiles,
  getPresets,
  inspectFiles,
  installDocumentPack,
  keepsTransparency,
  validateOptions,
  VIDEO_CODECS,
  VIDEO_HEIGHTS,
  type ConversionProgress,
  type ConversionResult,
  type FileInspection,
  type Preset,
} from "./lib/convt";
import { getBinary, getJobs } from "./lib/preferences";

const DOWNLOAD_URL = "https://convt.app/download";
const INSTALL_COMMAND = "brew tap opencoredev/convt https://github.com/opencoredev/convt && brew install --cask convt";

export default function Command() {
  const { data: binary, error, isLoading, revalidate } = usePromise(getBinary);
  const [files, setFiles] = useState<string[]>([]);
  const selectionChanged = useRef(false);

  useEffect(() => {
    if (process.platform !== "darwin") return;
    let active = true;
    getSelectedFinderItems()
      .then((items) => {
        if (active && !selectionChanged.current) setFiles(items.map((item) => item.path));
      })
      .catch(() => {
        /* The file picker is available when Finder has no selection. */
      });
    return () => {
      active = false;
    };
  }, []);

  if (error) {
    return (
      <Detail
        markdown={`# Set up Convt\n\n${error.message}\n\nInstall [Convt](${DOWNLOAD_URL}), then try again. The CLI is included with the app.\n\n${process.platform === "darwin" ? `You can also install it with Homebrew:\n\n\`\`\`sh\n${INSTALL_COMMAND}\n\`\`\`\n\n` : ""}For a custom installation, set **Convt CLI Path** in this extension's preferences.\n\nConvt's packaged builds require a trial or license. Files are converted locally.`}
        actions={
          <ActionPanel>
            <Action.OpenInBrowser title="Download Convt" url={DOWNLOAD_URL} />
            <Action title="Check Installation Again" icon={Icon.ArrowClockwise} onAction={revalidate} />
            <Action title="Open Extension Preferences" icon={Icon.Gear} onAction={openExtensionPreferences} />
            {process.platform === "darwin" && (
              <Action.CopyToClipboard title="Copy Install Command" content={INSTALL_COMMAND} />
            )}
          </ActionPanel>
        }
      />
    );
  }
  if (!binary) return <Detail isLoading={isLoading} markdown="Checking Convt installation…" />;
  return (
    <Converter
      binary={binary}
      files={files}
      setFiles={(next) => {
        selectionChanged.current = true;
        setFiles(next);
      }}
    />
  );
}

type FormValues = {
  target: string;
  outputDirectory: string[];
  preset?: string;
  quality?: string;
  maxSize?: string;
  pages?: string;
  dpi?: string;
  background?: string;
  backgroundColor?: string;
  videoHeight?: string;
  videoCodec?: string;
  audioBitrate?: string;
  noAudio?: boolean;
  recursive?: boolean;
};

const NO_PRESET = "";
const DEFAULT_OPTION = "";
const CUSTOM_BACKGROUND = "custom";

type Discovery = { files: string[]; binary: string; recursive: boolean; reload: number } & (
  { status: "ready"; inspection: FileInspection } | { status: "error"; message: string }
);

function Converter({
  binary,
  files,
  setFiles,
}: {
  binary: string;
  files: string[];
  setFiles: (files: string[]) => void;
}) {
  const { push } = useNavigation();
  const [discovery, setDiscovery] = useState<Discovery>();
  const [manualTarget, setManualTarget] = useState("");
  const [outputDirectory, setOutputDirectory] = useState<string[]>([]);
  const [quality, setQuality] = useState("");
  const [maxSize, setMaxSize] = useState("");
  const [pages, setPages] = useState("");
  const [dpi, setDpi] = useState("");
  const [preset, setPreset] = useState(NO_PRESET);
  const [presets, setPresets] = useState<Preset[]>([]);
  const [background, setBackground] = useState(DEFAULT_OPTION);
  const [backgroundColor, setBackgroundColor] = useState("");
  const [videoHeight, setVideoHeight] = useState(DEFAULT_OPTION);
  const [videoCodec, setVideoCodec] = useState(DEFAULT_OPTION);
  const [audioBitrate, setAudioBitrate] = useState(DEFAULT_OPTION);
  const [noAudio, setNoAudio] = useState(false);
  const [recursive, setRecursive] = useState(false);
  const [reload, setReload] = useState(0);
  const [installingPack, setInstallingPack] = useState(false);
  const [requiredError, setRequiredError] = useState(false);
  const [progress, setProgress] = useState<ConversionProgress>();
  const [busy, setBusy] = useState(false);
  const controller = useRef<AbortController | null>(null);
  const current =
    discovery?.files === files &&
    discovery.binary === binary &&
    discovery.recursive === recursive &&
    discovery.reload === reload
      ? discovery
      : undefined;
  const inspection = current?.status === "ready" ? current.inspection : undefined;
  const formats = inspection?.targets ?? [];
  const loadingTargets = files.length > 0 && !current;
  // Removing the selected ID during loading makes Raycast select the first new option.
  const target =
    loadingTargets || formats.some((format) => format.id === manualTarget)
      ? manualTarget
      : (inspection?.suggestedTargetIds[0] ?? formats[0]?.id ?? "");
  const hasFolders = !!inspection?.folders.length;
  const fileError = !files.length
    ? requiredError
      ? "Choose at least one file or folder."
      : undefined
    : current?.status === "error"
      ? current.message
      : inspection?.sources.some((source) => !source.format)
        ? "Convt does not recognize every file extension. Choose supported files."
        : inspection && !formats.length
          ? inspection.documentPackMissing
            ? "Office documents need Convt's document pack or LibreOffice. Install the document pack from the actions menu."
            : hasFolders
              ? "Nothing in this selection converts with Convt's installed engines. Choose other files or folders."
              : "No shared output format. Choose compatible files and check that Convt's required engines are installed."
          : undefined;

  useEffect(() => {
    let active = true;
    const abort = new AbortController();
    setDiscovery(undefined);
    setRequiredError(false);
    if (!files.length) {
      setManualTarget("");
      return;
    }
    inspectFiles(binary, files, { signal: abort.signal, recursive })
      .then((inspection) => {
        if (!active) return;
        setDiscovery({ files, binary, recursive, reload, status: "ready", inspection });
        setManualTarget((previous) => (inspection.targets.some((format) => format.id === previous) ? previous : ""));
      })
      .catch((error: unknown) => {
        if (active)
          setDiscovery({
            files,
            binary,
            recursive,
            reload,
            status: "error",
            message: error instanceof Error ? error.message : String(error),
          });
      });
    return () => {
      active = false;
      abort.abort();
    };
  }, [binary, files, recursive, reload]);

  useEffect(() => {
    let active = true;
    const abort = new AbortController();
    getPresets(binary, abort.signal)
      .then((found) => {
        if (active) setPresets(found);
      })
      .catch(() => {
        /* Presets are optional; the conversion still works without them. */
      });
    return () => {
      active = false;
      abort.abort();
    };
  }, [binary]);

  useEffect(() => () => controller.current?.abort(), []);

  async function installPack() {
    if (installingPack) return;
    setInstallingPack(true);
    const toast = await showToast({ style: Toast.Style.Animated, title: "Installing document pack" });
    try {
      await installDocumentPack(binary, {
        onProgress: (progress) => {
          toast.message =
            progress.kind === "download"
              ? progress.total
                ? `Downloading… ${Math.floor((progress.bytes / progress.total) * 100)}%`
                : `Downloading… ${Math.floor(progress.bytes / 1_048_576)} MB`
              : progress.kind === "verifying"
                ? "Verifying download…"
                : "Installing…";
        },
      });
      toast.style = Toast.Style.Success;
      toast.title = "Document pack installed";
      toast.message = "Office documents can now be converted.";
      setReload((value) => value + 1);
    } catch (error) {
      toast.style = Toast.Style.Failure;
      toast.title = "Could not install the document pack";
      toast.message = error instanceof Error ? error.message : String(error);
    } finally {
      setInstallingPack(false);
    }
  }

  async function submit(values: FormValues) {
    if (controller.current || loadingTargets) return;
    if (!files.length) {
      setRequiredError(true);
      return;
    }
    if (!formats.some((format) => format.id === values.target)) {
      await showToast({ style: Toast.Style.Failure, title: "Choose an available output format" });
      return;
    }
    const number = (value?: string) => (value?.trim() ? Number(value) : undefined);
    const codec = VIDEO_CODECS.find((id) => id === values.videoCodec);
    const options = {
      preset: values.preset || undefined,
      quality: number(values.quality),
      maxSize: number(values.maxSize),
      dpi: number(values.dpi),
      pages: values.pages?.trim() || undefined,
      background:
        values.background === CUSTOM_BACKGROUND
          ? values.backgroundColor?.trim() || undefined
          : values.background || undefined,
      videoHeight: number(values.videoHeight),
      videoCodec: codec,
      audioBitrate: number(values.audioBitrate),
      noAudio: values.noAudio || undefined,
    };
    if (values.background === CUSTOM_BACKGROUND && !options.background) {
      await showToast({ style: Toast.Style.Failure, title: "Enter a background color, such as #ff8800" });
      return;
    }
    try {
      validateOptions(options);
    } catch (error) {
      await showFailureToast(error, { title: "Check conversion options" });
      return;
    }
    const abort = new AbortController();
    controller.current = abort;
    setBusy(true);
    setProgress(undefined);
    try {
      const converted = await convertFiles(
        binary,
        {
          files,
          target: values.target,
          outputDirectory: values.outputDirectory[0],
          recursive: hasFolders && !!values.recursive,
          jobs: getJobs(),
          ...options,
        },
        {
          signal: abort.signal,
          onProgress: setProgress,
        },
      );
      push(<Results results={converted} />);
      const done = converted.filter((result) => result.status === "done").length;
      await showToast({
        style: done === converted.length ? Toast.Style.Success : Toast.Style.Failure,
        title: `${done} of ${converted.length} files converted`,
        message: done === converted.length ? "Your converted files are ready." : "See results for details.",
      });
    } catch (error) {
      await showFailureToast(error, { title: "Conversion failed" });
    } finally {
      controller.current = null;
      setBusy(false);
      setProgress(undefined);
    }
  }

  const selectedFormat = formats.find((format) => format.id === target);
  const image = selectedFormat?.category === "image";
  const video = selectedFormat?.category === "video";
  const audio = selectedFormat?.category === "audio";
  const sourceFormatIds = new Set([
    ...files.map((file) => path.extname(file).slice(1).toLowerCase()),
    ...(inspection?.folders.flatMap((folder) => folder.formats.map(({ format }) => format.id)) ?? []),
  ]);
  const hasPdf = sourceFormatIds.has("pdf");
  const hasSvg = sourceFormatIds.has("svg");
  const sourceCounts = new Map<string, number>();
  const formatLabel = (format: { name: string; category: string }) =>
    format.name.toLowerCase() === format.category.toLowerCase() ? format.name : `${format.name} ${format.category}`;
  for (const source of inspection?.sources ?? []) {
    const label = source.format
      ? formatLabel(source.format)
      : source.extension
        ? `Unknown (.${source.extension})`
        : "No file extension";
    sourceCounts.set(label, (sourceCounts.get(label) ?? 0) + 1);
  }
  for (const folder of inspection?.folders ?? []) {
    for (const { format, files: count } of folder.formats) {
      const label = formatLabel(format);
      sourceCounts.set(label, (sourceCounts.get(label) ?? 0) + count);
    }
  }
  const folderCount = inspection?.folders.length ?? 0;
  const otherFiles = (inspection?.folders ?? []).reduce(
    (sum, folder) => sum + folder.files - folder.formats.reduce((known, entry) => known + entry.files, 0),
    0,
  );
  const sourceSummary = [
    ...[...sourceCounts].map(([label, count]) => `${label} · ${count} file${count === 1 ? "" : "s"}`),
    ...(otherFiles ? [`${otherFiles} other file${otherFiles === 1 ? "" : "s"} skipped`] : []),
  ].join("; ");
  const folderNote = folderCount
    ? ` Includes ${folderCount} folder${folderCount === 1 ? "" : "s"}${inspection?.folders.some((folder) => !folder.complete) ? " (partly scanned)" : ""}; files that can't become the chosen format are skipped.`
    : "";
  const documentPackMissing = !!inspection?.documentPackMissing;
  const transparentAllowed = !selectedFormat || keepsTransparency(selectedFormat);
  const suggested = (inspection?.suggestedTargetIds ?? []).flatMap((id) =>
    formats.filter((format) => format.id === id),
  );
  const others = formats.filter((format) => !inspection?.suggestedTargetIds.includes(format.id));
  const sections = [
    { title: "Suggested", formats: suggested },
    ...[...new Set(others.map((format) => format.category))].map((category) => ({
      title: category.charAt(0).toUpperCase() + category.slice(1),
      formats: others.filter((format) => format.category === category),
    })),
  ].filter((section) => section.formats.length);

  return (
    <Form
      isLoading={loadingTargets || busy}
      actions={
        <ActionPanel>
          {documentPackMissing && !formats.length && !busy && (
            <Action title="Install Document Pack" icon={Icon.Download} onAction={installPack} />
          )}
          {busy ? (
            <Action title="Cancel Conversion" icon={Icon.XMarkCircle} onAction={() => controller.current?.abort()} />
          ) : (
            <Action.SubmitForm title="Convert Files" icon={Icon.ArrowRight} onSubmit={submit} />
          )}
          {documentPackMissing && !!formats.length && !busy && (
            <Action title="Install Document Pack" icon={Icon.Download} onAction={installPack} />
          )}
          {documentPackMissing && <Action.OpenInBrowser title="Open Convt.app" url={DOWNLOAD_URL} />}
          {process.platform === "darwin" && (
            <Action
              title="Use Finder Selection"
              icon={Icon.Finder}
              onAction={async () => {
                try {
                  setFiles((await getSelectedFinderItems()).map((item) => item.path));
                } catch (error) {
                  await showFailureToast(error, { title: "Could not read Finder selection" });
                }
              }}
            />
          )}
          <Action title="Open Extension Preferences" icon={Icon.Gear} onAction={openExtensionPreferences} />
          <Action.OpenInBrowser title="Open Convt Help" url="https://github.com/opencoredev/convt#engines" />
        </ActionPanel>
      }
    >
      <Form.Description text="Convert files locally with Convt. Originals are preserved; existing output names receive a numbered suffix." />
      {busy && (
        <Form.Description
          title="Converting"
          text={
            progress
              ? `${path.basename(progress.input)} — ${progress.completed} of ${progress.total} completed${progress.fraction != null ? ` (${Math.floor(progress.fraction * 100)}%)` : ""}`
              : "Preparing conversion…"
          }
        />
      )}
      <Form.FilePicker
        id="files"
        title="Files or Folders"
        value={files}
        onChange={setFiles}
        error={fileError}
        canChooseDirectories
        allowMultipleSelection
      />
      {!!files.length && (
        <Form.Description
          title="Detected File Types"
          text={
            loadingTargets
              ? "Checking file extensions…"
              : sourceSummary
                ? `${sourceSummary}. Based on file extensions.${folderNote}`
                : folderCount
                  ? `No files found.${folderNote}`
                  : "Could not identify file types."
          }
        />
      )}
      {hasFolders && (
        <Form.Checkbox
          id="recursive"
          title="Folders"
          label="Include subfolders"
          value={recursive}
          onChange={setRecursive}
          info="Converts files in every subfolder. With an output folder, Convt keeps the subfolder structure."
        />
      )}
      {documentPackMissing && (
        <Form.Description
          title="Document Pack"
          text={`Office documents need Convt's document pack or LibreOffice, and neither is installed. Choose Install Document Pack in the actions menu (⌘K), or download Convt from convt.app.${formats.length ? " Until then, documents in folders are skipped." : ""}`}
        />
      )}
      <Form.Dropdown
        id="target"
        title="Output Format"
        value={target}
        onChange={(value) => {
          if (!loadingTargets && value !== target && formats.some((format) => format.id === value))
            setManualTarget(value);
        }}
        info={
          hasFolders
            ? "Every listed format works for each selected file and at least one file in the folders. Suggestions come from Convt."
            : "All formats work for every selected file. Suggestions come from Convt."
        }
      >
        {!formats.length && (
          <Form.Dropdown.Item
            value={target}
            title={
              loadingTargets
                ? "Finding available formats…"
                : files.length
                  ? "No available formats"
                  : "Choose files first"
            }
          />
        )}
        {sections.map((section) => (
          <Form.Dropdown.Section key={section.title} title={section.title}>
            {section.formats.map((format) => (
              <Form.Dropdown.Item
                key={format.id}
                value={format.id}
                title={`${format.name} (.${format.extensions[0] || format.id})`}
              />
            ))}
          </Form.Dropdown.Section>
        ))}
      </Form.Dropdown>
      <Form.FilePicker
        id="outputDirectory"
        title="Output Folder"
        value={outputDirectory}
        onChange={setOutputDirectory}
        canChooseDirectories
        canChooseFiles={false}
        allowMultipleSelection={false}
        info="Optional. Leave empty to save beside each original file."
      />
      {!!presets.length && (
        <Form.Dropdown
          id="preset"
          title="Preset"
          value={preset}
          onChange={(value) => {
            setPreset(value);
            const presetTarget = presets.find((entry) => entry.name === value)?.target;
            if (presetTarget && formats.some((format) => format.id === presetTarget)) setManualTarget(presetTarget);
          }}
          info="Saved Convt presets. Options you set below override the preset's."
        >
          <Form.Dropdown.Item value={NO_PRESET} title="None" />
          {presets.map((entry) => (
            <Form.Dropdown.Item
              key={entry.name}
              value={entry.name}
              title={entry.target ? `${entry.name} (${entry.target})` : entry.name}
            />
          ))}
        </Form.Dropdown>
      )}
      {(image || video) && (
        <Form.TextField
          id="quality"
          title="Quality"
          value={quality}
          onChange={setQuality}
          placeholder="Default (1–100)"
          info="Applies to lossy formats supported by Convt."
        />
      )}
      {image && (
        <Form.TextField
          id="maxSize"
          title="Maximum Image Size"
          value={maxSize}
          onChange={setMaxSize}
          placeholder="Original size"
          info="Longest edge in pixels. Smaller images are not enlarged."
        />
      )}
      {hasPdf && image && (
        <Form.TextField
          id="pages"
          title="PDF Pages"
          value={pages}
          onChange={setPages}
          placeholder="All pages"
          info="Use a page or range, such as 2, 1-3, or 4-. Each page becomes a separate file."
        />
      )}
      {(hasPdf || hasSvg) && image && (
        <Form.TextField id="dpi" title="Render DPI" value={dpi} onChange={setDpi} placeholder="Default (18–1200)" />
      )}
      {image && (
        <Form.Dropdown
          id="background"
          title="Background"
          value={!transparentAllowed && background === "transparent" ? DEFAULT_OPTION : background}
          onChange={setBackground}
          info="What transparent areas become. By default, transparency is kept where the format can store it, and white is used where it can't."
        >
          <Form.Dropdown.Item value={DEFAULT_OPTION} title="Default" />
          {transparentAllowed && <Form.Dropdown.Item value="transparent" title="Transparent" />}
          <Form.Dropdown.Item value="white" title="White" />
          <Form.Dropdown.Item value="black" title="Black" />
          <Form.Dropdown.Item value={CUSTOM_BACKGROUND} title="Custom Color…" />
        </Form.Dropdown>
      )}
      {image && background === CUSTOM_BACKGROUND && (
        <Form.TextField
          id="backgroundColor"
          title="Background Color"
          value={backgroundColor}
          onChange={setBackgroundColor}
          placeholder="#ff8800"
        />
      )}
      {video && (
        <Form.Dropdown
          id="videoHeight"
          title="Maximum Video Height"
          value={videoHeight}
          onChange={setVideoHeight}
          info="Scales larger videos down, keeping the aspect ratio. Smaller videos are not enlarged."
        >
          <Form.Dropdown.Item value={DEFAULT_OPTION} title="Original" />
          {VIDEO_HEIGHTS.map((height) => (
            <Form.Dropdown.Item key={height} value={String(height)} title={`${height}p`} />
          ))}
        </Form.Dropdown>
      )}
      {video && CODEC_TARGET_IDS.has(target) && (
        <Form.Dropdown
          id="videoCodec"
          title="Video Codec"
          value={videoCodec}
          onChange={setVideoCodec}
          info="H.264 plays nearly everywhere. HEVC makes smaller files for newer players."
        >
          <Form.Dropdown.Item value={DEFAULT_OPTION} title="Default (H.264)" />
          <Form.Dropdown.Item value="h264" title="H.264" />
          <Form.Dropdown.Item value="hevc" title="HEVC (H.265)" />
        </Form.Dropdown>
      )}
      {(video || audio) && (
        <Form.Dropdown id="audioBitrate" title="Audio Bitrate" value={audioBitrate} onChange={setAudioBitrate}>
          <Form.Dropdown.Item value={DEFAULT_OPTION} title="Default" />
          {AUDIO_BITRATES.map((bitrate) => (
            <Form.Dropdown.Item key={bitrate} value={String(bitrate)} title={`${bitrate} kbps`} />
          ))}
        </Form.Dropdown>
      )}
      {video && <Form.Checkbox id="noAudio" title="Audio" label="Remove audio" value={noAudio} onChange={setNoAudio} />}
    </Form>
  );
}

function Results({ results }: { results: ConversionResult[] }) {
  const { pop } = useNavigation();
  const outputs = results.flatMap((result) => (result.status === "done" ? result.outputs : []));
  return (
    <List navigationTitle="Conversion Results" searchBarPlaceholder="Search converted files…">
      <List.Section title="Converted Files" subtitle={`${outputs.length} output${outputs.length === 1 ? "" : "s"}`}>
        {results.flatMap((result) =>
          result.status === "done"
            ? result.outputs.map((output) => (
                <List.Item
                  key={output}
                  title={path.basename(output)}
                  subtitle={path.dirname(output)}
                  icon={{ fileIcon: output }}
                  accessories={[
                    {
                      icon: { source: Icon.CheckCircle, tintColor: Color.Green },
                      tooltip: `Converted from ${path.basename(result.input)}`,
                    },
                  ]}
                  actions={
                    <ActionPanel>
                      <Action.ShowInFinder
                        title={process.platform === "win32" ? "Show in File Explorer" : "Show in Finder"}
                        path={output}
                      />
                      <Action.Open title="Open Converted File" target={output} />
                      <Action.CopyToClipboard title="Copy File" content={{ file: output }} />
                      <Action.CopyToClipboard title="Copy Path" content={output} />
                      <Action.CopyToClipboard title="Copy All Output Paths" content={outputs.join("\n")} />
                      <Action title="Convert More Files" icon={Icon.ArrowCounterClockwise} onAction={pop} />
                    </ActionPanel>
                  }
                />
              ))
            : [],
        )}
      </List.Section>
      <List.Section title="Not Converted">
        {results
          .flatMap((result, index) => (result.status === "done" ? [] : [{ result, index }]))
          .map(({ result, index }) => (
            <List.Item
              key={`${index}:${result.input}`}
              title={path.basename(result.input)}
              subtitle={result.message}
              icon={result.status === "cancelled" ? Icon.XMarkCircle : Icon.Warning}
              actions={
                <ActionPanel>
                  <Action.Push
                    title="Show Error"
                    target={
                      <Detail
                        markdown={`# ${result.status === "cancelled" ? "Conversion Cancelled" : "Conversion Failed"}\n\n${result.message}`}
                      />
                    }
                  />
                  <Action.CopyToClipboard title="Copy Error" content={result.message} />
                  <Action title="Back to Conversion" icon={Icon.ArrowCounterClockwise} onAction={pop} />
                  <Action.OpenInBrowser title="Open Convt Help" url="https://github.com/opencoredev/convt#engines" />
                </ActionPanel>
              }
            />
          ))}
      </List.Section>
    </List>
  );
}
