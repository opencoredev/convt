import {
  Convt,
  ConvtCancellationError,
  formats,
  Conversion,
  type Format,
  type Job,
} from "@convt/sdk";
import { createFileRoute, useRouter } from "@tanstack/react-router";
import { useEffect, useId, useRef, useState, type ReactNode } from "react";
import {
  Card,
  Meter,
  PageTitle,
  PrimaryButton,
  PrimaryLink,
  SecondaryButton,
  SecondaryLink,
  TextButton,
  cx,
  focusRing,
} from "#/components/app/ui";
import capabilities from "#/generated/cloud-formats.json";
import { fetchCloudAccess, fetchCloudCredential } from "#/server/cloud-fns";

export const Route = createFileRoute("/_app/_shell/dashboard/api_/convert")({
  head: () => ({ meta: [{ title: "Cloud converter · convt" }] }),
  loader: () => fetchCloudAccess(),
  component: Converter,
});

const MAX_BYTES = 2_000_000_000;

type Stage = "idle" | "uploading" | "queued" | "running" | "succeeded" | "failed" | "cancelled";
const steps = [
  { stage: "uploading", label: "Upload" },
  { stage: "queued", label: "Queue" },
  { stage: "running", label: "Convert" },
  { stage: "succeeded", label: "Ready" },
] as const;

const gb = (bytes: number) => `${(bytes / 1e9).toFixed(2)} GB`;
function size(bytes: number) {
  if (bytes >= 1e9) return `${(bytes / 1e9).toFixed(2)} GB`;
  if (bytes >= 1e6) return `${(bytes / 1e6).toFixed(1)} MB`;
  return `${Math.max(1, Math.round(bytes / 1e3))} KB`;
}
const detect = (file: File | null) =>
  formats.find((f) =>
    (f.extensions as readonly string[]).includes(file?.name.split(".").pop()?.toLowerCase() ?? ""),
  );
const targetsFor = (id: string | undefined) =>
  (capabilities.formats.find((f) => f.id === id)?.targets ?? []) as Format[];

function Converter() {
  const access = Route.useLoaderData();
  const router = useRouter();
  const [file, setFile] = useState<File | null>(null);
  const [target, setTarget] = useState<Format | null>(null);
  const [stage, setStage] = useState<Stage>("idle");
  const [job, setJob] = useState<Job | null>(null);
  const [result, setResult] = useState<Conversion | null>(null);
  const [error, setError] = useState("");
  const [pendingCancel, setPendingCancel] = useState<string | null>(null);
  const [dragging, setDragging] = useState(false);
  const controller = useRef<AbortController | null>(null);
  const fileInput = useRef<HTMLInputElement>(null);
  useEffect(() => () => controller.current?.abort(), []);
  const inputFormat = detect(file);
  const targets = targetsFor(inputFormat?.id);
  const busy = stage === "uploading" || stage === "queued" || stage === "running";

  function choose(next: File | null) {
    setFile(next);
    const reachable = targetsFor(detect(next)?.id);
    setTarget((current) => (current && reachable.includes(current) ? current : null));
    setResult(null);
    setError("");
    setJob(null);
    setStage("idle");
  }

  async function convert() {
    if (!file || !inputFormat || !target) return;
    setError("");
    setPendingCancel(null);
    setResult(null);
    setJob(null);
    if (file.size > MAX_BYTES) {
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
          setStage(j.status === "created" || j.status === "uploaded" ? "uploading" : j.status);
        },
      });
      setResult(converted);
      setStage("succeeded");
      await router.invalidate();
    } catch (e) {
      const cancelled = controller.current?.signal.aborted;
      setStage(cancelled ? "cancelled" : "failed");
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
        setStage("succeeded");
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
  function reset() {
    choose(null);
    if (fileInput.current) fileInput.current.value = "";
  }

  const remaining = Math.max(0, access.limit - access.used - access.reserved);

  return (
    <div className="flex max-w-3xl flex-col gap-6">
      <nav aria-label="Breadcrumb" className="text-[13px]/4 text-ink-2">
        <a href="/dashboard/api" className={cx("rounded-sm hover:text-ink", focusRing)}>
          API
        </a>
        <span aria-hidden="true" className="px-1.5 text-ink-3">
          /
        </span>
        <span aria-current="page" className="text-ink">
          Cloud converter
        </span>
      </nav>
      <header className="flex flex-col gap-2">
        <PageTitle>Cloud converter</PageTitle>
        <p className="max-w-[560px] text-[14px]/[21px] text-ink-2">
          Convert from your browser or phone with Pro. Your file is uploaded to convt cloud storage
          for this conversion and deleted after 24 hours. The desktop app converts without uploading
          anything.
        </p>
      </header>

      <Card className="flex flex-col gap-3 px-5 py-4">
        <div className="flex flex-wrap items-baseline justify-between gap-x-4 gap-y-1">
          <p className="text-[13px]/5">
            <span className="font-mono font-medium">{gb(access.used)}</span>
            <span className="text-ink-2"> of 50 GB used this month</span>
          </p>
          <p className="text-xs/4 text-ink-3">
            {access.reserved > 0 ? `${gb(access.reserved)} reserved · ` : ""}2 GB per file
          </p>
        </div>
        <Meter
          used={access.used}
          reserved={access.reserved}
          limit={access.limit}
          label={`${gb(access.used)} of 50 GB monthly cloud allowance used`}
        />
      </Card>

      {!access.allowed ? (
        <EmptyState
          title="Cloud conversion needs paid Pro"
          action={
            <SecondaryLink href="/dashboard/billing" className="self-start">
              View Pro billing
            </SecondaryLink>
          }
        >
          {access.state === "trialing"
            ? "Cloud conversions start after your trial becomes a paid subscription."
            : "An active Pro subscription includes 50 GB of input each month."}
        </EmptyState>
      ) : !access.configured ? (
        <EmptyState title="Cloud conversion is being connected">
          You can use the desktop app now; it converts on your machine.
        </EmptyState>
      ) : (
        <Card className="flex flex-col">
          <div className="flex flex-col gap-3 p-5 sm:p-6">
            <StepLabel n={1} htmlFor="cloud-file">
              Choose a file
            </StepLabel>
            <input
              ref={fileInput}
              id="cloud-file"
              type="file"
              disabled={busy}
              onChange={(e) => choose(e.target.files?.[0] ?? null)}
              className="peer sr-only"
            />
            {file ? (
              <div className="flex items-center gap-3 rounded-xl bg-sunken px-4 py-3 ring-1 ring-line">
                <span
                  aria-hidden="true"
                  className="flex size-9 shrink-0 items-center justify-center rounded-lg bg-raised font-mono text-[10px]/3 font-semibold text-ink-2 uppercase ring-1 ring-line"
                >
                  {inputFormat?.id.slice(0, 4) ?? "?"}
                </span>
                <span className="flex min-w-0 flex-1 flex-col gap-0.5">
                  <span className="truncate text-[13px]/4 font-medium">{file.name}</span>
                  <span className="text-xs/4 text-ink-2">
                    {size(file.size)}
                    {inputFormat ? ` · ${inputFormat.name}` : ""}
                  </span>
                </span>
                <TextButton tone="muted" disabled={busy} onClick={() => fileInput.current?.click()}>
                  Change
                </TextButton>
              </div>
            ) : (
              <label
                htmlFor="cloud-file"
                onDragOver={(e) => {
                  e.preventDefault();
                  setDragging(true);
                }}
                onDragLeave={() => setDragging(false)}
                onDrop={(e) => {
                  e.preventDefault();
                  setDragging(false);
                  choose(e.dataTransfer.files[0] ?? null);
                }}
                className={cx(
                  "flex cursor-pointer flex-col items-center gap-2 rounded-xl border border-dashed px-6 py-10 text-center transition-colors peer-focus-visible:ring-2 peer-focus-visible:ring-green",
                  dragging
                    ? "border-green bg-green-tint"
                    : "border-line-strong bg-sunken hover:bg-hover",
                )}
              >
                <UploadIcon />
                <span className="text-[14px]/5 font-medium">
                  Drop a file here, or <span className="text-green">browse</span>
                </span>
                <span className="text-xs/4 text-balance text-ink-3">
                  Images, documents, video and audio, up to 2&nbsp;GB
                </span>
              </label>
            )}
            {file && !inputFormat && (
              <p role="alert" className="text-[13px]/5 text-error">
                This file extension is not supported. Choose a supported image, document, video or
                audio file.
              </p>
            )}
          </div>

          <fieldset
            disabled={busy || !inputFormat}
            className="flex flex-col gap-3 border-t border-line p-5 disabled:opacity-100 sm:p-6"
          >
            <legend className="contents">
              <StepLabel n={2}>Convert to</StepLabel>
            </legend>
            {inputFormat ? (
              targets.length ? (
                <div className="flex flex-wrap gap-2">
                  {targets.map((id) => {
                    const f = formats.find((x) => x.id === id);
                    return (
                      <label
                        key={id}
                        className={cx(
                          "relative flex cursor-pointer items-center gap-2 rounded-lg px-3 py-2 text-[13px]/4 ring-1 transition-colors has-[:disabled]:cursor-not-allowed has-[:focus-visible]:ring-2 has-[:focus-visible]:ring-green",
                          target === id
                            ? "bg-green-tint font-medium text-ink ring-green"
                            : "bg-raised text-ink-2 ring-line hover:bg-hover hover:text-ink",
                        )}
                      >
                        <input
                          type="radio"
                          name="cloud-target"
                          value={id}
                          checked={target === id}
                          onChange={() => setTarget(id)}
                          className="sr-only"
                        />
                        <span className="font-mono text-[12px] uppercase">{id}</span>
                        {f && !f.name.toLowerCase().includes(id) && (
                          <span className="hidden text-xs text-ink-3 sm:inline">{f.name}</span>
                        )}
                      </label>
                    );
                  })}
                </div>
              ) : (
                <p className="text-[13px]/5 text-ink-2">
                  The cloud can't convert {inputFormat.name} files yet. The desktop app may.
                </p>
              )
            ) : (
              <p className="text-[13px]/5 text-ink-3">
                Choose a file to see the formats it can convert to.
              </p>
            )}
          </fieldset>

          <div className="flex flex-col gap-4 border-t border-line p-5 sm:p-6">
            {stage !== "idle" && stage !== "failed" && stage !== "cancelled" && (
              <Progress stage={stage} job={job} />
            )}

            {error && (
              <div
                role="alert"
                className={cx(
                  "flex flex-col gap-3 rounded-xl px-4 py-3 text-[13px]/5 ring-1",
                  stage === "succeeded" || (stage === "cancelled" && !pendingCancel)
                    ? "bg-sunken text-ink-2 ring-line"
                    : "bg-[#fbeceb] text-error ring-error-line dark:bg-[#33171a]",
                )}
              >
                <p>{error}</p>
                <div className="flex flex-wrap gap-4">
                  {pendingCancel && (
                    <TextButton tone="danger" className="font-medium" onClick={retryCancel}>
                      Retry cancellation
                    </TextButton>
                  )}
                  {!pendingCancel && stage === "failed" && file && inputFormat && target && (
                    <TextButton tone="ink" className="font-medium" onClick={convert}>
                      Try again
                    </TextButton>
                  )}
                </div>
              </div>
            )}

            {result && (
              <div
                role="status"
                className="flex flex-col gap-3 rounded-xl bg-green-tint px-4 py-4 ring-1 ring-green-line"
              >
                <div className="flex flex-col gap-1">
                  <h2 className="text-[14px]/5 font-semibold">Conversion complete</h2>
                  <p className="text-xs/4 text-ink-2">
                    Download your files now. These links expire in 5 minutes.
                  </p>
                </div>
                <ul className="flex flex-col gap-2">
                  {result.outputs.map((output) => (
                    <li key={output.url}>
                      <PrimaryLink
                        href={output.url}
                        download={output.name}
                        className="max-w-full gap-2 break-all"
                      >
                        Download {output.name}
                      </PrimaryLink>
                    </li>
                  ))}
                </ul>
              </div>
            )}

            <div className="flex flex-wrap items-center gap-3">
              {result ? (
                <SecondaryButton onClick={reset}>Convert another file</SecondaryButton>
              ) : (
                <PrimaryButton
                  disabled={busy || !!pendingCancel || !file || !inputFormat || !target}
                  onClick={convert}
                  className="h-9"
                >
                  {busy
                    ? "Converting…"
                    : target
                      ? `Convert to ${target.toUpperCase()}`
                      : "Upload and convert"}
                </PrimaryButton>
              )}
              {busy && (
                <TextButton tone="danger" onClick={() => controller.current?.abort()}>
                  Cancel conversion
                </TextButton>
              )}
              {!busy && !result && file && (
                <p className="text-xs/4 text-ink-3">
                  Uses {size(file.size)} of your {gb(remaining)} left this month.
                </p>
              )}
            </div>
          </div>
        </Card>
      )}
    </div>
  );
}

function StepLabel({ n, htmlFor, children }: { n: number; htmlFor?: string; children: ReactNode }) {
  const Tag = htmlFor ? "label" : "span";
  return (
    <Tag htmlFor={htmlFor} className="flex items-center gap-2.5 text-[14px]/5 font-semibold">
      <span
        aria-hidden="true"
        className="flex size-5 items-center justify-center rounded-full bg-chip font-mono text-[11px] text-ink-2 ring-1 ring-chip-line"
      >
        {n}
      </span>
      {children}
    </Tag>
  );
}

function Progress({ stage, job }: { stage: Stage; job: Job | null }) {
  const id = useId();
  const current = steps.findIndex((s) => s.stage === stage);
  const message =
    stage === "uploading"
      ? "Uploading your file…"
      : stage === "queued"
        ? "Your conversion is queued."
        : stage === "running"
          ? "Converting your file…"
          : "Your files are ready.";
  return (
    <div className="flex flex-col gap-3">
      <ol aria-labelledby={id} className="grid grid-cols-4 gap-2">
        {steps.map((step, i) => {
          const done = i < current || stage === "succeeded";
          const active = i === current && stage !== "succeeded";
          return (
            <li
              key={step.stage}
              aria-current={active ? "step" : undefined}
              className="flex flex-col gap-1.5"
            >
              <span
                className={cx(
                  "h-1 rounded-full",
                  done ? "bg-green" : active ? "animate-pulse bg-green/60" : "bg-hover",
                )}
              />
              <span
                className={cx("text-xs/4", done || active ? "font-medium text-ink" : "text-ink-3")}
              >
                {step.label}
              </span>
            </li>
          );
        })}
      </ol>
      <div id={id} role="status" aria-live="polite" className="text-[13px]/5">
        <p>{message}</p>
        {stage !== "succeeded" && (
          <p className="text-xs/4 text-ink-2">
            {job
              ? `${job.attempt > 1 ? `Retrying, attempt ${job.attempt}. ` : ""}Keep this page open until your download is ready.`
              : "Your file goes to convt cloud storage for this conversion."}
          </p>
        )}
      </div>
    </div>
  );
}

function EmptyState({
  title,
  children,
  action,
}: {
  title: string;
  children: ReactNode;
  action?: ReactNode;
}) {
  return (
    <Card className="flex flex-col items-start gap-3 p-6">
      <span
        aria-hidden="true"
        className="flex size-9 items-center justify-center rounded-lg bg-sunken text-ink-2 ring-1 ring-line"
      >
        <UploadIcon small />
      </span>
      <div className="flex flex-col gap-1">
        <h2 className="text-[15px]/5 font-semibold">{title}</h2>
        <p className="text-[13px]/5 text-ink-2">{children}</p>
      </div>
      {action}
    </Card>
  );
}

function UploadIcon({ small }: { small?: boolean }) {
  const s = small ? 16 : 24;
  return (
    <svg width={s} height={s} viewBox="0 0 24 24" aria-hidden="true" className="text-ink-2">
      <path
        d="M12 15V4m0 0L8 8m4-4 4 4M5 14v4a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2v-4"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.6"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}
