import { expect, test } from "bun:test";
import type { CaptureResult } from "posthog-js";

import { posthogOptions } from "../../src/components/posthog-provider";
import { sanitizeEvent, stripQuery } from "../../src/lib/analytics-sanitize";

// Shaped like the events posthog-js builds for a visitor who lands on the checkout
// result from Polar, then opens the desktop sign-in page and clicks a link.
const SECRETS = ["chk_secret123", "state-abc", "challenge-xyz", "tok_789", "frag"];

const pageview: CaptureResult = {
  uuid: "0192a8c0-0000-7000-8000-000000000001",
  event: "$pageview",
  properties: {
    $current_url: "https://convt.app/checkout/success?checkout_id=chk_secret123#frag",
    $host: "convt.app",
    $pathname: "/checkout/success",
    $referrer: "https://sandbox.polar.sh/checkout/x?token=tok_789",
    $referring_domain: "sandbox.polar.sh",
    $initial_current_url: "https://convt.app/device?state=state-abc&challenge=challenge-xyz",
    $initial_referrer: "$direct",
    $session_entry_url: "https://convt.app/device?state=state-abc&challenge=challenge-xyz",
    $prev_pageview_pathname: "/device",
    $browser: "Firefox",
    $screen_width: 1440,
    token: "phc_public_project_key",
  },
  $set_once: {
    $initial_current_url: "https://convt.app/device?state=state-abc&challenge=challenge-xyz",
    $initial_referrer: "https://sandbox.polar.sh/checkout/x?token=tok_789",
    $initial_pathname: "/device",
  },
};

const autocapture: CaptureResult = {
  uuid: "0192a8c0-0000-7000-8000-000000000002",
  event: "$autocapture",
  properties: {
    $current_url: "https://convt.app/device?state=state-abc&challenge=challenge-xyz",
    $event_type: "click",
    $external_click_url: "https://example.com/next?token=tok_789",
    $elements_chain:
      'a.link:attr__href="/device?state=state-abc&challenge=challenge-xyz"href="/device?state=state-abc&challenge=challenge-xyz"nth-child="1"text="Continue";main:nth-child="1"',
    $elements: [{ tag_name: "a", attr__href: "/checkout/success?checkout_id=chk_secret123" }],
  },
};

// Heatmaps key their points by the page URL.
const heatmap: CaptureResult = {
  uuid: "0192a8c0-0000-7000-8000-000000000003",
  event: "$$heatmap",
  properties: {
    $current_url: "https://convt.app/device?state=state-abc&challenge=challenge-xyz",
    $heatmap_data: {
      "https://convt.app/device?state=state-abc&challenge=challenge-xyz": [{ x: 1, y: 2 }],
    },
  },
};

test("no captured URL keeps a query string, fragment or sensitive value", () => {
  for (const event of [pageview, autocapture, heatmap]) {
    const json = JSON.stringify(sanitizeEvent(event));
    for (const secret of SECRETS) expect(json).not.toContain(secret);
    expect(json).not.toMatch(/https?:\/\/[^"]*[?#]/);
  }
});

test("origin and pathname survive, and non-URL properties are untouched", () => {
  const out = sanitizeEvent(pageview)!;
  expect(out.properties).toEqual({
    ...pageview.properties,
    $current_url: "https://convt.app/checkout/success",
    $referrer: "https://sandbox.polar.sh/checkout/x",
    $initial_current_url: "https://convt.app/device",
    $session_entry_url: "https://convt.app/device",
  });
  expect(out.$set_once).toEqual({
    $initial_current_url: "https://convt.app/device",
    $initial_referrer: "https://sandbox.polar.sh/checkout/x",
    $initial_pathname: "/device",
  });
  expect(out.uuid).toBe(pageview.uuid);
  expect(out.event).toBe("$pageview");

  const click = sanitizeEvent(autocapture)!.properties;
  expect(click.$elements_chain).toBe(
    'a.link:attr__href="/device"href="/device"nth-child="1"text="Continue";main:nth-child="1"',
  );
  expect(click.$elements).toEqual([{ tag_name: "a", attr__href: "/checkout/success" }]);
});

test("stripQuery handles paths, bare URLs and dropped events", () => {
  expect(stripQuery("https://convt.app/")).toBe("https://convt.app/");
  expect(stripQuery("/device?state=x")).toBe("/device");
  expect(stripQuery("$direct")).toBe("$direct");
  expect(sanitizeEvent(null)).toBeNull();
});

test("heatmap data is keyed by the page without its query string", () => {
  expect(sanitizeEvent(heatmap)!.properties.$heatmap_data).toEqual({
    "https://convt.app/device": [{ x: 1, y: 2 }],
  });
});

test("PostHog runs without the channels that bypass before_send", () => {
  const options = posthogOptions("https://us.i.posthog.com");
  // The /flags request sends the raw first-visit URL outside before_send.
  expect(options.advanced_disable_flags).toBe(true);
  expect(options.disable_session_recording).toBe(true);
  expect(options.capture_heatmaps).toBe(false);
  expect(options.api_host).toBe("https://us.i.posthog.com");
});
