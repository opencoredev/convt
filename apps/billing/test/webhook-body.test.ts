import { expect, test } from "bun:test";
import { readWebhookBody } from "../src/webhook-body";

test("oversized undeclared webhook stops reading and cancels at the hard cap", async () => {
  let pulled = 0;
  let canceled = false;
  const stream = new ReadableStream(
    {
      pull(controller) {
        pulled++;
        controller.enqueue(new Uint8Array(64 * 1024));
        if (pulled === 16) controller.close();
      },
      cancel() {
        canceled = true;
      },
    },
    { highWaterMark: 0 },
  );
  await expect(
    readWebhookBody(
      new Request("https://billing.test/webhooks/polar", { method: "POST", body: stream }),
    ),
  ).rejects.toMatchObject({ status: 413 });
  expect(pulled).toBe(5);
  expect(canceled).toBe(true);
});

test("non-POST webhook is rejected without consuming a body", async () => {
  let pulled = 0;
  const stream = new ReadableStream(
    {
      pull(c) {
        pulled++;
        c.enqueue(new Uint8Array(1));
        c.close();
      },
    },
    { highWaterMark: 0 },
  );
  await expect(
    readWebhookBody(
      new Request("https://billing.test/webhooks/polar", { method: "PUT", body: stream }),
    ),
  ).rejects.toMatchObject({ status: 405 });
  expect(pulled).toBe(0);
});

test("exact-cap webhook preserves raw signature bytes and declared oversize reads nothing", async () => {
  const bytes = new Uint8Array(256 * 1024).fill(0x82);
  expect(
    await readWebhookBody(
      new Request("https://billing.test/webhooks/polar", { method: "POST", body: bytes }),
    ),
  ).toEqual(bytes);
  await expect(
    readWebhookBody(
      new Request("https://billing.test/webhooks/polar", {
        method: "POST",
        headers: { "content-length": "262145" },
      }),
    ),
  ).rejects.toMatchObject({ status: 413 });
});
