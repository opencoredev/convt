import { Link, useRouterState } from "@tanstack/react-router";
import { useLayoutEffect, useRef, useState } from "react";

import { AccountMenu } from "#/components/landing/account-menu";
import { Mark } from "#/components/logo";
import type { Account } from "#/lib/types";

import { cx, focusRing } from "./ui";

const tabs = [
  { to: "/dashboard", label: "Overview", exact: true },
  { to: "/dashboard/licenses", label: "Licenses", exact: false },
  { to: "/dashboard/billing", label: "Billing", exact: false },
  { to: "/dashboard/api", label: "API", exact: false },
  { to: "/account", label: "Settings", exact: false },
] as const;

/** Width of the page column: 1080px of content plus the side padding. */
export const column = "mx-auto w-full max-w-[1120px] px-5";

/** Moves the underline under the tab marked `aria-current`, or hides it. */
function placeBar(list: HTMLElement | null, bar: HTMLElement | null, animate: boolean) {
  if (!list || !bar) return;
  const link = list.querySelector<HTMLElement>("[aria-current=page]");
  bar.style.transition = animate ? "" : "none";
  bar.style.opacity = link ? "1" : "0";
  if (link) bar.style.transform = `translateX(${link.offsetLeft}px) scaleX(${link.offsetWidth})`;
}

export function AppShell({ account, children }: { account: Account; children: React.ReactNode }) {
  return (
    <div className="min-h-screen bg-page text-ink">
      <a
        href="#main"
        className="sr-only focus:not-sr-only focus:fixed focus:top-3 focus:left-3 focus:z-50 focus:rounded-md focus:bg-raised focus:px-3 focus:py-2 focus:text-sm focus:ring-1 focus:ring-line"
      >
        Skip to content
      </a>
      {/* One bar: the logo, the account tabs and the avatar. Below md the tabs wrap
          to their own row under the logo. */}
      <header className="border-b border-line">
        <div className={cx(column, "flex flex-wrap items-center justify-between md:flex-nowrap")}>
          <a
            href="/"
            className={cx(
              "flex h-14 items-center gap-2 rounded-sm text-[17px]/[22px] font-semibold tracking-[-0.02em] text-ink md:mr-8",
              focusRing,
            )}
          >
            <Mark />
            convt
          </a>
          <div className="flex h-14 items-center md:order-last md:ml-auto">
            <AccountMenu account={account} />
          </div>
          <Tabs />
        </div>
      </header>
      <main id="main" className={cx(column, "pt-10 pb-18")}>
        {children}
      </main>
    </div>
  );
}

/**
 * The account tabs. One underline slides to the active tab; until it has been
 * measured (server render, first paint) the active tab draws its own.
 */
function Tabs() {
  // The pending location, so the underline leaves as soon as a tab is clicked
  // rather than when its page's data arrives.
  const path = useRouterState({ select: (state) => state.location.pathname });
  const list = useRef<HTMLUListElement>(null);
  const bar = useRef<HTMLSpanElement>(null);
  const [measured, setMeasured] = useState(false);

  // Snap on first paint, slide between tabs.
  useLayoutEffect(() => {
    placeBar(list.current, bar.current, measured);
    setMeasured(true);
  }, [path, measured]);

  // Snap when the row changes size: a resize, or the font arriving.
  useLayoutEffect(() => {
    const ul = list.current;
    if (!ul) return;
    const observer = new ResizeObserver(() => placeBar(ul, bar.current, false));
    observer.observe(ul);
    return () => observer.disconnect();
  }, []);

  return (
    <nav
      aria-label="Account"
      className="-mx-5 w-[calc(100%+40px)] overflow-x-auto px-5 md:mx-0 md:w-auto md:overflow-visible md:px-0"
    >
      <ul ref={list} className="relative flex gap-6">
        {tabs.map((tab) => (
          <li key={tab.to} className="flex shrink-0">
            <Link
              to={tab.to}
              activeOptions={{ exact: tab.exact }}
              className={cx(
                "flex h-11 items-center text-sm/4.5 whitespace-nowrap text-ink-2 transition-colors duration-150 hover:text-ink md:h-14",
                "outline-none focus-visible:text-ink focus-visible:underline focus-visible:underline-offset-4",
              )}
              activeProps={{
                className: cx("text-ink!", !measured && "shadow-[inset_0_-2px_0_var(--ink)]"),
                "aria-current": "page",
              }}
            >
              {tab.label}
            </Link>
          </li>
        ))}
        <span
          ref={bar}
          aria-hidden="true"
          className="pointer-events-none absolute bottom-0 left-0 h-0.5 w-px origin-left bg-ink opacity-0 transition-transform duration-200 ease-(--ease-out) motion-reduce:transition-none"
        />
      </ul>
    </nav>
  );
}

/** The account's picture, or its first initial. 28px unless `className` sets a size. */
export function Avatar({ account, className }: { account: Account; className?: string }) {
  const style = { boxShadow: "var(--avatar-ring) 0 0 0 1px" };
  if (account.avatarUrl) {
    return (
      <img
        src={account.avatarUrl}
        alt=""
        width={28}
        height={28}
        style={style}
        className={cx("size-7 shrink-0 rounded-full object-cover", className)}
      />
    );
  }
  return (
    <span
      aria-hidden="true"
      style={style}
      className={cx(
        "flex size-7 shrink-0 items-center justify-center rounded-full bg-sunken text-xs font-medium text-ink-2",
        className,
      )}
    >
      {account.name.slice(0, 1).toUpperCase()}
    </span>
  );
}
