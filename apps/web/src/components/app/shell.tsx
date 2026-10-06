import { Link } from "@tanstack/react-router";

import { signOut } from "#/lib/auth-client";
import { links } from "#/lib/config";
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

export function AppShell({ account, children }: { account: Account; children: React.ReactNode }) {
  return (
    <div className="min-h-screen bg-page text-ink">
      <a
        href="#main"
        className="sr-only focus:not-sr-only focus:fixed focus:top-3 focus:left-3 focus:z-50 focus:rounded-md focus:bg-raised focus:px-3 focus:py-2 focus:text-sm focus:ring-1 focus:ring-line"
      >
        Skip to content
      </a>
      <header className="border-b border-line">
        <div className={cx(column, "flex items-center justify-between gap-4 pt-5 pb-3.5")}>
          <div className="flex min-w-0 items-center gap-3.5">
            <Link
              to="/"
              className={cx(
                "rounded-sm text-[17px]/5.5 font-semibold tracking-[-0.02em]",
                focusRing,
              )}
            >
              convt
            </Link>
            <span aria-hidden="true" className="text-sm/4.5 text-separator">
              /
            </span>
            <span className="truncate text-sm/4.5">{account.name}'s account</span>
          </div>
          <div className="flex shrink-0 items-center gap-4.5">
            <a
              href={links.docs}
              className={cx("rounded-sm text-sm/4.5 text-ink-nav hover:text-ink", focusRing)}
            >
              Docs
            </a>
            <a
              href={links.help}
              className={cx("rounded-sm text-sm/4.5 text-ink-nav hover:text-ink", focusRing)}
            >
              Help
            </a>
            {/* Placed after Help until the designer gives it a home. */}
            <button
              type="button"
              onClick={async () => {
                await signOut();
                window.location.assign("/sign-in");
              }}
              className={cx(
                "cursor-pointer rounded-sm text-sm/4.5 whitespace-nowrap text-ink-nav hover:text-ink",
                focusRing,
              )}
            >
              Sign out
            </button>
            <Avatar account={account} />
          </div>
        </div>
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
      </header>
      <main id="main" className={cx(column, "pt-10 pb-18")}>
        {children}
      </main>
    </div>
  );
}

function Avatar({ account }: { account: Account }) {
  const style = { boxShadow: "var(--avatar-ring) 0 0 0 1px" };
  if (account.avatarUrl) {
    return (
      <img
        src={account.avatarUrl}
        alt=""
        width={28}
        height={28}
        style={style}
        className="size-7 shrink-0 rounded-full object-cover"
      />
    );
  }
  return (
    <span
      aria-hidden="true"
      style={style}
      className="flex size-7 shrink-0 items-center justify-center rounded-full bg-sunken text-xs font-medium text-ink-2"
    >
      {account.name.slice(0, 1).toUpperCase()}
    </span>
  );
}
