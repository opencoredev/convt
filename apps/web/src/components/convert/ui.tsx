import type { ReactNode } from "react";

import { cx, focusRing } from "#/components/landing/ui";

/** Renders `inline code` in the conversion copy. The copy is ours, never user input. */
export function Prose({ text }: { text: string }) {
  return (
    <>
      {text.split(/(`[^`]+`)/).map((part, i) =>
        part.startsWith("`") && part.endsWith("`") ? (
          <code
            key={i}
            className="rounded-[5px] bg-sunken px-1.5 py-px font-mono text-[0.86em] text-land-mono"
          >
            {part.slice(1, -1)}
          </code>
        ) : (
          part
        ),
      )}
    </>
  );
}

export function SectionTitle({ id, children }: { id?: string; children: ReactNode }) {
  return (
    <h2
      id={id}
      className="text-[24px]/[30px] font-medium tracking-[-0.025em] text-ink md:text-[28px]/[34px]"
    >
      {children}
    </h2>
  );
}

export function TextLink({ className, ...props }: React.ComponentProps<"a">) {
  return (
    <a
      className={cx(
        "rounded-sm text-ink underline decoration-line-strong underline-offset-4 transition-colors hover:decoration-land-accent",
        focusRing,
        className,
      )}
      {...props}
    />
  );
}

/** A file name chip, as in the engine cards: name.ext in mono. */
export function FileChip({ name, ext, done }: { name: string; ext: string; done?: boolean }) {
  return (
    <span
      className={cx(
        "rounded-lg bg-raised px-3 py-2 font-mono text-[13px]/[16px] shadow-land-card",
        done ? "text-green" : "text-land-mono",
      )}
    >
      {name}.{ext}
    </span>
  );
}
