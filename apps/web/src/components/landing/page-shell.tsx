import type { ReactNode } from "react";

import { CallToAction, Footer } from "./closing";
import { Nav } from "./nav";
import { focusRing } from "./ui";

/** The landing page's frame (nav, closing call to action, footer) for other public pages. */
export function PageShell({ children }: { children: ReactNode }) {
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
        {children}
        <div className="pt-16 md:pt-24">
          <CallToAction />
        </div>
      </main>
      <Footer />
    </div>
  );
}
