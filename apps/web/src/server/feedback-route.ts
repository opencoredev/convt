// The /api/feedback handler. The desktop app posts without cookies or Origin; a
// browser always sends Origin on a POST, and it must be this site's.

import { consumeSendBucket } from "@convt/db";

import { requestContext } from "./context";
import { readSmallJson } from "./device-auth";
import { feedbackLimits, submitFeedback, type FeedbackResponse } from "./feedback";

const hourMs = 60 * 60 * 1000;

function respond(r: FeedbackResponse): Response {
  return Response.json(r.body, { status: r.status, headers: { "cache-control": "no-store" } });
}

export async function handleFeedback(request: Request, context: unknown): Promise<Response> {
  const { scope, appEnv } = requestContext(context);
  const origin = request.headers.get("origin");
  if (origin !== null && origin !== new URL(appEnv.authUrl).origin)
    return respond({ status: 403, body: { error: "forbidden" } });
  const body = await readSmallJson(request, feedbackLimits.body);
  if (body === "too_large") return respond({ status: 413, body: { error: "too_large" } });
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
        }),
      submissionId: crypto.randomUUID(),
    }),
  );
}
