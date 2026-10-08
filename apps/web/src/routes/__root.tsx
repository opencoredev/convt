import { lazy, Suspense, useEffect } from "react";
import { HeadContent, Scripts, createRootRoute, useMatches } from "@tanstack/react-router";

import { IdentifyUser } from "#/components/identify-user";
import { PostHogProvider } from "#/components/posthog-provider";
import { rememberAttribution } from "#/lib/analytics-attribution";
import { getPublicConfig } from "#/server/public-config";
import { getSession } from "#/server/session";
import appCss from "../styles.css?url";

declare module "@tanstack/react-router" {
  interface StaticDataRouteOption {
    /** Pin a route to one theme, e.g. the landing page, which is designed dark only. */
    theme?: "light" | "dark";
  }
}

// Devtools load only in development; production builds never import them.
const Devtools = import.meta.env.DEV
  ? lazy(async () => {
      const [{ TanStackDevtools }, { TanStackRouterDevtoolsPanel }] = await Promise.all([
        import("@tanstack/react-devtools"),
        import("@tanstack/react-router-devtools"),
      ]);
      return {
        default: () => (
          <TanStackDevtools
            config={{ position: "bottom-right" }}
            plugins={[{ name: "Tanstack Router", render: <TanStackRouterDevtoolsPanel /> }]}
          />
        ),
      };
    })
  : null;

export const Route = createRootRoute({
  loader: async () => {
    const [config, session] = await Promise.all([getPublicConfig(), getSession()]);
    return { posthog: config.posthog, userId: session?.user.id ?? null };
  },
  head: () => ({
    meta: [
      { charSet: "utf-8" },
      { name: "viewport", content: "width=device-width, initial-scale=1" },
      { title: "convt" },
      {
        name: "description",
        content: "Convert images, video, audio and documents on your own computer.",
      },
    ],
    links: [
      { rel: "stylesheet", href: appCss },
      { rel: "icon", href: "/favicon.ico", sizes: "48x48" },
      { rel: "icon", href: "/favicon.svg", type: "image/svg+xml" },
      { rel: "apple-touch-icon", href: "/apple-touch-icon.png" },
    ],
  }),
  shellComponent: RootDocument,
});

function RootDocument({ children }: { children: React.ReactNode }) {
  const theme = useMatches({
    // The deepest route that pins a theme wins.
    select: (matches) =>
      [...matches].reverse().find((match) => match.staticData.theme)?.staticData.theme,
  });

  return (
    // The account pages set `dark` from the OS before hydration, so the class can differ.
    <html lang="en" className={theme === "dark" ? "dark" : undefined} suppressHydrationWarning>
      <head>
        <HeadContent />
      </head>
      <body>
        <PostHogRoot>{children}</PostHogRoot>
        {Devtools && (
          <Suspense fallback={null}>
            <Devtools />
          </Suspense>
        )}
        <Scripts />
      </body>
    </html>
  );
}

function PostHogRoot({ children }: { children: React.ReactNode }) {
  const { posthog, userId } = Route.useLoaderData();
  useEffect(() => {
    rememberAttribution();
  }, []);
  return (
    <PostHogProvider config={posthog}>
      <IdentifyUser userId={userId} />
      {children}
    </PostHogProvider>
  );
}
