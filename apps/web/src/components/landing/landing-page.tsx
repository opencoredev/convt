import { CallToAction, Footer } from "./closing";
import { Engines } from "./engines";
import { Formats } from "./formats";
import { Hero } from "./hero";
import { Nav } from "./nav";
import { Pricing } from "./pricing";
import { focusRing } from "./ui";

// Marks each [data-reveal-group] and [data-reveal-watch] with data-shown the first time it
// scrolls into view, which plays its entrance in styles.css. Plain inline script, not
// React: the static build hydrates late, and the reveals should not wait for it. The
// data-reveal attribute that arms the hidden state is only set when this script runs and
// motion is allowed, so without JavaScript nothing is hidden.
const revealScript = `(function () {
  if (!("IntersectionObserver" in window)) return;
  if (matchMedia("(prefers-reduced-motion: reduce)").matches) return;
  document.documentElement.setAttribute("data-reveal", "");
  addEventListener("DOMContentLoaded", function () {
    var io = new IntersectionObserver(
      function (entries) {
        entries.forEach(function (entry) {
          // Also reveal groups already above the viewport, e.g. after a reload mid-page.
          if (!entry.isIntersecting && entry.boundingClientRect.top > 0) return;
          entry.target.setAttribute("data-shown", "");
          io.unobserve(entry.target);
        });
      },
      { rootMargin: "0px 0px -12% 0px" },
    );
    document.querySelectorAll("[data-reveal-group],[data-reveal-watch]").forEach(function (el) {
      io.observe(el);
    });
  });
})();`;

/** convt.app home page. Always dark; the route sets `dark` on <html>. */
export function LandingPage() {
  return (
    <div className="min-h-screen overflow-x-clip bg-page text-ink">
      <a
        href="#main"
        className={`sr-only rounded-md bg-raised px-3 py-2 text-sm focus:not-sr-only focus:fixed focus:top-3 focus:left-3 focus:z-50 ${focusRing}`}
      >
        Skip to content
      </a>
      <script dangerouslySetInnerHTML={{ __html: revealScript }} />
      <Nav />
      <main id="main">
        <Hero />
        <Formats />
        <Engines />
        <Pricing />
        <CallToAction />
      </main>
      <Footer />
    </div>
  );
}
