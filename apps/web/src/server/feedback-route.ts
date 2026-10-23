// The /api/feedback handler. The desktop app posts without cookies or Origin; a
// browser always sends Origin on a POST, and it must be the host it posted to
// (convt.app and www.convt.app both serve the site).

import { consumeSendBucket } from "@convt/db";

import { requestContext } from "./context";
import { readSmallJson } from "./device-auth";
import { feedbackLimits, submitFeedback, type FeedbackResponse } from "./feedback";

const hourMs = 60 * 60 * 1000;

function respond(r: FeedbackResponse): Response {
  return Response.json(r.body, { status: r.status, headers: { "cache-control": "no-store" } });
}

/** How long PostHog gets to accept a submission before the form says try again. */
const sendTimeoutMs = 5000;

/**
 * The checks before anything else runs: a browser's Origin must be the host it
 * posted to, and the body must fit the size cap. Returns the parsed body, or
 * the response that ends the request.
 */
export async function screenFeedback(
  request: Request,
): Promise<{ value: unknown } | null | FeedbackResponse> {
  const origin = request.headers.get("origin");
  if (origin !== null && origin !== new URL(request.url).origin)
    return { status: 403, body: { error: "forbidden" } };
  const body = await readSmallJson(request, feedbackLimits.body);
  if (body === "too_large") return { status: 413, body: { error: "too_large" } };
  return body;
}

export async function handleFeedback(request: Request, context: unknown): Promise<Response> {
  const { scope, appEnv } = requestContext(context);
  const body = await screenFeedback(request);
  if (body !== null && "status" in body) return respond(body);
  const ip = request.headers.get("cf-connecting-ip")?.trim() || "unknown";
  return respond(
    await submitFeedback({
      survey: appEnv.feedback,
      input: body?.value ?? null,
      consume: () => consumeSendBucket(scope.db, `feedback:ip:${ip}`, hourMs, new Date()),
      send: (url, payload) =>
        fetch(url, {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: payload,
          signal: AbortSignal.timeout(sendTimeoutMs),
        }),
      submissionId: crypto.randomUUID(),
    }),
  );
}
