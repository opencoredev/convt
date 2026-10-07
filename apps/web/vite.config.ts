import { defineConfig } from "vite";
import { devtools } from "@tanstack/devtools-vite";

import { tanstackStart } from "@tanstack/react-start/plugin/vite";

import viteReact from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { cloudflare } from "@cloudflare/vite-plugin";

const landingOnly = process.env.CONVT_LANDING_ONLY === "1";

const config = defineConfig({
  resolve: { tsconfigPaths: true },
  plugins: [
    devtools(),
    // convt-billing runs beside the site as an auxiliary Worker, reached through the
    // BILLING service binding; its settings come from apps/billing/.dev.vars.
    cloudflare({
      viteEnvironment: { name: "ssr" },
      auxiliaryWorkers: [{ configPath: "../billing/wrangler.jsonc" }],
    }),
    tailwindcss(),
    // CSRF is checked by our own request middleware (src/server/request.ts): every
    // request that is not GET or HEAD needs this site's Origin or
    // Sec-Fetch-Site: same-origin, server functions included.
    tanstackStart({
      serverFns: { disableCsrfMiddlewareWarning: true },
      // `bun run build:landing` prerenders the home and brand pages to static HTML for the
      // static Vercel deploy (see LAUNCHED in src/lib/site.ts).
      ...(landingOnly && {
        prerender: { enabled: true, autoStaticPathsDiscovery: false, crawlLinks: false },
        pages: [{ path: "/" }, { path: "/brand" }],
      }),
    }),
    viteReact(),
  ],
});

export default config;
