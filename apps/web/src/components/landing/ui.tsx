import type { ComponentProps, ReactNode } from "react";

export const focusRing =
  "outline-none focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-green";

export function cx(...classes: Array<string | false | null | undefined>) {
  return classes.filter(Boolean).join(" ");
}

/** 1200px content column with side gutters on smaller screens. */
export function Container({ className, ...props }: ComponentProps<"div">) {
  return <div className={cx("mx-auto w-full max-w-[1240px] px-5", className)} {...props} />;
}

const buttonVariants = {
  // White on green is about 3:1. Matches the design until Leo picks a fix.
  primary: "bg-land-green text-white shadow-land-primary hover:brightness-110",
  secondary: "bg-sunken text-ink shadow-land-secondary hover:bg-hover",
};

export function ButtonLink({
  variant,
  className,
  children,
  ...props
}: ComponentProps<"a"> & { variant: keyof typeof buttonVariants; children: ReactNode }) {
  return (
    <a
      className={cx(
        "inline-flex shrink-0 items-center justify-center gap-2 font-medium whitespace-nowrap transition-[filter,background-color] duration-150",
        buttonVariants[variant],
        focusRing,
        className,
      )}
      {...props}
    >
      {children}
    </a>
  );
}

/** Stands in for a button whose destination is not live yet. Same box, not clickable. */
export function ComingSoon({
  className,
  children = "Coming soon",
}: {
  className?: string;
  children?: ReactNode;
}) {
  return (
    <span
      className={cx(
        "inline-flex shrink-0 cursor-default items-center justify-center gap-2 font-medium whitespace-nowrap bg-sunken text-ink-2 shadow-land-secondary select-none",
        className,
      )}
    >
      {children}
    </span>
  );
}

export function DownloadIcon() {
  return (
    <svg width="14" height="14" viewBox="0 0 14 14" aria-hidden="true" className="shrink-0">
      <path
        d="M7 1.5v8M3.5 6.5L7 10l3.5-3.5M2 12.5h10"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.6"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

export function ConvertIcon() {
  return (
    <svg width="12" height="12" viewBox="0 0 12 12" aria-hidden="true" className="shrink-0">
      <path
        d="M2 4h7M7 2l2 2-2 2M10 8H3M5 6 3 8l2 2"
        fill="none"
        stroke="#ffffff"
        strokeWidth="1.4"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}
