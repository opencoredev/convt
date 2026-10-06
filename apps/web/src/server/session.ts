// Session access for pages and server functions. Route context is never trusted on
// its own: every server function that touches account data runs `authed`, which
// validates the session cookie again.

import { createMiddleware, createServerFn } from "@tanstack/react-start";
import { setResponseHeader } from "@tanstack/react-start/server";
import { redirect } from "@tanstack/react-router";

import { loadSession } from "./context";

export type SessionInfo = {
  user: { id: string; name: string; email: string; emailVerified: boolean; image: string | null };
  sessionId: string;
};

/** The signed-in user for the header and the page guards, or null. */
export const getSession = createServerFn({ method: "GET" }).handler(
  async ({ context }): Promise<SessionInfo | null> => {
    setResponseHeader("cache-control", "private, no-store");
    const { session } = await loadSession(context);
    if (!session) return null;
    const u = session.user;
    return {
      user: {
        id: u.id,
        name: u.name,
        email: u.email,
        emailVerified: u.emailVerified,
        image: u.image ?? null,
      },
      sessionId: session.session.id,
    };
  },
);

/** Server function middleware for account data: a valid session with a verified email. */
export const authed = createMiddleware({ type: "function" }).server(async ({ next, context }) => {
  setResponseHeader("cache-control", "private, no-store");
  const { auth, scope, appEnv, session } = await loadSession(context);
  if (!session) throw redirect({ to: "/sign-in" });
  if (!session.user.emailVerified) throw redirect({ to: "/sign-in/verify-email" });
  return next({
    context: {
      auth,
      db: scope.db,
      appEnv,
      userId: session.user.id,
      sessionId: session.session.id,
      sessionCreatedAt: new Date(session.session.createdAt),
      user: session.user,
    },
  });
});
