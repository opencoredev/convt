// Better Auth for convt.app. Built per request, because a Worker cannot share a
// database connection across requests. See docs/p6-accounts-plan.md, section 2.

import {
  claimPurchases,
  consumeSendBucket,
  revokeAllDevices,
  revokeOtherSessions,
  type Db,
} from "@convt/db";
import { newId, type IdPrefix } from "@convt/license";
import * as schema from "@convt/db/schema";
import { betterAuth, type BetterAuthOptions } from "better-auth";
import { drizzleAdapter } from "better-auth/adapters/drizzle";
import {
  APIError,
  addOAuthServerContext,
  createAuthMiddleware,
  getOAuthState,
  getSessionFromCtx,
} from "better-auth/api";
import { deleteSessionCookie } from "better-auth/cookies";
import { emailOTP } from "better-auth/plugins/email-otp";
import { genericOAuth, type GenericOAuthConfig } from "better-auth/plugins/generic-oauth";
import { tanstackStartCookies } from "better-auth/tanstack-start";
import { and, eq, lt, ne, sql } from "drizzle-orm";

import {
  captureEvent,
  signupEventFromAuthHook,
  type AuthHookContext,
  type CaptureAnalytics,
} from "./analytics";
import { googleEmailIsAuthoritative, type GoogleClaims } from "./authoritative";
import type { AppEnv } from "./env";
import { codeEmail, sendMail, type MailMessage } from "./mail";
import { redactingLog } from "./redact";

export const codeMinutes = 15;
export const freshAgeSeconds = 60 * 60;
const linkIntentMaxAgeMs = 10 * 60 * 1000;

/** What one request shares with Better Auth: its database client and background work. */
export type RequestScope = {
  db: Db;
  background: (work: Promise<unknown>) => void;
};

export type AuthDeps = {
  /** Replaces the mail transport (tests capture codes this way). */
  sendMail?: (message: MailMessage) => Promise<void>;
  /** Off for Better Auth calls outside a TanStack Start request (tests). */
  startCookies?: boolean;
  /** Replaces PostHog capture (tests record signup events this way). */
  captureAnalytics?: CaptureAnalytics;
};

const idPrefixes: Record<string, IdPrefix> = {
  user: "usr",
  session: "ses",
  account: "acc",
  verification: "ver",
  rateLimit: "rl",
};

const sendPaths = new Set(["/email-otp/send-verification-otp", "/email-otp/request-email-change"]);
const freshPaths = new Set([
  "/link-social",
  "/email-otp/request-email-change",
  "/email-otp/change-email",
]);

export const sendLimits = {
  email: { max: 3, windowMs: 15 * 60 * 1000 },
  ip: { max: 10, windowMs: 60 * 60 * 1000 },
};

async function sha256Hex(text: string): Promise<string> {
  const digest = new Uint8Array(
    await crypto.subtle.digest("SHA-256", new TextEncoder().encode(text)),
  );
  return [...digest].map((b) => b.toString(16).padStart(2, "0")).join("");
}

function clientIp(headers: Headers | undefined): string {
  return headers?.get("cf-connecting-ip")?.trim() || "unknown";
}

type LinkIntent = { userId: string; sessionId: string; provider: string; at: number };

function isLinkIntent(value: unknown): value is LinkIntent {
  const v = value as LinkIntent | null;
  return (
    !!v &&
    typeof v.userId === "string" &&
    typeof v.sessionId === "string" &&
    typeof v.provider === "string" &&
    typeof v.at === "number"
  );
}

/** GitHub through the local mock: the same profile logic as Better Auth's provider. */
function mockGithub(mock: NonNullable<AppEnv["oauthMock"]>): GenericOAuthConfig {
  return {
    providerId: "github",
    clientId: "convt-local",
    clientSecret: "mock-secret",
    authorizationUrl: `${mock.publicUrl}/github/authorize`,
    tokenUrl: `${mock.url}/github/token`,
    scopes: ["user:email"],
    pkce: true,
    async getUserInfo(tokens) {
      const headers = { authorization: `Bearer ${tokens.accessToken}` };
      const res = await fetch(`${mock.url}/github/user`, { headers });
      if (!res.ok) return null;
      const profile = (await res.json()) as {
        id: number;
        login: string;
        name: string | null;
        email: string | null;
        avatar_url: string | null;
      };
      const emails = (await (
        await fetch(`${mock.url}/github/user/emails`, { headers })
      ).json()) as Array<{ email: string; primary: boolean }>;
      const email = profile.email ?? (emails.find((e) => e.primary) ?? emails[0])?.email;
      return {
        id: String(profile.id),
        name: profile.name || profile.login,
        email,
        image: profile.avatar_url ?? undefined,
        emailVerified: false,
      };
    },
    mapProfileToUser: () => ({ emailVerified: false }),
  };
}

function mockGoogle(mock: NonNullable<AppEnv["oauthMock"]>): GenericOAuthConfig {
  return {
    providerId: "google",
    clientId: "convt-local",
    clientSecret: "mock-secret",
    discoveryUrl: `${mock.url}/google/.well-known/openid-configuration`,
    scopes: ["openid", "email", "profile"],
    pkce: true,
    mapProfileToUser: (profile) => ({
      emailVerified: googleEmailIsAuthoritative(profile as GoogleClaims),
    }),
  };
}

export function authOptions(scope: RequestScope, env: AppEnv, deps: AuthDeps = {}) {
  const { db } = scope;
  const deliver = deps.sendMail ?? ((message: MailMessage) => sendMail(env.mail, message));
  const capture = deps.captureAnalytics ?? ((event) => captureEvent(env.posthog, event));
  // Set by the before hook on /sign-in/email-otp (and the user create hook), read
  // by its after hook. The auth instance lives for one request, so these cannot
  // leak across requests.
  let accountBeforeSignIn: string | null = null;
  let unverifiedBeforeSignIn: string | null = null;
  let userCreatedHere: string | null = null;

  const isFresh = (createdAt: Date) => Date.now() - createdAt.getTime() < freshAgeSeconds * 1000;

  const notFresh = () =>
    new APIError("FORBIDDEN", { code: "SESSION_NOT_FRESH", message: "Sign in again to continue." });

  async function consumeSendLimits(email: string, ip: string) {
    const now = new Date();
    const emailCount = await consumeSendBucket(
      db,
      `email:${await sha256Hex(email)}`,
      sendLimits.email.windowMs,
      now,
    );
    const ipCount = await consumeSendBucket(db, `ip:${ip}`, sendLimits.ip.windowMs, now);
    if (emailCount > sendLimits.email.max || ipCount > sendLimits.ip.max) {
      throw new APIError("TOO_MANY_REQUESTS", {
        code: "TOO_MANY_CODES",
        message: "Too many codes were sent. Wait a few minutes and try again.",
      });
    }
  }

  const options = {
    appName: "convt",
    baseURL: env.authUrl,
    secret: env.authSecret,
    trustedOrigins: [env.authUrl],
    database: drizzleAdapter(db, { provider: "pg", usePlural: true, schema }),
    emailAndPassword: { enabled: false },
    // Better Auth would delete every expired verification row on each lookup, in one
    // statement whose row-lock order can cycle with a code being issued (marker,
    // then code). Expired rows are already treated as invalid; convt-server's
    // cleanup task removes them every 10 minutes, markers first.
    verification: { disableCleanup: true },
    session: {
      expiresIn: 30 * 24 * 60 * 60,
      updateAge: 24 * 60 * 60,
      freshAge: freshAgeSeconds,
      cookieCache: { enabled: false },
    },
    account: {
      encryptOAuthTokens: true,
      accountLinking: {
        enabled: true,
        // Trusted only so an explicit link from settings is accepted: our mapping marks
        // GitHub (and non-authoritative Google) emails unverified, and Better Auth refuses
        // to link an untrusted provider's unverified email. Implicit linking stays off.
        trustedProviders: ["github", "google"],
        disableImplicitLinking: true,
        allowDifferentEmails: true,
        // The verified email is always a sign-in method (by code), so the last OAuth
        // identity can go too.
        allowUnlinkingAll: true,
      },
    },
    socialProviders: {
      ...(env.github
        ? {
            github: {
              ...env.github,
              scope: ["user:email"],
              mapProfileToUser: () => ({ emailVerified: false }),
            },
          }
        : {}),
      ...(env.google
        ? {
            google: {
              ...env.google,
              mapProfileToUser: (profile: GoogleClaims) => ({
                emailVerified: googleEmailIsAuthoritative(profile),
              }),
            },
          }
        : {}),
    },
    rateLimit: {
      enabled: true,
      storage: "database",
      window: 60,
      max: 100,
      customRules: {
        "/sign-in/email-otp": { window: 60, max: 10 },
        "/email-otp/verify-email": { window: 60, max: 10 },
        "/email-otp/change-email": { window: 60, max: 10 },
        "/sign-in/social": { window: 60, max: 20 },
        "/sign-in/oauth2": { window: 60, max: 20 },
        "/link-social": { window: 60, max: 20 },
        "/callback/*": { window: 60, max: 20 },
        "/oauth2/callback/*": { window: 60, max: 20 },
        // The send buckets in otp_send_limits govern these.
        "/email-otp/send-verification-otp": { window: 60, max: 100 },
        "/email-otp/request-email-change": { window: 60, max: 100 },
      },
    },
    disabledPaths: [
      "/email-otp/request-password-reset",
      "/forget-password/email-otp",
      "/email-otp/reset-password",
      "/email-otp/check-verification-otp",
    ],
    onAPIError: { errorURL: `${env.authUrl}/sign-in` },
    advanced: {
      cookiePrefix: "convt",
      useSecureCookies: env.env === "production" || env.env === "staging",
      ipAddress: { ipAddressHeaders: ["cf-connecting-ip"] },
      database: { generateId: ({ model }: { model: string }) => newId(idPrefixes[model] ?? "usr") },
      backgroundTasks: { handler: scope.background },
    },
    logger: {
      level: env.env === "production" || env.env === "staging" ? "warn" : "info",
      log: redactingLog,
    },
    hooks: {
      before: createAuthMiddleware(async (ctx) => {
        const path = ctx.path;
        const body = (ctx.body ?? {}) as Record<string, unknown>;
        if (freshPaths.has(path)) {
          const session = await getSessionFromCtx(ctx);
          if (session && !isFresh(new Date(session.session.createdAt))) throw notFresh();
          if (path === "/link-social") {
            // An idToken links without a redirect, so it would skip the intent check.
            if (body.idToken)
              throw new APIError("BAD_REQUEST", {
                code: "ID_TOKEN_NOT_ALLOWED",
                message: "Link through the provider's sign-in page.",
              });
            if (session) {
              const intent: LinkIntent = {
                userId: session.user.id,
                sessionId: session.session.id,
                provider: String(body.provider ?? ""),
                at: Date.now(),
              };
              await addOAuthServerContext({ linkIntent: intent });
            }
          }
        }
        if (sendPaths.has(path)) {
          const target = String(
            (path === "/email-otp/request-email-change" ? body.newEmail : body.email) ?? "",
          )
            .trim()
            .toLowerCase();
          if (target) {
            await consumeSendLimits(target, clientIp(ctx.headers ?? ctx.request?.headers));
          }
        }
        if (path === "/email-otp/verify-email") {
          // Verifying keeps the account's OAuth identities and sessions, which is
          // right only when the request comes from that account's own session: then
          // one browser proved both the identity and the mailbox. From anywhere else,
          // the mailbox owner signs in with a code instead, which removes unproven
          // access (see /sign-in/email-otp).
          const email = String(body.email ?? "")
            .trim()
            .toLowerCase();
          const [user] = await db
            .select({ id: schema.users.id, emailVerified: schema.users.emailVerified })
            .from(schema.users)
            .where(eq(schema.users.email, email));
          if (user && !user.emailVerified) {
            const session = await getSessionFromCtx(ctx);
            if (session?.user.id !== user.id) {
              throw new APIError("FORBIDDEN", {
                code: "VERIFY_FROM_ACCOUNT",
                message: "Sign in with an email code instead.",
              });
            }
          }
        }
        if (path === "/sign-in/email-otp") {
          const email = String(body.email ?? "")
            .trim()
            .toLowerCase();
          const [user] = await db
            .select({ id: schema.users.id, emailVerified: schema.users.emailVerified })
            .from(schema.users)
            .where(eq(schema.users.email, email));
          accountBeforeSignIn = user?.id ?? null;
          unverifiedBeforeSignIn = user && !user.emailVerified ? user.id : null;
          if (unverifiedBeforeSignIn) {
            // Better Auth removes unproven access under a short lock row. With its
            // inline expiry deletes off, a lock left behind by an interrupted attempt
            // would make it skip that cleanup, so drop an expired one first.
            await db
              .delete(schema.verifications)
              .where(
                and(
                  eq(
                    schema.verifications.identifier,
                    `revoke-unproven-account-access:${unverifiedBeforeSignIn}`,
                  ),
                  lt(schema.verifications.expiresAt, new Date()),
                ),
              );
          }
        }
      }),
      after: createAuthMiddleware(async (ctx) => {
        const failed = ctx.context.returned instanceof Error;
        if (failed) return;
        if (ctx.path === "/sign-in/email-otp") {
          const created = ctx.context.newSession;
          if (!created) return;
          // The code was issued for whichever account had the address when the
          // request began, or for a new one. If the address changed hands before
          // Better Auth looked the account up, the session belongs to an account
          // the code was never checked against (perhaps an unverified squatter
          // whose cleanup Better Auth skipped), so end it and ask for a new code.
          if (created.user.id !== (accountBeforeSignIn ?? userCreatedHere)) {
            await db.delete(schema.sessions).where(eq(schema.sessions.id, created.session.id));
            deleteSessionCookie(ctx);
            throw new APIError("CONFLICT", {
              code: "ACCOUNT_CHANGED",
              message: "Something changed while you were signing in. Send a new code.",
            });
          }
          // The code proved the mailbox; nothing else on an unverified account was
          // proven to belong to its owner. Better Auth removes the OAuth identities
          // and sessions under a lock it gives up on after two seconds (and then
          // signs in to the still unverified account), so do it here too, whatever
          // happened there: every identity, every session but the one just
          // created, every device, then mark the email verified and claim.
          if (unverifiedBeforeSignIn !== created.user.id && created.user.emailVerified) return;
          const userId = created.user.id;
          const keep = created.session.id;
          // Two code sign-ins finishing at once (possible only while a stalled
          // takeover holds Better Auth's lock) each delete the other's new session;
          // both then sign in again. Keeping sessions created during the request
          // instead could keep one an attacker opened in that window.
          await db.transaction(async (tx) => {
            await tx.delete(schema.accounts).where(eq(schema.accounts.userId, userId));
            await tx
              .delete(schema.sessions)
              .where(and(eq(schema.sessions.userId, userId), ne(schema.sessions.id, keep)));
            await tx
              .update(schema.users)
              .set({ emailVerified: true, updatedAt: new Date() })
              .where(eq(schema.users.id, userId));
          });
          await revokeAllDevices(db, userId, new Date());
          await claimPurchases(db, userId);
        }
        if (ctx.path === "/email-otp/change-email") {
          const session = ctx.context.session;
          if (session)
            await revokeOtherSessions(db, session.user.id, session.session.id, new Date());
        }
      }),
    },
    databaseHooks: {
      user: {
        create: {
          after: async (user, hookCtx) => {
            userCreatedHere = user.id;
            await claimPurchases(db, user.id);
            // Once per account, from the insert path so an ad blocker cannot drop it.
            scope.background(
              capture(signupEventFromAuthHook(user.id, hookCtx as AuthHookContext, env.authUrl)),
            );
          },
        },
        update: { after: async (user) => void (await claimPurchases(db, user.id)) },
      },
      session: {
        create: {
          after: async (session) => void (await claimPurchases(db, session.userId)),
        },
      },
      verification: {
        create: {
          // Sign-in codes: one row per identifier, written atomically. The plugin
          // inserts, and on a unique violation deletes and inserts again, which
          // concurrent sends to one address can interleave into an error. Here a new
          // code (attempt count 0) replaces whatever is there, so the newest code
          // wins, and it is recorded in an `otp-issued:` marker row that consuming a
          // code leaves in place. The marker names the issuance by code hash and
          // expiry; the expiry is made strictly later than the previous issuance's
          // in the same statement, so two sends never share an issuance even when
          // they repeat six digits within one millisecond.
          //
          // After a wrong guess the plugin consumes the row and writes the same code
          // back with one more attempt and the original expiry. That write goes
          // through only while the marker still names that issuance, and it locks
          // the marker while checking, so a code a resend replaced never comes back,
          // whether or not the newer code was used meanwhile. New codes write marker
          // then code in one transaction, so concurrent sends serialize on the marker
          // and leave the two in agreement. Other verification rows (OAuth state,
          // locks) keep Better Auth's own insert.
          before: async (verification) => {
            const identifier = verification.identifier;
            if (!/^(sign-in|email-verification|change-email|forget-password)-otp-/.test(identifier))
              return;
            const marker = `otp-issued:${identifier}`;
            const codeHash = verification.value.slice(0, verification.value.lastIndexOf(":"));
            const id = verification.id ?? newId("ver");
            if (verification.value.endsWith(":0")) {
              await db.transaction(async (tx) => {
                const issued = await tx.execute<{ expires_at: Date }>(sql`
                  insert into verifications (id, identifier, value, expires_at)
                  values (${newId("ver")}, ${marker}, ${codeHash}, ${verification.expiresAt})
                  on conflict (identifier) do update set
                    value = excluded.value,
                    expires_at = greatest(excluded.expires_at, verifications.expires_at + interval '1 millisecond'),
                    updated_at = now()
                  returning expires_at`);
                const expiresAt = new Date(issued.rows[0].expires_at);
                await tx
                  .insert(schema.verifications)
                  .values({ id, identifier, value: verification.value, expiresAt })
                  .onConflictDoUpdate({
                    target: schema.verifications.identifier,
                    set: { value: verification.value, expiresAt, updatedAt: new Date() },
                  });
              });
            } else {
              await db.execute(sql`
                with current_issue as (
                  select 1 from verifications
                  where identifier = ${marker} and value = ${codeHash} and expires_at = ${verification.expiresAt}
                  for update
                )
                insert into verifications (id, identifier, value, expires_at)
                select ${id}, ${identifier}, ${verification.value}, ${verification.expiresAt}
                from current_issue
                on conflict (identifier) do nothing`);
            }
            return false;
          },
        },
      },
      account: {
        create: {
          before: async (account) => {
            const state = await getOAuthState();
            if (!state?.link) return;
            const intent = state.serverContext?.linkIntent;
            const refuse = (reason: string) => {
              console.warn(`[auth] link refused: ${reason}`);
              return false as const;
            };
            if (!isLinkIntent(intent)) return refuse("no link intent");
            if (intent.userId !== state.link.userId || intent.userId !== account.userId)
              return refuse("intent names another user");
            if (intent.provider !== account.providerId)
              return refuse("intent names another provider");
            if (Date.now() - intent.at > linkIntentMaxAgeMs) return refuse("intent expired");
            const [session] = await db
              .select({
                createdAt: schema.sessions.createdAt,
                expiresAt: schema.sessions.expiresAt,
              })
              .from(schema.sessions)
              .where(
                and(
                  eq(schema.sessions.id, intent.sessionId),
                  eq(schema.sessions.userId, intent.userId),
                ),
              );
            if (!session || session.expiresAt <= new Date()) return refuse("session revoked");
            if (!isFresh(session.createdAt)) return refuse("session not fresh");
          },
        },
      },
    },
    plugins: [
      emailOTP({
        otpLength: 6,
        expiresIn: codeMinutes * 60,
        allowedAttempts: 3,
        storeOTP: "hashed",
        resendStrategy: "rotate",
        changeEmail: { enabled: true, verifyCurrentEmail: false },
        async sendVerificationOTP({ email, otp, type }) {
          await deliver(codeEmail(type, email, otp, env.authUrl, codeMinutes));
        },
      }),
      ...(env.oauthMock && !env.github && !env.google
        ? [genericOAuth({ config: [mockGithub(env.oauthMock), mockGoogle(env.oauthMock)] })]
        : []),
      // Must stay last: it forwards cookies set by auth.api calls inside server functions.
      ...(deps.startCookies === false ? [] : [tanstackStartCookies()]),
    ],
  } satisfies BetterAuthOptions;
  return options;
}

export function createAuth(scope: RequestScope, env: AppEnv, deps: AuthDeps = {}) {
  return betterAuth(authOptions(scope, env, deps));
}

export type Auth = ReturnType<typeof createAuth>;
