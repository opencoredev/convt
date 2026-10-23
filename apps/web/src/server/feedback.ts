// POST /api/feedback: the feedback form on convt.app and in the desktop app.
// Each submission becomes one PostHog `survey sent` event for the API survey in
// POSTHOG_FEEDBACK_SURVEY_ID. The Worker sends it, not posthog-js, so feedback
// arrives when the browser blocks PostHog or opted out of analytics: sending
// the form is the consent. The event carries no IP (GeoIP off) and creates no
// person profile; the email is there only when the user typed one.

import { stripQuery } from "#/lib/analytics-sanitize";

import type { FeedbackSurvey } from "./env";

export const feedbackLimits = {
  /** Characters in the message. The form counts the same. */
  message: 4000,
  /** Submissions per IP per hour. */
  perIpPerHour: 10,
  /** The largest JSON body read. */
  body: 32 * 1024,
};

export type FeedbackSource = "web" | "desktop";

export type Feedback = {
  message: string;
  email: string | null;
  source: FeedbackSource;
  /** App version for desktop, or null. */
  version: string | null;
  /** "macOS 15.1 arm64", "Linux x86_64", a browser platform, or null. */
  platform: string | null;
  /** The web page path the form was opened on, without its query. */
  page: string | null;
};

export type FeedbackError = "invalid_message" | "invalid_email" | "invalid_request";

const EMAIL = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;

function shortText(value: unknown, max: number): string | null {
  if (typeof value !== "string") return null;
  // Control characters become spaces: these values are one-line labels.
  const text = Array.from(value, (ch) => (/\P{Cc}/u.test(ch) ? ch : " "))
    .join("")
    .trim();
  return text === "" ? null : text.slice(0, max);
}

/** Parses the request body. Unknown fields are ignored. */
export function parseFeedback(
  input: unknown,
): { ok: true; value: Feedback } | { ok: false; error: FeedbackError } {
  if (input === null || typeof input !== "object" || Array.isArray(input))
    return { ok: false, error: "invalid_request" };
  const body: Record<string, unknown> = { ...input };
  const source = body.source;
  if (source !== "web" && source !== "desktop") return { ok: false, error: "invalid_request" };
  const message = typeof body.message === "string" ? body.message.trim() : "";
  if (message === "" || [...message].length > feedbackLimits.message)
    return { ok: false, error: "invalid_message" };
  let email: string | null = null;
  if (body.email !== undefined && body.email !== null && body.email !== "") {
    if (typeof body.email !== "string") return { ok: false, error: "invalid_email" };
    const trimmed = body.email.trim();
    if (trimmed.length > 254 || !EMAIL.test(trimmed)) return { ok: false, error: "invalid_email" };
    email = trimmed;
  }
  const page = shortText(body.page, 200);
  return {
    ok: true,
    value: {
      message,
      email,
      source,
      version: shortText(body.version, 32),
      platform: shortText(body.platform, 80),
      page: page?.startsWith("/") ? stripQuery(page) : null,
    },
  };
}

/**
 * The capture request body for PostHog's `/i/v0/e/` endpoint. Responses go under the
 * question-id keys PostHog recommends when the ids are configured, and always under
 * the legacy index keys (`$survey_response`, `$survey_response_1`) as a fallback.
 */
/** One event for PostHog's capture API. */
export type CaptureEvent = {
  api_key: string;
  event: string;
  distinct_id: string;
  properties: Record<string, string | boolean>;
};

export function feedbackEvent(
  survey: FeedbackSurvey,
  feedback: Feedback,
  submissionId: string,
): CaptureEvent {
  const responses: Record<string, string> = { $survey_response: feedback.message };
  if (survey.messageQuestionId)
    responses[`$survey_response_${survey.messageQuestionId}`] = feedback.message;
  if (feedback.email) {
    responses.$survey_response_1 = feedback.email;
    if (survey.emailQuestionId)
      responses[`$survey_response_${survey.emailQuestionId}`] = feedback.email;
  }
  return {
    api_key: survey.key,
    event: "survey sent",
    // Anonymous: one id per submission, never the browser's PostHog id.
    distinct_id: `feedback_${submissionId}`,
    properties: {
      $survey_id: survey.surveyId,
      $survey_submission_id: submissionId,
      $survey_completed: true,
      ...responses,
      feedback_source: feedback.source,
      ...(feedback.version ? { app_version: feedback.version } : {}),
      ...(feedback.platform ? { platform: feedback.platform } : {}),
      ...(feedback.page ? { page: feedback.page } : {}),
      $lib: "convt-feedback",
      $process_person_profile: false,
      $geoip_disable: true,
    },
  };
}

export type FeedbackResponse = { status: number; body: { ok: true } | { error: string } };

/**
 * Validates, rate limits and forwards one submission. `consume` counts this IP's
 * submissions in the current hour; `send` posts the event to PostHog.
 */
export async function submitFeedback({
  survey,
  input,
  consume,
  send,
  submissionId,
}: {
  survey: FeedbackSurvey | null;
  input: unknown;
  consume: () => Promise<number>;
  send: (url: string, body: string) => Promise<{ ok: boolean; status: number }>;
  submissionId: string;
}): Promise<FeedbackResponse> {
  if (!survey) return { status: 503, body: { error: "unavailable" } };
  const parsed = parseFeedback(input);
  if (!parsed.ok) return { status: 400, body: { error: parsed.error } };
  if ((await consume()) > feedbackLimits.perIpPerHour)
    return { status: 429, body: { error: "rate_limited" } };
  const event = feedbackEvent(survey, parsed.value, submissionId);
  const result = await send(`${survey.host}/i/v0/e/`, JSON.stringify(event)).catch(() => null);
  if (!result?.ok) {
    console.error("[feedback] PostHog did not accept the event", result?.status ?? "network");
    return { status: 502, body: { error: "upstream" } };
  }
  return { status: 200, body: { ok: true } };
}
