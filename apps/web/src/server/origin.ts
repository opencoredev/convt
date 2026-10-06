// CSRF protection for server functions and pages. Better Auth checks Origin on
// its own endpoints; everything else that changes state goes through here.

export function isSameOriginRequest(request: Request, siteOrigin: string): boolean {
  if (request.method === "GET" || request.method === "HEAD") return true;
  const origin = request.headers.get("origin");
  if (origin !== null) return origin === siteOrigin;
  return request.headers.get("sec-fetch-site") === "same-origin";
}
