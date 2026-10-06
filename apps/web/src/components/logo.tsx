import { useId } from "react";

/*
 * The convt mark: the source file (ink) and the converted file (green) overlap, and the
 * overlap is where the conversion happens. Colors come from the --mark-* tokens in
 * styles.css so it follows the page theme. public/favicon.svg draws the same geometry.
 */
export function Mark({ size = 20, className }: { size?: number; className?: string }) {
  const id = useId();
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 32 32"
      aria-hidden="true"
      className={className ? `shrink-0 ${className}` : "shrink-0"}
    >
      <defs>
        <clipPath id={`${id}-source`}>
          <rect x="2" y="2" width="19" height="19" rx="5" />
        </clipPath>
        <linearGradient id={`${id}-green`} x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stopColor="var(--mark-green-top)" />
          <stop offset="1" stopColor="var(--mark-green-bottom)" />
        </linearGradient>
      </defs>
      <rect x="2" y="2" width="19" height="19" rx="5" fill="var(--ink)" />
      <rect x="11" y="11" width="19" height="19" rx="5" fill={`url(#${id}-green)`} />
      <rect
        x="11"
        y="11"
        width="19"
        height="19"
        rx="5"
        fill="var(--mark-overlap)"
        clipPath={`url(#${id}-source)`}
      />
    </svg>
  );
}
