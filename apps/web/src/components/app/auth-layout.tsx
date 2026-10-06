import { Link } from "@tanstack/react-router";

import { Mark } from "#/components/logo";
import { links } from "#/lib/config";

import { cx, focusRing } from "./ui";

const signInAside = {
  title: "AFTER YOU SIGN IN",
  items: ["Download the Mac app", "Find your license key and receipts", "Create API keys"],
};

/**
 * Two-column layout for sign-in and checkout results: the content on the left, the
 * dithered panel with a short list on the right.
 */
export function AuthLayout({
  children,
  aside = signInAside,
}: {
  children: React.ReactNode;
  aside?: { title: string; items: string[] };
}) {
  return (
    <div className="flex min-h-screen bg-page text-ink">
      <div className="flex w-full flex-col justify-between gap-12 px-6 py-10 sm:px-16 lg:w-[600px] lg:shrink-0">
        <Link
          to="/"
          className={cx(
            "flex items-center gap-2 self-start rounded-sm text-lg/5.5 font-semibold tracking-[-0.02em]",
            focusRing,
          )}
        >
          <Mark />
          convt
        </Link>
        <main className="flex w-full max-w-[380px] flex-col gap-6">{children}</main>
        <nav aria-label="Legal" className="flex gap-5 text-xs/4 text-ink-3">
          <a href={links.terms} className={cx("rounded-sm hover:text-ink", focusRing)}>
            Terms
          </a>
          <a href={links.privacy} className={cx("rounded-sm hover:text-ink", focusRing)}>
            Privacy
          </a>
          <a href={links.help} className={cx("rounded-sm hover:text-ink", focusRing)}>
            Help
          </a>
        </nav>
      </div>
      <div className="hidden flex-1 p-4 lg:flex">
        <div className="dither flex flex-1 items-end rounded-2xl bg-panel p-8 shadow-[inset_0_0_0_1px_var(--line)]">
          <div className="flex w-[360px] shrink-0 flex-col gap-3 rounded-xl bg-raised p-5 shadow-note">
            <h2 className="font-mono text-[11px]/3.5 text-ink-2">{aside.title}</h2>
            <ul className="flex flex-col gap-2">
              {aside.items.map((item) => (
                <li key={item} className="flex items-center gap-2.5 text-[13px]/4">
                  <span aria-hidden="true" className="size-1.5 shrink-0 rounded-[3px] bg-green" />
                  {item}
                </li>
              ))}
            </ul>
          </div>
        </div>
      </div>
    </div>
  );
}
