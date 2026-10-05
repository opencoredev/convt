import { CallToAction, Footer } from "./closing";
import { Engines } from "./engines";
import { Formats } from "./formats";
import { Hero } from "./hero";
import { Nav } from "./nav";
import { Pricing } from "./pricing";
import { focusRing } from "./ui";

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
