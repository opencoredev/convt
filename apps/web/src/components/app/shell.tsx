import { Link, useRouterState } from "@tanstack/react-router";

import { Nav } from "#/components/landing/nav";
import type { Account } from "#/lib/types";

import { cx } from "./ui";

const tabs = [
  { to: "/dashboard", label: "Overview", exact: true },
  { to: "/dashboard/licenses", label: "Licenses", exact: false },
  { to: "/dashboard/billing", label: "Billing", exact: false },
  { to: "/dashboard/cloud", label: "Cloud", exact: false },
  { to: "/dashboard/api", label: "API", exact: false },
  { to: "/account", label: "Settings", exact: false },
] as const;

/** Width of the page column: 1080px of content plus the side padding. */
export const column = "mx-auto w-full max-w-[1120px] px-5";

export function AppShell({ account, children }: { account: Account; children: React.ReactNode }) {
  const path = useRouterState({ select: (state) => state.location.pathname });
  return (
    <div className="min-h-screen bg-page text-ink">
      <a
        href="#main"
        className="sr-only focus:not-sr-only focus:fixed focus:top-3 focus:left-3 focus:z-50 focus:rounded-md focus:bg-raised focus:px-3 focus:py-2 focus:text-sm focus:ring-1 focus:ring-line"
      >
        Skip to content
      </a>
      {/* The tabs sit under the site nav, outside its <header>, behind one rule. */}
      <div className="border-b border-line">
        <Nav account={account} path={path} className="max-w-[1120px]!" />
        <nav aria-label="Account" className={cx(column, "overflow-x-auto")}>
          <ul className="flex gap-6">
            {tabs.map((tab) => (
              <li key={tab.to} className="flex shrink-0">
                <Link
                  to={tab.to}
                  activeOptions={{ exact: tab.exact }}
                  className={cx(
                    "flex pt-2.5 pb-3 text-sm/4.5 whitespace-nowrap text-ink-2 hover:text-ink",
                    "outline-none focus-visible:text-ink focus-visible:underline focus-visible:underline-offset-4",
                  )}
                  activeProps={{
                    className: "font-medium text-ink! shadow-[inset_0_-2px_0_var(--ink)]",
                    "aria-current": "page",
                  }}
                >
                  {tab.label}
                </Link>
              </li>
            ))}
          </ul>
        </nav>
      </div>
      <main id="main" className={cx(column, "pt-10 pb-18")}>
        {children}
      </main>
    </div>
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
