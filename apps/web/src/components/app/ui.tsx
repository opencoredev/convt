import type { ComponentProps } from "react";

// Shared pieces for the dashboard and account pages, sized from the Paper file.

function cx(...parts: Array<string | false | null | undefined>) {
  return parts.filter(Boolean).join(" ");
}

export { cx };

const focus =
  "outline-none focus-visible:ring-2 focus-visible:ring-green focus-visible:ring-offset-2 focus-visible:ring-offset-page";

export const focusRing = focus;

/** Green gradient button. See `.btn-primary` in styles.css for the contrast note. */
export function PrimaryButton({ className, ...props }: ComponentProps<"button">) {
  return (
    <button
      type="button"
      {...props}
      className={cx(
        "btn-primary inline-flex cursor-pointer items-center justify-center rounded-lg px-3.5 py-2 text-[13px]/4 font-medium disabled:cursor-not-allowed disabled:opacity-60",
        focus,
        className,
      )}
    />
  );
}

export function PrimaryLink({ className, ...props }: ComponentProps<"a">) {
  return (
    <a
      {...props}
      className={cx(
        "btn-primary inline-flex items-center justify-center rounded-lg px-3.5 py-2 text-sm/4.5 font-medium",
        focus,
        className,
      )}
    />
  );
}

const secondary =
  "inline-flex cursor-pointer items-center justify-center rounded-lg bg-raised px-3 py-[7px] text-[13px]/4 font-medium text-ink shadow-button hover:bg-hover dark:bg-sunken";

export function SecondaryButton({ className, ...props }: ComponentProps<"button">) {
  return <button type="button" {...props} className={cx(secondary, focus, className)} />;
}

export function SecondaryLink({ className, ...props }: ComponentProps<"a">) {
  return <a {...props} className={cx(secondary, focus, className)} />;
}

/** Plain text action: green by default, `tone="muted"` for gray, `tone="danger"` for red. */
export function TextButton({
  tone = "green",
  className,
  ...props
}: ComponentProps<"button"> & { tone?: "green" | "muted" | "ink" | "danger" }) {
  return (
    <button
      type="button"
      {...props}
      className={cx(
        "cursor-pointer rounded-sm text-[13px]/4 hover:underline hover:underline-offset-2",
        tone === "green" && "font-medium text-green",
        tone === "muted" && "text-ink-2",
        tone === "ink" && "text-ink",
        tone === "danger" && "text-error",
        focus,
        className,
      )}
    />
  );
}

export function PageTitle({ children }: { children: React.ReactNode }) {
  return <h1 className="text-[32px]/10 font-semibold tracking-[-0.025em] text-ink">{children}</h1>;
}

export function SectionTitle({ children, id }: { children: React.ReactNode; id?: string }) {
  return (
    <h2 id={id} className="text-[15px]/4.5 font-semibold text-ink">
      {children}
    </h2>
  );
}

/** Bordered surface. Light cards sit on the page; dark cards use the raised color. */
export function Card({ className, ...props }: ComponentProps<"div">) {
  return <div {...props} className={cx("rounded-xl bg-raised ring-1 ring-line", className)} />;
}

export function Badge({
  tone = "green",
  size = "md",
  children,
}: {
  tone?: "green" | "neutral";
  size?: "sm" | "md";
  children: React.ReactNode;
}) {
  return (
    <span
      className={cx(
        "inline-block rounded px-1.5 py-0.5 font-mono",
        size === "md" ? "text-xs/4" : "text-[11px]/3.5",
        tone === "green" ? "bg-green-tint text-green" : "bg-hover text-ink-2",
      )}
    >
      {children}
    </span>
  );
}

/** Classes for the bordered tables (invoices, keys, sign-in methods, sessions). */
export const table = {
  /** Scroll wrapper so wide tables stay usable on a phone. */
  wrap: "overflow-x-auto rounded-xl bg-raised ring-1 ring-line",
  table: "w-full min-w-[720px] border-collapse text-left text-[13px]/4",
  headRow: "border-b border-line bg-sunken text-xs/4 text-ink-2",
  th: "px-0 py-2.5 font-normal first:pl-6 last:pr-6",
  row: "border-b border-line last:border-b-0",
  td: "py-3.5 first:pl-6 last:pr-6",
};

export function ExternalIcon() {
  return (
    <svg width="12" height="12" viewBox="0 0 12 12" aria-hidden="true" className="shrink-0">
      <path
        d="M4 2.5 H9.5 V8 M9.5 2.5 L2.5 9.5"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.4"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

export function ChevronIcon() {
  return (
    <svg width="12" height="12" viewBox="0 0 12 12" aria-hidden="true" className="shrink-0">
      <path
        d="M4.5 2.5 L8 6 L4.5 9.5"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.4"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

/**
 * Small inline label for screens that are previews of flows the backend does not
 * support yet. PLACEHOLDER: remove the uses once auth and accounts are live.
 */
export function PreviewNote({
  children,
  className,
}: {
  children: React.ReactNode;
  className?: string;
}) {
  return (
    <p
      className={cx(
        "inline-flex items-center gap-2 rounded-md bg-sunken px-2 py-1 text-xs/4 text-ink-2 ring-1 ring-line",
        className,
      )}
    >
      <span aria-hidden="true" className="size-1.5 shrink-0 rounded-full bg-ink-3" />
      {children}
    </p>
  );
}
