import {
  Convt,
  ConvtCancellationError,
  formats,
  Conversion,
  type Format,
  type Job,
} from "@convt/sdk";
import { createFileRoute, useRouter } from "@tanstack/react-router";
import { useEffect, useRef, useState } from "react";
import {
  Card,
  PageTitle,
  PrimaryButton,
  SecondaryLink,
  TextButton,
  focusRing,
} from "#/components/app/ui";
import capabilities from "#/generated/cloud-formats.json";
import { fetchCloudAccess, fetchCloudCredential } from "#/server/cloud-fns";

export const Route = createFileRoute("/_app/_shell/dashboard/api_/convert")({
  head: () => ({ meta: [{ title: "Cloud converter · convt" }] }),
  loader: () => fetchCloudAccess(),
  component: Converter,
});
function Converter() {
  const access = Route.useLoaderData();
  const router = useRouter();
  const [file, setFile] = useState<File | null>(null);
  const [target, setTarget] = useState<Format>("pdf");
  const [stage, setStage] = useState("idle");
  const [job, setJob] = useState<Job | null>(null);
  const [result, setResult] = useState<Conversion | null>(null);
  const [error, setError] = useState("");
  const [pendingCancel, setPendingCancel] = useState<string | null>(null);
  const controller = useRef<AbortController | null>(null);
  useEffect(() => () => controller.current?.abort(), []);
  const inputFormat = formats.find((f) =>
    (f.extensions as readonly string[]).includes(file?.name.split(".").pop()?.toLowerCase() ?? ""),
  );
  const busy = stage === "uploading" || stage === "queued" || stage === "running";
  async function convert() {
    if (!file || !inputFormat) return;
    setError("");
    setPendingCancel(null);
    setResult(null);
    setJob(null);
    if (file.size > 2_000_000_000) {
      setError("This file exceeds the 2 GB limit. Use the desktop app for larger files.");
      return;
    }
    if (file.size + access.used + access.reserved > access.limit) {
      setError("Monthly allowance reached. Choose a smaller file or wait until next month.");
      return;
    }
    controller.current = new AbortController();
    setStage("uploading");
    try {
      let credential = await fetchCloudCredential();
      let issued = Date.now();
      const client = new Convt({
        baseUrl: credential.baseUrl,
        token: async () => {
          if (Date.now() - issued > 240_000) {
            credential = await fetchCloudCredential();
            issued = Date.now();
          }
          return credential.token;
        },
      });
      const converted = await client.convert(file, {
        to: target,
        signal: controller.current.signal,
        onProgress: (j) => {
          setJob(j);
          setStage(j.status);
        },
      });
      setResult(converted);
      setStage("succeeded");
      await router.invalidate();
    } catch (e) {
      setStage("failed");
      if (e instanceof ConvtCancellationError) setPendingCancel(e.jobId);
      setError(e instanceof Error ? e.message : "Conversion failed. Try again.");
      await router.invalidate();
    }
  }
  async function retryCancel() {
    if (!pendingCancel) return;
    try {
      const credential = await fetchCloudCredential();
      const client = new Convt({ apiKey: credential.token, baseUrl: credential.baseUrl });
      const terminal = await client.cancel(pendingCancel);
      if (terminal.status === "succeeded") {
        const outputs = await client.download(pendingCancel);
        setResult(new Conversion(terminal, outputs.outputs));
        setError("Conversion completed before cancellation. Your allowance was settled.");
      } else if (terminal.status === "cancelled" || terminal.status === "failed") {
        setError("Conversion cancelled. Your allowance was released.");
      } else throw new Error("Cancellation is not confirmed yet. Retry shortly.");
      setPendingCancel(null);
      await router.invalidate();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Retry cancellation shortly.");
    }
  }
  return (
    <div className="flex max-w-3xl flex-col gap-6">
      <div className="flex flex-col gap-2">
        <PageTitle>Cloud converter</PageTitle>
        <p className="text-sm text-ink-2">
          Convert from your browser or phone with Pro. Files are deleted after 24 hours.
        </p>
      </div>
      <SecondaryLink href="/dashboard/api" className="self-start">
        API keys and usage
      </SecondaryLink>
      <Card className="flex flex-wrap justify-between gap-3 p-5 text-sm">
        <span>
          <span className="font-mono">{(access.used / 1e9).toFixed(2)} GB</span>
          <span className="text-ink-2"> of 50 GB used this month</span>
        </span>
        <span className="text-ink-2">
          2 GB per file
          {access.reserved > 0 ? ` · ${(access.reserved / 1e9).toFixed(2)} GB reserved` : ""}
        </span>
      </Card>
      {!access.allowed ? (
        <Card className="flex flex-col gap-3 p-6">
          <h2 className="text-base font-semibold">Cloud conversion needs paid Pro</h2>
          <p className="text-sm text-ink-2">
            {access.state === "trialing"
              ? "Cloud conversions start after your trial becomes a paid subscription."
              : "An active Pro subscription includes 50 GB of input each month."}
          </p>
          <SecondaryLink href="/dashboard/billing" className="self-start">
            View Pro billing
          </SecondaryLink>
        </Card>
      ) : !access.configured ? (
        <Card className="p-6 text-sm text-ink-2">
          Cloud conversion is being connected. You can use the desktop app now.
        </Card>
      ) : (
        <Card className="flex flex-col gap-5 p-6">
          <div className="flex flex-col gap-2">
            <label htmlFor="cloud-file" className="text-sm font-semibold">
              Choose a file
            </label>
            <input
              id="cloud-file"
              type="file"
              disabled={busy}
              onChange={(e) => {
                const next = e.target.files?.[0] ?? null;
                setFile(next);
                const source = formats.find((f) =>
                  (f.extensions as readonly string[]).includes(
                    next?.name.split(".").pop()?.toLowerCase() ?? "",
                  ),
                );
                const reachable =
                  capabilities.formats.find((f) => f.id === source?.id)?.targets ?? [];
                if (!reachable.includes(target)) setTarget((reachable[0] ?? "pdf") as Format);
                setResult(null);
                setError("");
                setStage("idle");
              }}
              className={`w-full min-w-0 rounded-lg border border-dashed border-line bg-sunken p-4 text-sm file:mr-3 file:rounded file:border-0 file:bg-hover file:px-3 file:py-2 file:text-ink ${focusRing}`}
            />
            {file && (
              <p className="break-all text-xs text-ink-2">
                {file.name} · {(file.size / 1e6).toFixed(2)} MB
              </p>
            )}
          </div>
          <div className="flex flex-col gap-2">
            <label htmlFor="cloud-target" className="text-sm font-semibold">
              Convert to
            </label>
            <select
              id="cloud-target"
              value={target}
              disabled={busy}
              onChange={(e) => setTarget(e.target.value as Format)}
              className={`w-full rounded-lg border border-line bg-page px-3 py-2.5 text-sm ${focusRing}`}
            >
              {formats
                .filter(
                  (f) =>
                    !inputFormat ||
                    capabilities.formats
                      .find((source) => source.id === inputFormat.id)
                      ?.targets.includes(f.id),
                )
                .map((f) => (
                  <option key={f.id} value={f.id}>
                    {f.name} ({f.id})
                  </option>
                ))}
            </select>
            <p className="text-xs text-ink-3">
              {file
                ? "Targets available for your file in the cloud."
                : "Choose a file to see the formats it can convert to."}
            </p>
          </div>
          <div className="flex items-center gap-4">
            <PrimaryButton
              disabled={busy || !!pendingCancel || !file || !inputFormat}
              onClick={convert}
            >
              {busy ? "Converting…" : "Upload and convert"}
            </PrimaryButton>
            {busy && (
              <TextButton tone="danger" onClick={() => controller.current?.abort()}>
                Cancel conversion
              </TextButton>
            )}
          </div>
          {file && !inputFormat && (
            <p role="alert" className="text-sm text-error">
              This file extension is not supported. Choose a supported image, document, video or
              audio file.
            </p>
          )}
          {busy && (
            <div role="status" aria-live="polite" className="rounded-lg bg-sunken p-4 text-sm">
              <p>
                {stage === "uploading"
                  ? "Uploading your file…"
                  : stage === "queued"
                    ? "Your conversion is queued."
                    : "Converting your file…"}
              </p>
              <p className="mt-1 text-xs text-ink-2">
                {job
                  ? `Attempt ${job.attempt || 1}. Keep this page open until your download is ready.`
                  : "Your file goes to convt cloud storage for this conversion."}
              </p>
            </div>
          )}
          {error && (
            <p role="alert" className="rounded-lg bg-sunken p-4 text-sm text-error">
              {error}
            </p>
          )}
          {pendingCancel && (
            <TextButton tone="danger" className="self-start" onClick={retryCancel}>
              Retry cancellation
            </TextButton>
          )}
          {result && (
            <div className="flex flex-col gap-3 rounded-lg bg-green-tint p-4" role="status">
              <h2 className="text-sm font-semibold">Conversion complete</h2>
              <p className="text-xs text-ink-2">
                Download your files now. These links expire in 5 minutes.
              </p>
              {result.outputs.map((output) => (
                <a
                  key={output.url}
                  href={output.url}
                  download={output.name}
                  className={`break-all text-sm font-medium text-green underline ${focusRing}`}
                >
                  Download {output.name}
                </a>
              ))}
            </div>
          )}
        </Card>
      )}
    </div>
  );
}
