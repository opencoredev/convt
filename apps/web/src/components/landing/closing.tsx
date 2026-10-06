import { LAUNCHED, routes } from "#/lib/site";

import { footerColumns } from "../site/links";
import { ButtonLink, ComingSoon, Container, cx, focusRing } from "./ui";

export function CallToAction() {
  return (
    <Container>
      <section
        aria-labelledby="cta-title"
        className="bg-land-glow flex flex-col items-center gap-7 overflow-clip rounded-2xl bg-bottom px-5 pt-16 pb-24 text-center md:h-[460px] md:pt-[88px] md:pb-0"
      >
        <div className="flex flex-col items-center gap-3.5">
          <h2
            id="cta-title"
            className="text-[34px]/[40px] font-medium tracking-[-0.04em] text-balance text-ink md:text-[56px]/[60px]"
          >
            Stop uploading your files
            <br className="hidden md:inline" /> to convert them.
          </h2>
          <p className="text-[17px]/[26px] text-land-soft">
            Try convt free for 7 days. Your files stay on your machine.
          </p>
        </div>
        <div className="flex flex-wrap justify-center gap-3">
          {LAUNCHED ? (
            <DownloadActions />
          ) : (
            <ComingSoon className="h-11 rounded-[10px] px-5 text-[15px]/[18px]">
              Coming soon to macOS, Windows and Linux
            </ComingSoon>
          )}
        </div>
      </section>
    </Container>
  );
}

function DownloadActions() {
  return (
    <>
      <ButtonLink
        variant="primary"
        href={`${routes.download}?os=macos`}
        className="h-11 rounded-[10px] px-5 text-[15px]/[18px]"
      >
        Download for macOS
      </ButtonLink>
      <ButtonLink
        variant="secondary"
        href={`${routes.download}#platforms`}
        className="h-11 rounded-[10px] px-5 text-[15px]/[18px]"
      >
        Windows and Linux
      </ButtonLink>
    </>
  );
}

export function Footer() {
  return (
    <footer>
      <Container className="flex flex-col gap-14 pt-16 pb-12 md:pt-24">
        <div className="flex flex-col gap-10 md:flex-row md:justify-between">
          <div className="flex max-w-[320px] flex-col gap-3 md:w-[320px] md:shrink-0">
            <p className="text-[20px]/[24px] font-semibold tracking-[-0.03em] text-ink">convt</p>
            <p className="text-[14px]/[21px] text-ink-2">
              Local file conversion for macOS, Windows and Linux. Open source under AGPL-3.0.
            </p>
          </div>
          {/* Every footer page needs the full site, so the coming-soon page has none. */}
          {LAUNCHED && (
            <div className="flex flex-wrap gap-x-20 gap-y-10">
              {footerColumns.map((column) => (
                <nav
                  key={column.title}
                  aria-labelledby={`footer-${column.title}`}
                  className="flex flex-col gap-3"
                >
                  <h2
                    id={`footer-${column.title}`}
                    className="font-mono text-[12px]/[16px] font-normal text-land-muted uppercase"
                  >
                    {column.title}
                  </h2>
                  <ul className="flex flex-col gap-3">
                    {column.links.map((link) => (
                      <li key={link.label}>
                        <a
                          href={link.href}
                          className={cx(
                            "rounded-sm text-[14px]/[18px] text-ink transition-colors hover:text-ink-2",
                            focusRing,
                          )}
                        >
                          {link.label}
                        </a>
                      </li>
                    ))}
                  </ul>
                </nav>
              ))}
            </div>
          )}
        </div>
        <div className="flex flex-col gap-2 border-t border-line pt-6 sm:flex-row sm:justify-between">
          <p className="text-[13px]/[16px] text-land-muted">© 2026 convt</p>
          <p className="font-mono text-[12px]/[16px] text-land-muted">Built with Rust and GPUI</p>
        </div>
      </Container>
    </footer>
  );
}
