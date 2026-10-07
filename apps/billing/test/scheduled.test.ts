import { expect, mock, test } from "bun:test";

// Bun has no WorkerEntrypoint; the refused route must never reach Worker setup.
mock.module("cloudflare:workers", () => ({ WorkerEntrypoint: class {} }));
const { default: worker } = await import("../src/index");

for (const env of ["production", "staging", "test", undefined]) {
  test(`manual cron is 404 for ENV=${env}, even without billing secrets`, async () => {
    const response = await worker.fetch(
      new Request("https://billing.test/__billing/scheduled?cron=*+*+*+*+*"),
      { ENV: env, HYPERDRIVE_BILLING: { connectionString: "unused" } },
      {
        waitUntil: () => {
          throw new Error("must not schedule work");
        },
      },
    );
    expect(response.status).toBe(404);
  });
}
