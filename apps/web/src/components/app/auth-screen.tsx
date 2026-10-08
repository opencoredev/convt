import { Link } from "@tanstack/react-router";
import type { CSSProperties, ReactNode } from "react";

import { Mark } from "#/components/logo";
import { links } from "#/lib/config";

import { DitherGlow } from "./dither-glow";
import { cx, focusRing } from "./ui";

/*
 * Sign-in, check-email, verify and the desktop sign-in page share this screen.
 *
 * Direction. Reference: Delphi's sign-in (one centered column, a lot of white, pill
 * buttons) and the convt mark. Palette: the page tokens plus the mark's greens, which
 * rise from the bottom as a dithered glow (--glow). Type: Inter, the subtitle in
 * ink-2 at 17px. Layout: logo, one line, the actions; nothing else competes. Signature:
 * the pixel-grain glow, which echoes the dither panels on the landing page. Avoiding:
 * split screens, cards on cards, and a second accent color.
 */
export function AuthScreen({ children }: { children: ReactNode }) {
  return (
    <div className="relative isolate flex min-h-dvh flex-col overflow-hidden bg-page text-ink">
      <DitherGlow className="absolute inset-x-0 bottom-0 -z-10 h-[58vh] w-full min-h-[320px]" />
      <main className="flex flex-1 flex-col items-center justify-center px-6 pt-16 pb-24 sm:pb-32">
        <div className="flex w-full max-w-[380px] flex-col items-center">
          <Rise index={0}>
            <Link
              to="/"
              aria-label="convt home"
              className={cx(
                "flex items-center gap-2.5 rounded-md text-[30px]/9 font-semibold tracking-[-0.035em]",
                focusRing,
              )}
            >
              <Mark size={30} />
              convt
            </Link>
          </Rise>
          {children}
        </div>
      </main>
      <nav
        aria-label="Legal"
        className="flex justify-center gap-5 pb-6 text-xs/4 text-ink-2 [text-shadow:0_0_8px_var(--page)]"
      >
        <a href={links.help} className={cx("rounded-sm hover:text-ink", focusRing)}>
          Help
        </a>
        <a href={links.terms} className={cx("rounded-sm hover:text-ink", focusRing)}>
          Terms
        </a>
        <a href={links.privacy} className={cx("rounded-sm hover:text-ink", focusRing)}>
          Privacy
        </a>
      </nav>
    </div>
  );
}

/** Children of the screen arrive in order, 60 ms apart (reduced motion: at once). */
export function Rise({
  index,
  className,
  children,
}: {
  index: number;
  className?: string;
  children: ReactNode;
}) {
  return (
    <div
      className={cx("auth-rise w-full", className)}
      style={{ "--d": `${80 + index * 60}ms` } as CSSProperties}
    >
      {children}
    </div>
  );
}

/** The one line under the logo, Delphi-style: the page's real heading. */
export function AuthTitle({ children, sub }: { children: ReactNode; sub?: ReactNode }) {
  return (
    <div className="mt-6 flex flex-col items-center gap-2 text-center">
      <h1 className="text-[17px]/6 font-medium tracking-[-0.01em] text-balance text-ink-2">
        {children}
      </h1>
      {sub ? <p className="text-sm/[22px] text-pretty break-words text-ink-2">{sub}</p> : null}
    </div>
  );
}

const pill =
  "inline-flex h-11 w-full cursor-pointer items-center justify-center gap-2.5 rounded-full text-[15px]/5 font-medium transition-[background-color,box-shadow,scale] duration-150 ease-out active:scale-[0.98] disabled:cursor-wait disabled:opacity-70 motion-reduce:active:scale-100";

/** The dark pill: Continue with Google, Approve. Inverts in dark mode. */
export const darkPill = cx(
  pill,
  "bg-ink text-page shadow-[0_1px_2px_rgb(0_0_0/12%),0_4px_14px_rgb(0_0_0/10%)] hover:bg-ink/88 dark:shadow-none",
  focusRing,
);

/** The light pill: Continue with Email, Cancel. */
export const lightPill = cx(
  pill,
  "bg-pill text-ink shadow-[inset_0_0_0_1px_var(--pill-line)] hover:bg-pill-hover",
  focusRing,
);

export const authInput =
  "h-11 w-full rounded-xl bg-raised/90 px-4 text-[15px]/5 text-ink shadow-input outline-none backdrop-blur-sm transition-shadow duration-150 placeholder:text-ink-3 focus-visible:shadow-[0_0_0_1px_var(--green),0_0_0_4px_color-mix(in_oklab,var(--green)_18%,transparent)]";

/** "or", between Google and the email step. */
export function OrDivider() {
  return (
    <div className="flex w-full items-center gap-4" role="separator" aria-label="or">
      <span className="h-px flex-1 bg-line" />
      <span aria-hidden="true" className="text-[13px]/4 text-ink-3">
        or
      </span>
      <span className="h-px flex-1 bg-line" />
    </div>
  );
}

/** Small, centered secondary link-style button: "Back", "Use a different email". */
export function QuietButton({ className, ...props }: React.ComponentProps<"button">) {
  return (
    <button
      type="button"
      {...props}
      className={cx(
        "cursor-pointer rounded-sm text-[13px]/4 text-ink-2 underline-offset-[3px] hover:text-ink hover:underline",
        focusRing,
        className,
      )}
    />
  );
}

/** The Google "G" in one color, like the label beside it. */
export function GoogleMark() {
  return (
    <svg width="17" height="17" viewBox="0 0 24 24" aria-hidden="true" className="shrink-0">
      <path
        fill="currentColor"
        d="M21.6 12.23c0-.71-.06-1.4-.18-2.05H12v3.88h5.38a4.6 4.6 0 0 1-2 3.02v2.5h3.24c1.9-1.75 2.98-4.32 2.98-7.35ZM12 22c2.7 0 4.97-.9 6.62-2.42l-3.24-2.5c-.9.6-2.04.95-3.38.95-2.6 0-4.8-1.75-5.59-4.11H3.07v2.59A10 10 0 0 0 12 22Zm-5.59-8.08A6 6 0 0 1 6.1 12c0-.67.11-1.31.31-1.92V7.49H3.07A10 10 0 0 0 2 12c0 1.61.39 3.14 1.07 4.51l3.34-2.59ZM12 5.97c1.47 0 2.79.5 3.83 1.5l2.87-2.87A9.6 9.6 0 0 0 12 2a10 10 0 0 0-8.93 5.49l3.34 2.59C7.2 7.72 9.4 5.97 12 5.97Z"
      />
    </svg>
  );
}

export function GitHubMark() {
  return (
    <svg width="14" height="14" viewBox="0 0 24 24" aria-hidden="true" className="shrink-0">
      <path
        fill="currentColor"
        d="M12 2a10 10 0 0 0-3.16 19.49c.5.09.68-.22.68-.48v-1.7c-2.78.6-3.37-1.34-3.37-1.34-.45-1.16-1.11-1.47-1.11-1.47-.91-.62.07-.6.07-.6 1 .07 1.53 1.03 1.53 1.03.9 1.53 2.34 1.09 2.91.83.09-.65.35-1.09.63-1.34-2.22-.25-4.55-1.11-4.55-4.94 0-1.09.39-1.98 1.03-2.68-.1-.25-.45-1.27.1-2.64 0 0 .84-.27 2.75 1.02a9.6 9.6 0 0 1 5 0c1.91-1.3 2.75-1.02 2.75-1.02.55 1.37.2 2.39.1 2.64.64.7 1.03 1.59 1.03 2.68 0 3.84-2.34 4.68-4.57 4.93.36.31.68.92.68 1.85v2.75c0 .27.18.58.69.48A10 10 0 0 0 12 2Z"
      />
    </svg>
  );
}
