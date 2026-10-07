import { useEffect, useState } from "react";

import { headerLinks } from "#/components/site/links";
import { Mark } from "#/components/logo";
import { fetchSignedInAccount } from "#/lib/auth-client";
import { LAUNCHED, routes } from "#/lib/site";
import type { Account } from "#/lib/types";

import { AccountMenu } from "./account-menu";
import { ButtonLink, ComingSoon, Container, cx, focusRing } from "./ui";

/**
 * The site nav, shared by landing, the other public pages, and the account shell.
 * Pages that already loaded the session pass `account`. When `account` is omitted
 * (prerender), the browser asks once and the actions stay a skeleton until then —
 * never Sign in, so a signed-in visitor does not flash the wrong state.
 */
export function Nav({
  account,
  className,
  path = "/",
}: {
  account?: Account | null;
  className?: string;
  path?: string;
}) {
  const fetched = useBrowserAccount(account === undefined);
  const signedIn = account === undefined ? fetched : account;
  const onDownload = path === routes.download;
  const showDownloadCta = !onDownload && signedIn === null;
  const links = headerLinks.filter((link) => {
    // Signed-out visitors get the green Download button; signed-in visitors get
    // the text link. Never both, and never on /download itself.
    if (link.href === routes.download) return signedIn != null && !onDownload;
    return true;
  });

  return (
    <header>
      <Container className={cx("relative flex items-center justify-between py-6", className)}>
        <a
          href="/"
          className={cx(
            "relative z-10 flex items-center gap-2 rounded-sm text-[17px]/[22px] font-semibold tracking-[-0.02em] text-ink",
            focusRing,
          )}
        >
          <Mark />
          convt
        </a>
        <nav
          aria-label="Main"
          className="pointer-events-none absolute inset-0 hidden items-center justify-center md:flex"
        >
          <ul className="pointer-events-auto flex gap-8">
            {links.map((link) => (
              <li key={link.href}>
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
        <div className="relative z-10 flex items-center justify-end gap-2">
          {signedIn === undefined ? (
            <NavActionsSkeleton />
          ) : LAUNCHED ? (
            <NavActions account={signedIn} showDownload={showDownloadCta} />
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

/** `undefined` until the session request finishes, so the first paint is not Sign in. */
function useBrowserAccount(enabled: boolean) {
  const [account, setAccount] = useState<Account | null | undefined>(undefined);
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
  return enabled ? account : null;
}

function NavActionsSkeleton() {
  return (
    <div className="flex items-center gap-2" aria-hidden="true">
      <span className="h-8 w-[4.5rem] rounded-lg bg-sunken" />
      <span className="h-8 w-24 rounded-lg bg-sunken" />
    </div>
  );
}

function NavActions({ account, showDownload }: { account: Account | null; showDownload: boolean }) {
  const download = showDownload ? (
    <ButtonLink
      variant="primary"
      href={routes.download}
      className="rounded-lg px-3 py-[7px] text-[14px]/[18px] shadow-[inset_0_1px_0_#ffffff47,0_0_0_1px_#157f4a,0_1px_2px_#0a3c2340,0_2px_6px_#0a3c231f]!"
    >
      Download
    </ButtonLink>
  ) : null;
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
