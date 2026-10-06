import { createFileRoute } from "@tanstack/react-router";

import { LandingPage } from "#/components/landing/landing-page";
import { seo } from "#/lib/site";

const title = "convt: convert any file with a right-click";
const description =
  "Images, video, audio and documents, converted on your own computer. Nothing gets uploaded.";

export const Route = createFileRoute("/")({
  // The landing page is designed dark only. __root reads this and puts `dark` on <html>.
  staticData: { theme: "dark" },
  head: () => {
    const tags = seo({ title, description, path: "/" });
    return { ...tags, meta: [...tags.meta, { name: "theme-color", content: "#0a0b0b" }] };
  },
  component: LandingPage,
});
