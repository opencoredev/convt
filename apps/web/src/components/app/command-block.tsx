import { Copy01Icon, Tick02Icon } from "@hugeicons/core-free-icons";
import { useEffect, useRef, useState } from "react";

import { Icon } from "#/components/icon";

import { cx, focusRing } from "./ui";

/**
 * Shell commands, one per line with a `$` prompt, on a panel that follows the theme.
 * Lines never wrap: a long one scrolls sideways, so a single command can't look like
 * several. Copy copies the commands without the prompts, one per line. `label` names
 * the commands for screen readers ("Copy Homebrew commands"); the page shows its own
 * heading. `quiet` is the smaller size, for a side path such as sign-in's.
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
        "flex min-w-0 items-start rounded-xl bg-sunken text-left text-ink shadow-[inset_0_0_0_1px_var(--line)]",
        className,
      )}
    >
      <pre
        className={cx(
          // The fade at the right edge hints that a long line scrolls.
          "min-w-0 flex-1 overflow-x-auto font-mono whitespace-pre [mask-image:linear-gradient(to_right,black_calc(100%-24px),transparent)] [scrollbar-width:thin]",
          quiet ? "py-2.5 pl-3.5 text-[11.5px]/5" : "py-3.5 pl-4 text-[13px]/6",
        )}
      >
        <code>
          {commands.map((command) => (
            <span key={command} className="block pr-6">
              <span aria-hidden="true" className="text-ink-3 select-none">
                ${" "}
              </span>
              {command}
            </span>
          ))}
        </code>
      </pre>
      <button
        type="button"
        onClick={copy}
        aria-label={`Copy ${label} commands`}
        className={cx(
          "flex shrink-0 cursor-pointer items-center gap-1.5 rounded-lg font-medium text-ink-2 transition-colors duration-150 hover:bg-hover hover:text-ink",
          quiet ? "m-1.5 h-7 px-2 text-[12px]/4" : "m-2 h-8 px-2 text-[13px]/4 sm:px-2.5",
          focusRing,
        )}
      >
        <Icon
          icon={copied ? Tick02Icon : Copy01Icon}
          size={quiet ? 14 : 15}
          strokeWidth={1.8}
          className={copied ? "text-green" : undefined}
        />
        <span aria-hidden="true" className={quiet ? undefined : "max-sm:hidden"}>
          {copied ? "Copied" : "Copy"}
        </span>
        <span className="sr-only" aria-live="polite">
          {copied ? "Copied" : ""}
        </span>
      </button>
    </div>
  );
}

/** The Homebrew install, as the release workflow publishes the cask. */
export const homebrewCommands: [string, string] = [
  "brew tap opencoredev/convt https://github.com/opencoredev/convt",
  "brew install --cask convt",
];
