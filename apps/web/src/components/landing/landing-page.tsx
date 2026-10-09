import { useEffect } from "react";

import type { Account } from "#/lib/types";

import { CallToAction, Footer } from "./closing";
import { Engines } from "./engines";
import { Formats } from "./formats";
import { Hero } from "./hero";
import { Nav } from "./nav";
import { Pricing } from "./pricing";
import { focusRing } from "./ui";

/** convt.app home page. Always dark; the route sets `dark` on <html>. */
export function LandingPage({
  sales,
  account,
}: {
  sales: "desktop" | "all";
  account: Account | null;
}) {
  useReveal();
  return (
    <div className="min-h-screen overflow-x-clip bg-page text-ink">
      <a
        href="#main"
        className={`sr-only rounded-md bg-raised px-3 py-2 text-sm focus:not-sr-only focus:fixed focus:top-3 focus:left-3 focus:z-50 ${focusRing}`}
      >
        Skip to content
      </a>
      <Nav account={account} path="/" />
      <main id="main">
        <Hero signedIn={account != null} />
        <Formats />
        <Engines />
        <div data-reveal="" className="reveal">
          <Pricing sales={sales} />
        </div>
        <div data-reveal="" className="reveal">
          <CallToAction signedIn={account != null} />
        </div>
      </main>
      <Footer />
    </div>
  );
}

/**
 * Fades `[data-reveal]` elements in as they scroll into view. Without JavaScript, or with
 * reduced motion, everything is simply visible: only elements still below the fold when
 * the page hydrates get hidden, so nothing on screen blinks out. Same as /brand.
 */
function useReveal() {
  useEffect(() => {
    const items = [...document.querySelectorAll<HTMLElement>("[data-reveal]")];
    if (matchMedia("(prefers-reduced-motion: reduce)").matches) return;
    const observer = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (!entry.isIntersecting || !(entry.target instanceof HTMLElement)) continue;
          entry.target.dataset.reveal = "shown";
          observer.unobserve(entry.target);
        }
      },
      { rootMargin: "0px 0px -8% 0px" },
    );
    for (const item of items) {
      if (item.getBoundingClientRect().top < innerHeight) continue;
      item.dataset.reveal = "hidden";
      observer.observe(item);
    }
    return () => observer.disconnect();
  }, []);
}
