import { Mark } from "#/components/logo";
import { GITHUB_URL, LAUNCHED, routes } from "#/lib/site";

import { ButtonLink, ComingSoon, Container, cx, focusRing } from "./ui";

const links = [
  { label: "Formats", href: "#formats" },
  { label: "Pricing", href: "#pricing" },
  { label: "API", href: "#api" },
  // The repo is private until launch.
  ...(LAUNCHED ? [{ label: "GitHub", href: GITHUB_URL }] : []),
];

export function Nav() {
  return (
    <header>
      <Container className="flex items-center justify-between py-6">
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
            <NavActions />
          ) : (
            <ComingSoon className="rounded-lg px-3 py-[7px] text-[14px]/[18px]" />
          )}
        </div>
      </Container>
    </header>
  );
}

function NavActions() {
  return (
    <>
      <ButtonLink
        variant="secondary"
        href={routes.signIn}
        className="rounded-lg px-3 py-[7px] text-[14px]/[18px]"
      >
        Sign in
      </ButtonLink>
      <ButtonLink
        variant="primary"
        href={routes.download}
        className="rounded-lg px-3 py-[7px] text-[14px]/[18px] shadow-[inset_0_1px_0_#ffffff47,0_0_0_1px_#157f4a,0_1px_2px_#0a3c2340,0_2px_6px_#0a3c231f]!"
      >
        Download
      </ButtonLink>
    </>
  );
}
