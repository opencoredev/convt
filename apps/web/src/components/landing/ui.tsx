import type { ComponentProps, ReactNode } from "react";

export const focusRing =
  "outline-none focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-solid focus-visible:outline-green";

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
        "inline-flex max-w-full cursor-default items-center justify-center gap-2 text-center font-medium text-balance bg-sunken text-ink-2 shadow-land-secondary select-none",
        className,
      )}
    >
      {children}
    </span>
  );
}

/**
 * The cat photo used across the landing page: a 6 KB AVIF with a 640px JPEG fallback.
 * Not lazy-loaded: it is one cached file, and lazy loading made the cards pop in
 * while scrolling.
 */
export function MisoPhoto({ className, ...props }: ComponentProps<"img">) {
  return (
    // display: contents keeps the img sized by its container, as before.
    <picture className="contents">
      <source srcSet="/landing/miso.avif" type="image/avif" />
      <img src="/landing/miso.jpg" decoding="async" className={className} {...props} />
    </picture>
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
