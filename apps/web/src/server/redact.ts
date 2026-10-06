// Every server log line goes through here. Sign-in codes must never reach logs:
// values of fields named `otp` or `code` are replaced, and in auth paths any run of
// six digits is masked too.

const secretKeys = new Set(["otp", "code", "token", "password", "secret"]);
const sixDigits = /(?<!\d)\d{6}(?!\d)/g;

export function redactText(text: string, authPath = true): string {
  let out = text
    .replace(/([?&#](?:otp|code|token)=)[^&#\s"]*/gi, "$1[redacted]")
    .replace(/("(?:otp|code|token)"\s*:\s*)"[^"]*"/gi, '$1"[redacted]"');
  if (authPath) out = out.replace(sixDigits, "[redacted]");
  return out;
}

export function redact(value: unknown, authPath = true, depth = 0): unknown {
  if (typeof value === "string") return redactText(value, authPath);
  if (depth > 6 || value === null || typeof value !== "object") return value;
  if (value instanceof Error) {
    const copy = new Error(redactText(value.message, authPath));
    copy.name = value.name;
    return copy;
  }
  if (Array.isArray(value)) return value.map((v) => redact(v, authPath, depth + 1));
  return Object.fromEntries(
    Object.entries(value).map(([k, v]) => [
      k,
      secretKeys.has(k.toLowerCase()) ? "[redacted]" : redact(v, authPath, depth + 1),
    ]),
  );
}

type Level = "debug" | "info" | "warn" | "error";

/** Better Auth's `logger.log`: drops nothing but redacts everything. */
export function redactingLog(level: Level, message: string, ...args: unknown[]) {
  const write =
    level === "debug"
      ? console.debug
      : level === "info"
        ? console.info
        : level === "warn"
          ? console.warn
          : console.error;
  write(`[auth] ${redactText(message)}`, ...args.map((a) => redact(a)));
}
