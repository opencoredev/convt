// Browser calls to Better Auth's endpoints under /api/auth. Plain fetch: the
// session cookie is HttpOnly and same-origin, and Better Auth checks Origin.

export type AuthResult<T = unknown> =
  | { ok: true; data: T }
  | { ok: false; status: number; code: string; message: string };

const messages: Record<string, string> = {
  INVALID_OTP: "That code isn't right. Check the email and try again.",
  OTP_EXPIRED: "That code has expired. Send a new one.",
  TOO_MANY_ATTEMPTS: "Too many wrong tries for that code. Send a new one.",
  TOO_MANY_CODES: "Too many codes were sent. Wait a few minutes and try again.",
  TOO_MANY_REQUESTS: "Too many tries. Wait a minute and try again.",
  SESSION_NOT_FRESH: "Sign in again to continue.",
  INVALID_EMAIL: "Enter a valid email address.",
  ACCOUNT_NOT_FOUND: "That sign-in method isn't connected.",
  VERIFY_FROM_ACCOUNT: "Sign in with an email code instead.",
  ACCOUNT_CHANGED: "Something changed while you were signing in. Send a new code.",
};

/** Text for an error code from an API response or a `?error=` redirect. */
export function authErrorMessage(code: string | undefined | null): string {
  if (!code) return "Something went wrong. Try again.";
  const key = code.toUpperCase();
  if (messages[key]) return messages[key];
  if (key === "ACCOUNT_NOT_LINKED")
    return "That account isn't connected to convt yet. Sign in with an email code, then connect it in Settings.";
  if (key === "EMAIL_NOT_FOUND")
    return "That account has no email address we can use. Sign in with an email code instead.";
  if (key === "ACCESS_DENIED") return "Sign-in was cancelled.";
  if (key.includes("STATE")) return "That sign-in link expired. Start again.";
  if (key === "UNABLE_TO_LINK_ACCOUNT" || key === "ACCOUNT_ALREADY_LINKED_TO_DIFFERENT_USER")
    return "That account couldn't be connected. It may belong to another convt account.";
  return "Something went wrong. Try again.";
}

async function post<T>(path: string, body: unknown): Promise<AuthResult<T>> {
  let res: Response;
  try {
    res = await fetch(`/api/auth${path}`, {
      method: "POST",
      credentials: "same-origin",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(body),
    });
  } catch {
    return {
      ok: false,
      status: 0,
      code: "NETWORK",
      message: "Can't reach convt. Check your connection.",
    };
  }
  const data = (await res.json().catch(() => null)) as
    | (T & { code?: string; message?: string })
    | null;
  if (res.ok) return { ok: true, data: data as T };
  const code =
    res.status === 429 && !data?.code ? "TOO_MANY_REQUESTS" : (data?.code ?? `HTTP_${res.status}`);
  return { ok: false, status: res.status, code, message: authErrorMessage(code) };
}

export const sendSignInCode = (email: string) =>
  post("/email-otp/send-verification-otp", { email: email.trim(), type: "sign-in" });

export const signInWithCode = (email: string, otp: string) =>
  post("/sign-in/email-otp", { email: email.trim(), otp });

export const sendVerificationCode = (email: string) =>
  post("/email-otp/send-verification-otp", { email, type: "email-verification" });

export const verifyEmail = (email: string, otp: string) =>
  post("/email-otp/verify-email", { email, otp });

export const requestEmailChange = (newEmail: string) =>
  post("/email-otp/request-email-change", { newEmail: newEmail.trim() });

export const changeEmail = (newEmail: string, otp: string) =>
  post("/email-otp/change-email", { newEmail: newEmail.trim(), otp });

export const unlinkAccount = (accountId: string) => post("/unlink-account", { accountId });

export const signOut = () => post("/sign-out", {});

export type SocialProvider = "github" | "google";

/** Starts GitHub or Google sign-in and leaves the page for the provider. */
export async function startSocialSignIn(
  provider: SocialProvider,
  redirectTo: string,
): Promise<AuthResult> {
  const result = await post<{ url: string }>("/sign-in/social", {
    provider,
    callbackURL: redirectTo,
    errorCallbackURL: "/sign-in",
    disableRedirect: true,
  });
  if (result.ok) window.location.assign(result.data.url);
  return result;
}

/** Connects GitHub or Google to the signed-in account from settings. */
export async function linkSocial(provider: SocialProvider): Promise<AuthResult> {
  const result = await post<{ url: string }>("/link-social", {
    provider,
    callbackURL: "/account",
    errorCallbackURL: "/account",
    disableRedirect: true,
  });
  if (result.ok) window.location.assign(result.data.url);
  return result;
}
