import { lazy, Suspense } from "react";
import { HeadContent, Scripts, createRootRoute, useMatches } from "@tanstack/react-router";

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
    links: [{ rel: "stylesheet", href: appCss }],
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
        {children}
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
