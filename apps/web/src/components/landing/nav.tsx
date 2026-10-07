import { useEffect, useState } from "react";

import { Mark } from "#/components/logo";
import { fetchSignedInAccount } from "#/lib/auth-client";
import { GITHUB_URL, LAUNCHED, routes } from "#/lib/site";
import type { Account } from "#/lib/types";

import { AccountMenu } from "./account-menu";
import { ButtonLink, ComingSoon, Container, cx, focusRing } from "./ui";

const links = [
  { label: "Formats", href: "/#formats" },
  { label: "Pricing", href: "/#pricing" },
  { label: "API", href: "/#api" },
  // The repo was private before launch.
  ...(LAUNCHED ? [{ label: "GitHub", href: GITHUB_URL }] : []),
];

/**
 * The site nav, shared by the landing page and the account pages. Pages that already
 * loaded the session pass `account`; the prerendered landing page leaves it out and
 * asks the server once it is in the browser, showing the signed-out actions until then.
 */
export function Nav({ account, className }: { account?: Account | null; className?: string }) {
  const fetched = useBrowserAccount(account === undefined);
  const signedIn = account ?? fetched;
  return (
    <header>
      <Container className={cx("flex items-center justify-between py-6", className)}>
        <div className="flex items-center md:w-[200px] md:shrink-0">
          <a
            href="/"
            className={cx(
              "flex items-center gap-2 rounded-sm text-[17px]/[22px] font-semibold tracking-[-0.02em] text-ink",
              focusRing,
            )}
          >
            <Mark />
            convt
          </a>
        </div>
        <nav aria-label="Main" className="hidden md:block">
          <ul className="flex gap-8">
            {links.map((link) => (
              <li key={link.label}>
                <a
                  href={link.href}
                  className={cx(
                    "rounded-sm text-[14px]/[18px] text-ink-2 transition-colors hover:text-ink",
                    focusRing,
                  )}
                >
                  {link.label}
                </a>
              </li>
            ))}
          </ul>
        </nav>
        <div className="flex items-center justify-end gap-2 md:w-[200px] md:shrink-0">
          {LAUNCHED ? (
            <NavActions account={signedIn} />
          ) : signedIn ? (
            <AccountMenu account={signedIn} />
          ) : (
            <ComingSoon className="rounded-lg px-3 py-[7px] text-[14px]/[18px]" />
          )}
        </div>
      </Container>
    </header>
  );
}

function useBrowserAccount(enabled: boolean) {
  const [account, setAccount] = useState<Account | null>(null);
  useEffect(() => {
    if (!enabled) return;
    let current = true;
    void fetchSignedInAccount().then((value) => {
      if (current) setAccount(value);
    });
    return () => {
      current = false;
    };
  }, [enabled]);
  return account;
}

function NavActions({ account }: { account: Account | null }) {
  const download = (
    <ButtonLink
      variant="primary"
      href={routes.download}
      className="rounded-lg px-3 py-[7px] text-[14px]/[18px] shadow-[inset_0_1px_0_#ffffff47,0_0_0_1px_#157f4a,0_1px_2px_#0a3c2340,0_2px_6px_#0a3c231f]!"
    >
      Download
    </ButtonLink>
  );
  // Signed in, the avatar takes Sign in's place and moves to the far edge.
  if (account) {
    return (
      <>
        {download}
        <AccountMenu account={account} />
      </>
    );
  }
  return (
    <>
      <ButtonLink
        variant="secondary"
        href={routes.signIn}
        className="rounded-lg px-3 py-[7px] text-[14px]/[18px]"
      >
        Sign in
      </ButtonLink>
      {download}
    </>
  );
}
