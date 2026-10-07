import { defineConfig } from "blume";
import { openapi } from "blume/reference";
import { apiBase, finalApi } from "./api-host.ts";

export default defineConfig({
  title: "convt docs",
  description:
    "Convert files from your own code with the convt API: reserve a job, upload, start, poll and download.",
  logo: { image: "/logo.svg", text: "convt", href: "/" },
  content: { root: "content" },
  variables: {
    api: apiBase,
    "final-api": finalApi,
  },
  theme: {
    accent: { light: "#17834d", dark: "#3fcb84" },
    radius: "md",
    mode: "system",
    fonts: { display: "geist", body: "geist", mono: "geist-mono" },
  },
  markdown: {
    code: {
      icons: true,
      theme: { light: "vitesse-light", dark: "vitesse-dark" },
    },
  },
  navigation: {
    tabs: [
      { label: "Docs", path: "/", href: "/" },
      { label: "API reference", path: "/api" },
      { label: "Examples", path: "/examples" },
    ],
  },
  reference: [
    openapi({
      route: "/api",
      spec: "../../crates/convt-server/openapi.json",
      overlays: ["./openapi/public.yaml", "./openapi/servers.yaml"],
      codeSamples: ["curl", "node", "js", "python"],
    }),
  ],
  github: { owner: "opencoredev", repo: "convt", dir: "apps/docs" },
  footer: {
    links: [
      { label: "convt.app", href: "https://convt.app" },
      { label: "Dashboard", href: "https://convt.app/dashboard/api" },
      { label: "Pricing", href: "https://convt.app/#pricing" },
    ],
  },
  feedback: false,
  deployment: {
    site: "https://convt.app",
    base: "/docs",
  },
});
