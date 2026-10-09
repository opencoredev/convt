import { Copy01Icon, Tick02Icon } from "@hugeicons/core-free-icons";
import { useEffect, useRef, useState } from "react";

import { Icon } from "#/components/icon";

import { cx, focusRing } from "./ui";

/**
 * Shell commands, one per line with a `$` prompt, in a dark terminal panel in both
 * themes. Lines never wrap: a long one scrolls sideways, so a single command can't look
 * like several. Copy copies the commands without the prompts, one per line. `quiet` is
 * a light panel that follows the theme, for a side path such as sign-in's.
 */
export function CommandBlock({
  label,
  commands,
  quiet = false,
  className,
}: {
  label: string;
  commands: [string, ...string[]];
  quiet?: boolean;
  className?: string;
}) {
  const [copied, setCopied] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout>>(undefined);
  useEffect(() => () => clearTimeout(timer.current), []);

  async function copy() {
    try {
      await navigator.clipboard.writeText(commands.join("\n"));
      setCopied(true);
      clearTimeout(timer.current);
      timer.current = setTimeout(() => setCopied(false), 1600);
    } catch {
      setCopied(false);
    }
  }

  return (
    <div
      className={cx(
        "flex min-w-0 flex-col overflow-hidden rounded-xl text-left",
        quiet
          ? "bg-sunken/80 text-ink-2 shadow-[inset_0_0_0_1px_var(--line)] backdrop-blur-sm"
          : "dark bg-code text-code-ink shadow-[0_0_0_1px_rgb(0_0_0/6%),0_8px_24px_rgb(10_30_20/10%)] dark:shadow-[inset_0_0_0_1px_#232726]",
        className,
      )}
    >
      <div
        className={cx(
          "flex items-center justify-between gap-3 border-b pr-1.5 pl-4",
          quiet ? "h-8 border-line" : "h-9 border-[#ffffff12]",
        )}
      >
        <span
          className={cx(
            "font-mono text-[11px]/4 tracking-[0.04em] uppercase",
            quiet ? "text-ink-2" : "text-[#8a908c]",
          )}
        >
          {label}
        </span>
        <button
          type="button"
          onClick={copy}
          className={cx(
            "flex h-7 cursor-pointer items-center gap-1.5 rounded-md px-2 text-[12px]/4 font-medium transition-colors duration-150",
            quiet
              ? "text-ink-2 hover:bg-hover hover:text-ink"
              : "text-[#c4c9c6] hover:bg-[#ffffff12] hover:text-white",
            focusRing,
          )}
        >
          <Icon icon={copied ? Tick02Icon : Copy01Icon} size={14} strokeWidth={1.8} />
          <span aria-live="polite">{copied ? "Copied" : "Copy"}</span>
        </button>
      </div>
      <pre
        className={cx(
          // The fade at the right edge hints that a long line scrolls.
          "overflow-x-auto px-4 font-mono whitespace-pre [mask-image:linear-gradient(to_right,black_calc(100%-20px),transparent)] [scrollbar-width:thin]",
          quiet ? "py-2.5 text-[11.5px]/5" : "py-3 text-[12.5px]/[22px]",
        )}
      >
        <code>
          {commands.map((command) => (
            <span key={command} className="block">
              <span aria-hidden="true" className="text-ink-3 select-none">
                ${" "}
              </span>
              {command}
            </span>
          ))}
        </code>
      </pre>
    </div>
  );
}

/** The Homebrew install, as the release workflow publishes the cask. */
export const homebrewCommands: [string, string] = [
  "brew tap opencoredev/convt https://github.com/opencoredev/convt",
  "brew install --cask convt",
];
