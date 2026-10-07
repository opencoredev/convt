import type { ArtifactKind } from "#/lib/release-manifest";
import { installSteps } from "#/lib/install-guide";
import { osNames, type Os } from "#/lib/platform";

import { cx } from "#/components/app/ui";

/** Three install panels for the selected OS and file kind, in the spirit of a post-download guide. */
export function InstallGuide({ os, kind }: { os: Os; kind: ArtifactKind }) {
  const steps = installSteps(os, kind);
  return (
    <section aria-labelledby="install-title" className="flex flex-col items-center gap-10">
      <div className="flex max-w-[560px] flex-col items-center gap-3 text-center">
        <h2
          id="install-title"
          className="text-[34px]/10 font-semibold tracking-[-0.03em] text-balance md:text-[40px]/12"
        >
          You're almost there
        </h2>
        <p className="text-[17px]/[26px] text-ink-2">
          Download convt for {osNames[os]}, then finish the install in three steps.
        </p>
      </div>
      <ol className="grid w-full gap-5 md:grid-cols-3">
        {steps.map((step, index) => (
          <li key={step.title} className="flex flex-col gap-4">
            <div
              className={cx(
                "flex aspect-[5/4] items-center justify-center overflow-clip rounded-2xl bg-sunken shadow-[inset_0_0_0_1px_var(--line)]",
                index === 1 &&
                  "bg-[radial-gradient(circle_at_50%_42%,var(--green-tint),var(--sunken)_70%)]",
              )}
            >
              <InstallArt os={os} kind={kind} step={index} />
            </div>
            <div className="flex flex-col gap-1.5 text-center">
              <p className="text-[15px]/6 font-medium">{step.title}</p>
              <p className="text-[13px]/5 text-ink-2">{step.body}</p>
            </div>
          </li>
        ))}
      </ol>
    </section>
  );
}

function fileLabel(kind: ArtifactKind): string {
  if (kind === "AppImage") return "app";
  if (kind === "tar.gz") return ".tar";
  return `.${kind}`;
}

function InstallArt({ os, kind, step }: { os: Os; kind: ArtifactKind; step: number }) {
  if (kind === "zip" || kind === "tar.gz") return <ArchiveArt kind={kind} os={os} step={step} />;
  if (os === "macos") return <MacArt step={step} />;
  if (os === "windows") return <WindowsArt step={step} />;
  return <LinuxArt kind={kind} step={step} />;
}

function FileGlyph({ label }: { label: string }) {
  return (
    <g>
      <rect
        x="20"
        y="14"
        width="40"
        height="50"
        rx="8"
        fill="var(--raised)"
        stroke="var(--line-strong)"
      />
      <rect x="28" y="24" width="24" height="4" rx="2" fill="var(--ink-2)" />
      <rect x="28" y="32" width="16" height="4" rx="2" fill="var(--line-strong)" />
      <text
        x="40"
        y="56"
        textAnchor="middle"
        fill="var(--ink)"
        fontSize="9"
        fontFamily="ui-sans-serif, system-ui, sans-serif"
      >
        {label}
      </text>
    </g>
  );
}

function FolderGlyph({ x, y, label }: { x: number; y: number; label: string }) {
  return (
    <g transform={`translate(${x} ${y})`}>
      <path
        d="M8 18h64a8 8 0 0 1 8 8v36a8 8 0 0 1-8 8H8a8 8 0 0 1-8-8V26a8 8 0 0 1 8-8h18l8-8h14"
        fill="var(--raised)"
        stroke="var(--line-strong)"
      />
      <text
        x="40"
        y="50"
        textAnchor="middle"
        fill="var(--ink-2)"
        fontSize="9"
        fontFamily="ui-sans-serif, system-ui, sans-serif"
      >
        {label}
      </text>
    </g>
  );
}

function AppGlyph() {
  return (
    <g>
      <rect
        x="28"
        y="22"
        width="40"
        height="40"
        rx="12"
        fill="var(--raised)"
        stroke="var(--line-strong)"
      />
      <rect x="36" y="30" width="14" height="14" rx="4" fill="var(--ink)" />
      <rect x="46" y="40" width="14" height="14" rx="4" fill="#22a867" />
    </g>
  );
}

function AppWindow() {
  return (
    <svg width="160" height="120" viewBox="0 0 160 120" aria-hidden="true">
      <rect
        x="28"
        y="18"
        width="104"
        height="78"
        rx="12"
        fill="var(--raised)"
        stroke="var(--line-strong)"
      />
      <circle cx="40" cy="30" r="3" fill="var(--line-strong)" />
      <circle cx="50" cy="30" r="3" fill="var(--line-strong)" />
      <g transform="translate(36 38) scale(1.6)">
        <rect x="2" y="2" width="19" height="19" rx="5" fill="var(--ink)" />
        <rect x="11" y="11" width="19" height="19" rx="5" fill="#22a867" />
      </g>
    </svg>
  );
}

function InstallerWindow() {
  return (
    <svg width="160" height="120" viewBox="0 0 160 120" aria-hidden="true">
      <rect
        x="34"
        y="22"
        width="92"
        height="72"
        rx="10"
        fill="var(--raised)"
        stroke="var(--line-strong)"
      />
      <rect x="46" y="36" width="68" height="8" rx="4" fill="var(--ink)" />
      <rect x="46" y="50" width="48" height="6" rx="3" fill="var(--line-strong)" />
      <rect x="70" y="72" width="36" height="12" rx="6" fill="#22a867" />
    </svg>
  );
}

function DownloadsFile({ label }: { label: string }) {
  return (
    <svg width="160" height="120" viewBox="0 0 160 120" aria-hidden="true">
      <FolderGlyph x={36} y={22} label="Downloads" />
      <g transform="translate(86 18)">
        <FileGlyph label={label} />
      </g>
    </svg>
  );
}

function TerminalArt({ line, file }: { line: string; file?: string }) {
  return (
    <svg width="160" height="120" viewBox="0 0 160 120" aria-hidden="true">
      <rect x="28" y="22" width="104" height="72" rx="10" fill="var(--code)" />
      <text
        x="44"
        y={file ? 54 : 64}
        fill="var(--code-ink)"
        fontSize="12"
        fontFamily="ui-monospace, monospace"
      >
        {line}
      </text>
      {file && (
        <text x="44" y="72" fill="#3fcb84" fontSize="12" fontFamily="ui-monospace, monospace">
          {file}
        </text>
      )}
    </svg>
  );
}

function MacArt({ step }: { step: number }) {
  if (step === 0) return <DownloadsFile label=".dmg" />;
  if (step === 1) {
    return (
      <svg width="160" height="120" viewBox="0 0 160 120" aria-hidden="true">
        <AppGlyph />
        <path
          d="M78 42h18"
          fill="none"
          stroke="var(--ink-2)"
          strokeWidth="2"
          strokeLinecap="round"
        />
        <path
          d="M90 36l8 6-8 6"
          fill="none"
          stroke="var(--ink-2)"
          strokeWidth="2"
          strokeLinecap="round"
          strokeLinejoin="round"
        />
        <FolderGlyph x={78} y={28} label="Applications" />
      </svg>
    );
  }
  return <AppWindow />;
}

function WindowsArt({ step }: { step: number }) {
  if (step === 0) return <DownloadsFile label=".msi" />;
  if (step === 1) return <InstallerWindow />;
  return (
    <svg width="160" height="120" viewBox="0 0 160 120" aria-hidden="true">
      <rect
        x="28"
        y="18"
        width="104"
        height="78"
        rx="12"
        fill="var(--raised)"
        stroke="var(--line-strong)"
      />
      <g transform="translate(60 38)">
        <rect width="16" height="16" rx="2" fill="var(--ink)" />
        <rect x="20" width="16" height="16" rx="2" fill="#22a867" />
        <rect y="20" width="16" height="16" rx="2" fill="#22a867" />
        <rect x="20" y="20" width="16" height="16" rx="2" fill="var(--ink)" />
      </g>
    </svg>
  );
}

function LinuxArt({ kind, step }: { kind: ArtifactKind; step: number }) {
  if (step === 0) return <DownloadsFile label={fileLabel(kind)} />;
  if (step === 1) {
    if (kind === "deb" || kind === "rpm") return <InstallerWindow />;
    return <TerminalArt line="$ chmod +x" file="convt.AppImage" />;
  }
  return <AppWindow />;
}

function ArchiveArt({ kind, os, step }: { kind: ArtifactKind; os: Os; step: number }) {
  if (step === 0) return <DownloadsFile label={fileLabel(kind)} />;
  if (step === 1) {
    if (os === "macos") {
      return (
        <svg width="160" height="120" viewBox="0 0 160 120" aria-hidden="true">
          <AppGlyph />
          <path
            d="M78 42h18"
            fill="none"
            stroke="var(--ink-2)"
            strokeWidth="2"
            strokeLinecap="round"
          />
          <path
            d="M90 36l8 6-8 6"
            fill="none"
            stroke="var(--ink-2)"
            strokeWidth="2"
            strokeLinecap="round"
            strokeLinejoin="round"
          />
          <FolderGlyph x={78} y={28} label="Applications" />
        </svg>
      );
    }
    if (os === "windows") {
      return (
        <svg width="160" height="120" viewBox="0 0 160 120" aria-hidden="true">
          <FolderGlyph x={40} y={22} label="convt" />
        </svg>
      );
    }
    return <TerminalArt line="$ tar xf" file="convt.tar.gz" />;
  }
  if (os === "windows") {
    return (
      <svg width="160" height="120" viewBox="0 0 160 120" aria-hidden="true">
        <FolderGlyph x={18} y={22} label="convt" />
        <g transform="translate(88 18)">
          <FileGlyph label=".exe" />
        </g>
      </svg>
    );
  }
  if (os === "linux" || kind === "tar.gz") {
    return <TerminalArt line="$ ./convt-app" />;
  }
  return <AppWindow />;
}
