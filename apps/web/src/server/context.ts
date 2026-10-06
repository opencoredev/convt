// Server-only helpers for server functions and routes. Import this only from
// handler bodies (createServerFn().handler, middleware .server, server routes):
// the Start compiler drops imports used only there from the browser bundle, and
// this module pulls in pg and Better Auth.

import { getRequestHeaders } from "@tanstack/react-start/server";

import { getUser, type Db } from "@convt/db";

import { createAuth } from "./auth";
import type { RequestContext } from "./request";

export function requestContext(context: unknown): RequestContext {
  const c = context as Partial<RequestContext> | undefined;
  if (!c?.scope || !c.appEnv) throw new Error("the request middleware did not run");
  return c as RequestContext;
}

export async function loadSession(context: unknown) {
  const { scope, appEnv } = requestContext(context);
  const auth = createAuth(scope, appEnv);
  const session = await auth.api.getSession({ headers: getRequestHeaders() as unknown as Headers });
  return { auth, scope, appEnv, session };
}

export async function signedInUser(db: Db, userId: string) {
  const user = await getUser(db, userId);
  if (!user) throw new Error("the signed-in user no longer exists");
  return user;
}
