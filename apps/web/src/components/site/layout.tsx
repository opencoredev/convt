import type { ReactNode } from "react";

import { cx, focusRing, PrimaryLink, SecondaryLink } from "#/components/app/ui";
import { Mark } from "#/components/logo";
import { routes } from "#/lib/site";

import { footerColumns, headerLinks } from "./links";

/** Width of the public pages' column: 1080px of content plus the side padding. */
export const siteColumn = "mx-auto w-full max-w-[1120px] px-5";

const navLink = cx(
  "rounded-sm text-sm/4.5 text-ink-nav transition-colors hover:text-ink",
  focusRing,
);

/**
 * Frame for the public pages other than the landing page: header, main column and
 * footer. Follows the visitor's light or dark setting like the account pages.
 */
export function SitePage({ children }: { children: ReactNode }) {
  return (
    <div className="flex min-h-screen flex-col overflow-x-clip bg-page text-ink">
      <a
        href="#main"
        className="sr-only focus:not-sr-only focus:fixed focus:top-3 focus:left-3 focus:z-50 focus:rounded-md focus:bg-raised focus:px-3 focus:py-2 focus:text-sm focus:ring-1 focus:ring-line"
      >
        Skip to content
      </a>
      <SiteHeader />
      <main id="main" className="flex-1">
        {children}
      </main>
      <SiteFooter />
    </div>
  );
}

function SiteHeader() {
  return (
    <header className="relative border-b border-line">
      <div className={cx(siteColumn, "flex items-center justify-between gap-4 py-4")}>
        <a
          href="/"
          className={cx(
            "flex items-center gap-2 rounded-sm text-[17px]/5.5 font-semibold tracking-[-0.02em]",
            focusRing,
          )}
        >
          <Mark />
          convt
        </a>
        <nav aria-label="Main" className="hidden md:block">
          <ul className="flex gap-7">
            {headerLinks.map((link) => (
              <li key={link.href}>
                <a href={link.href} className={navLink}>
                  {link.label}
                </a>
              </li>
            ))}
          </ul>
        </nav>
        <div className="flex items-center gap-2">
          <span className="hidden sm:contents">
            <SecondaryLink href={routes.signIn}>Sign in</SecondaryLink>
          </span>
          <PrimaryLink href={routes.download} className="py-[7px] text-[13px]/4">
            Download
          </PrimaryLink>
          <MobileMenu />
        </div>
      </div>
    </header>
  );
}

// A <details> disclosure, so the menu works before hydration and without script.
function MobileMenu() {
  return (
    <details className="group md:hidden">
      <summary
        className={cx(
          "flex size-8 cursor-pointer list-none items-center justify-center rounded-lg text-ink shadow-button hover:bg-hover [&::-webkit-details-marker]:hidden",
          focusRing,
        )}
      >
        <span className="sr-only">Menu</span>
        <svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true">
          <path
            className="group-open:hidden"
            d="M2.5 4.5h11M2.5 8h11M2.5 11.5h11"
            stroke="currentColor"
            strokeWidth="1.5"
            strokeLinecap="round"
          />
          <path
            className="hidden group-open:block"
            d="M4 4l8 8M12 4l-8 8"
            stroke="currentColor"
            strokeWidth="1.5"
            strokeLinecap="round"
          />
        </svg>
      </summary>
      <nav
        aria-label="Main"
        className="absolute inset-x-0 top-full z-40 border-b border-line bg-page shadow-note"
      >
        <ul className={cx(siteColumn, "flex flex-col py-2")}>
          {[...headerLinks, { label: "Sign in", href: routes.signIn }].map((link) => (
            <li key={link.href}>
              <a
                href={link.href}
                className={cx("flex rounded-sm py-3 text-[15px]/5 text-ink", focusRing)}
              >
                {link.label}
              </a>
            </li>
          ))}
        </ul>
      </nav>
    </details>
  );
}

function SiteFooter() {
  return (
    <footer className="border-t border-line">
      <div className={cx(siteColumn, "flex flex-col gap-12 pt-14 pb-10")}>
        <div className="flex flex-col gap-10 md:flex-row md:justify-between">
          <div className="flex max-w-[320px] flex-col gap-2.5">
            <p className="flex items-center gap-2 text-lg/5.5 font-semibold tracking-[-0.02em]">
              <Mark />
              convt
            </p>
            <p className="text-sm/[21px] text-ink-2">
              Local file conversion for macOS, Windows and Linux. Open source under AGPL-3.0.
            </p>
          </div>
          <div className="grid grid-cols-2 gap-x-16 gap-y-10 sm:flex sm:gap-x-20">
            {footerColumns.map((column) => (
              <nav key={column.title} aria-label={column.title} className="flex flex-col gap-3">
                <h2 className="font-mono text-xs/4 text-ink-2 uppercase">{column.title}</h2>
                <ul className="flex flex-col gap-3">
                  {column.links.map((link) => (
                    <li key={link.label}>
                      <a
                        href={link.href}
                        className={cx("rounded-sm text-sm/4.5 hover:text-ink-2", focusRing)}
                      >
                        {link.label}
                      </a>
                    </li>
                  ))}
                </ul>
              </nav>
            ))}
          </div>
        </div>
        <p className="border-t border-line pt-6 text-[13px]/4 text-ink-2">© 2026 convt</p>
      </div>
    </footer>
  );
}

/** Page heading block: optional mono eyebrow, title and lead paragraph. */
export function PageHeader({
  eyebrow,
  title,
  children,
}: {
  eyebrow?: string;
  title: string;
  children?: ReactNode;
}) {
  return (
    <div className="flex max-w-[680px] flex-col gap-3">
      {eyebrow && <p className="font-mono text-xs/4 text-ink-2 uppercase">{eyebrow}</p>}
      <h1 className="text-[34px]/10 font-semibold tracking-[-0.03em] text-balance md:text-[44px]/12">
        {title}
      </h1>
      {children && <div className="text-[17px]/[26px] text-ink-2">{children}</div>}
    </div>
  );
}

/** Inline link inside running text. */
export function TextLink({ className, ...props }: React.ComponentProps<"a">) {
  return (
    <a
      {...props}
      className={cx(
        "rounded-sm font-medium text-[#157f4a] dark:text-green underline-offset-2 hover:underline",
        focusRing,
        className,
      )}
    />
  );
}
