import { expect, test } from "bun:test";

import { readEnv } from "../../src/server/env";

const base = { BETTER_AUTH_URL: "https://convt.app", BETTER_AUTH_SECRET: "x".repeat(64) };

test("sales default to Desktop in production and all locally", () => {
  expect(readEnv(base).sales).toBe("desktop");
  expect(readEnv({ ...base, ENV: "development" }).sales).toBe("all");
  expect(readEnv({ ...base, ENV: "test" }).sales).toBe("all");
  expect(readEnv({ ...base, SALES: "all" }).sales).toBe("all");
  expect(() => readEnv({ ...base, SALES: "typo" })).toThrow("SALES");
});
