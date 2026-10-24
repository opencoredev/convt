import { expect, test } from "bun:test";

import { availableProviders, readEnv } from "../../src/server/env";

const base = {
  ENV: "test",
  BETTER_AUTH_URL: "http://localhost:3000",
  BETTER_AUTH_SECRET: "x".repeat(64),
};

test("OAuth availability requires both credentials and exposes only booleans", () => {
  expect(availableProviders(readEnv(base))).toEqual({ github: false, google: false, apple: false });
  expect(availableProviders(readEnv({ ...base, GITHUB_CLIENT_ID: "id" }))).toEqual({
    github: false,
    google: false,
    apple: false,
  });
  expect(
    availableProviders(
      readEnv({ ...base, GITHUB_CLIENT_ID: "id", GITHUB_CLIENT_SECRET: "secret" }),
    ),
  ).toEqual({ github: true, google: false, apple: false });
  expect(
    availableProviders(
      readEnv({ ...base, GOOGLE_CLIENT_ID: "id", GOOGLE_CLIENT_SECRET: "secret" }),
    ),
  ).toEqual({ github: false, google: true, apple: false });
});

test("local mock availability matches auth's all-or-nothing registration", () => {
  const mock = { ...base, OAUTH_MOCK_URL: "http://localhost:4100" };
  expect(availableProviders(readEnv(mock))).toEqual({ github: true, google: true, apple: false });
  expect(
    availableProviders(
      readEnv({ ...mock, GITHUB_CLIENT_ID: "id", GITHUB_CLIENT_SECRET: "secret" }),
    ),
  ).toEqual({ github: true, google: false, apple: false });
});
