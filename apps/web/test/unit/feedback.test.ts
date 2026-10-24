import { describe, expect, test } from "bun:test";

import { readEnv } from "../../src/server/env";
import {
  feedbackEvent,
  feedbackLimits,
  parseFeedback,
  submitFeedback,
  type Feedback,
  type FeedbackError,
} from "../../src/server/feedback";
import { screenFeedback } from "../../src/server/feedback-route";

const base = {
  BETTER_AUTH_URL: "http://localhost:3000",
  BETTER_AUTH_SECRET: "x".repeat(32),
  ENV: "development",
};

const survey = {
  key: "phc_test",
  host: "https://us.i.posthog.com",
  surveyId: "survey-1",
  messageQuestionId: "q-message",
  emailQuestionId: "q-email",
};

const feedback: Feedback = {
  message: "PDF to PNG lost the second page",
  email: "ada@example.com",
  source: "desktop",
  version: "0.2.1",
  platform: "Linux x86_64",
  page: null,
};

describe("readEnv feedback", () => {
  test("is null without a survey id", () => {
    expect(readEnv({ ...base, POSTHOG_KEY: "phc_test" }).feedback).toBeNull();
  });
  test("is null without a PostHog key", () => {
    expect(readEnv({ ...base, POSTHOG_FEEDBACK_SURVEY_ID: "s" }).feedback).toBeNull();
  });
  test("reuses the PostHog key and host", () => {
    const env = readEnv({
      ...base,
      POSTHOG_KEY: "phc_test",
      POSTHOG_HOST: "https://eu.i.posthog.com/",
      POSTHOG_FEEDBACK_SURVEY_ID: "survey-1",
      POSTHOG_FEEDBACK_MESSAGE_QUESTION_ID: "q-message",
    });
    expect(env.feedback).toEqual({
      key: "phc_test",
      host: "https://eu.i.posthog.com",
      surveyId: "survey-1",
      messageQuestionId: "q-message",
      emailQuestionId: null,
    });
  });
});

describe("parseFeedback", () => {
  test("accepts a desktop submission", () => {
    expect(
      parseFeedback({
        source: "desktop",
        message: "  hello  ",
        email: " ada@example.com ",
        version: "0.2.1",
        platform: "macOS 15.1 arm64",
      }),
    ).toEqual({
      ok: true,
      value: {
        message: "hello",
        email: "ada@example.com",
        source: "desktop",
        version: "0.2.1",
        platform: "macOS 15.1 arm64",
        page: null,
      },
    });
  });
  test("treats an empty email as none", () => {
    const parsed = parseFeedback({ source: "web", message: "hi", email: "" });
    expect(parsed.ok && parsed.value.email).toBeNull();
  });
  test("strips the query from the page", () => {
    const parsed = parseFeedback({ source: "web", message: "hi", page: "/device?state=abc#x" });
    expect(parsed.ok && parsed.value.page).toBe("/device");
  });
  test("drops a page that isn't a path", () => {
    const parsed = parseFeedback({ source: "web", message: "hi", page: "https://evil.example/" });
    expect(parsed.ok && parsed.value.page).toBeNull();
  });
  const rejected: [string, unknown, FeedbackError][] = [
    ["no body", null, "invalid_request"],
    ["an array", [], "invalid_request"],
    ["an unknown source", { source: "cli", message: "hi" }, "invalid_request"],
    ["a blank message", { source: "web", message: "   " }, "invalid_message"],
    ["a non-string message", { source: "web", message: 3 }, "invalid_message"],
    [
      "a message over the limit",
      { source: "web", message: "a".repeat(feedbackLimits.message + 1) },
      "invalid_message",
    ],
    ["a bad email", { source: "web", message: "hi", email: "nope" }, "invalid_email"],
    ["a non-string email", { source: "web", message: "hi", email: 1 }, "invalid_email"],
  ];
  for (const [name, input, error] of rejected) {
    test(`rejects ${name}`, () => expect(parseFeedback(input)).toEqual({ ok: false, error }));
  }
});

describe("feedbackEvent", () => {
  test("is a survey sent event with id-keyed and index-keyed responses", () => {
    expect(feedbackEvent(survey, feedback, "sub-1")).toEqual({
      api_key: "phc_test",
      event: "survey sent",
      distinct_id: "feedback_sub-1",
      properties: {
        $survey_id: "survey-1",
        $survey_submission_id: "sub-1",
        $survey_completed: true,
        $survey_response: feedback.message,
        $survey_response_1: "ada@example.com",
        "$survey_response_q-message": feedback.message,
        "$survey_response_q-email": "ada@example.com",
        feedback_source: "desktop",
        app_version: "0.2.1",
        platform: "Linux x86_64",
        $lib: "convt-feedback",
        $process_person_profile: false,
        $geoip_disable: true,
      },
    });
  });
  test("leaves out the email and the id keys it has no value for", () => {
    const event = feedbackEvent(
      { ...survey, messageQuestionId: null, emailQuestionId: null },
      { ...feedback, email: null, version: null, platform: null },
      "sub-2",
    );
    expect(Object.keys(event.properties).filter((k) => k.startsWith("$survey_response"))).toEqual([
      "$survey_response",
    ]);
    expect(event.properties).not.toHaveProperty("app_version");
  });
});

describe("submitFeedback", () => {
  const input = { source: "web", message: "hi" };
  const sent: { url: string; body: string }[] = [];
  const ok = async (url: string, body: string) => {
    sent.push({ url, body });
    return { ok: true, status: 200 };
  };
  const run = (overrides: Partial<Parameters<typeof submitFeedback>[0]> = {}) =>
    submitFeedback({
      survey,
      input,
      consume: async () => 1,
      send: ok,
      submissionId: "sub-3",
      ...overrides,
    });

  test("sends one event to the capture endpoint", async () => {
    sent.length = 0;
    expect(await run()).toEqual({ status: 200, body: { ok: true } });
    expect(sent).toHaveLength(1);
    expect(sent[0].url).toBe("https://us.i.posthog.com/i/v0/e/");
    expect(JSON.parse(sent[0].body).properties.$survey_response).toBe("hi");
  });
  test("503 when no survey is configured", async () => {
    expect((await run({ survey: null })).status).toBe(503);
  });
  test("400 before counting a bad request", async () => {
    let counted = false;
    const r = await run({
      input: { source: "web" },
      consume: async () => {
        counted = true;
        return 1;
      },
    });
    expect(r.status).toBe(400);
    expect(counted).toBe(false);
  });
  test("429 over the hourly limit", async () => {
    sent.length = 0;
    expect((await run({ consume: async () => feedbackLimits.perIpPerHour + 1 })).status).toBe(429);
    expect(sent).toHaveLength(0);
  });
  test("502 when PostHog refuses or is unreachable", async () => {
    expect((await run({ send: async () => ({ ok: false, status: 500 }) })).status).toBe(502);
    expect(
      (
        await run({
          send: async () => {
            throw new Error("offline");
          },
        })
      ).status,
    ).toBe(502);
  });
});

test("control characters in labels become spaces", () => {
  const parsed = parseFeedback({
    source: "desktop",
    message: "hi",
    platform: "Linux\nx86_64\u0000",
  });
  expect(parsed.ok && parsed.value.platform).toBe("Linux x86_64");
});

describe("screenFeedback", () => {
  const post = (url: string, headers: Record<string, string>, body = '{"message":"hi"}') =>
    new Request(url, {
      method: "POST",
      headers: { "content-type": "application/json", ...headers },
      body,
    });

  test("refuses a browser posting from another site", async () => {
    const r = await screenFeedback(
      post("https://convt.app/api/feedback", { origin: "https://evil.example" }),
    );
    expect(r).toEqual({ status: 403, body: { error: "forbidden" } });
  });

  test("accepts a browser on either host the site serves", async () => {
    for (const host of ["https://convt.app", "https://www.convt.app"]) {
      const r = await screenFeedback(post(`${host}/api/feedback`, { origin: host }));
      expect(r).toEqual({ value: { message: "hi" } });
    }
  });

  test("refuses www posting to the apex, which a browser never does", async () => {
    const r = await screenFeedback(
      post("https://convt.app/api/feedback", { origin: "https://www.convt.app" }),
    );
    expect(r).toEqual({ status: 403, body: { error: "forbidden" } });
  });

  test("accepts the desktop app, which sends no Origin", async () => {
    const r = await screenFeedback(post("https://convt.app/api/feedback", {}));
    expect(r).toEqual({ value: { message: "hi" } });
  });

  test("refuses a body over the size cap", async () => {
    const big = JSON.stringify({ message: "x".repeat(feedbackLimits.body) });
    const r = await screenFeedback(post("https://convt.app/api/feedback", {}, big));
    expect(r).toEqual({ status: 413, body: { error: "too_large" } });
  });
});
