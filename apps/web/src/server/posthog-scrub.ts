export const scrub = (value: string) => value
  .replace(/\b(Bearer|Token|Basic)\s+[^\s,;]+/gi, "$1 <redacted>")
  .replace(/\b(?:token|api[_-]?key|x-api-key|access[_-]?token|refresh[_-]?token|license[_-]?key|secret|authorization)\s*[:=]\s*[^\s,;]+/gi, "<credential>=<redacted>")
  .replace(/\b(?:cvt|convt)[_-][A-Za-z0-9_-]{8,}\b/gi, "<license-key>")
  .replace(/\b[A-Z0-9]{4}(?:-[A-Z0-9]{4}){3,}\b/g, "<license-key>")
  .replace(/[\w.+-]+@[\w.-]+\.[A-Za-z]{2,}/g, "<email>")
  .replace(/(?:[A-Za-z]:[\\/]|\/)(?:[^\s/\\]+[\\/])*[^\s/\\]+(?:\.[A-Za-z0-9]{1,8})?/g, "<path>");

export function requestAllowsServerExceptions(request: Request): boolean {
  if (request.headers.get("sec-gpc") === "1" || request.headers.get("dnt") === "1") return false;
  return !/(?:^|;\s*)(?:convt:analytics-opt-out|analytics-opt-out)=1(?:;|$)/.test(request.headers.get("cookie") ?? "");
}
