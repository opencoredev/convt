# @convt/sdk

The TypeScript client for the [convt API](https://convt.app/docs): convert images, video, audio, documents and PDFs from your own code. Each successful conversion costs $0.01. Failed and cancelled ones are free.

```sh
npm install @convt/sdk
```

```ts
import { Convt } from "@convt/sdk";

const convt = new Convt(); // reads CONVT_API_KEY
const out = await convt.convert("report.docx", { to: "pdf" });
await out.save("report.pdf");
```

Create an API key on the [dashboard](https://convt.app/dashboard/api). Keep it on your server; never ship it to a browser.

## What it does

- `convert(input, { to })` takes a file path (Node, Bun, Deno), a `Blob` or `File`, or bytes, up to 2 GB. It reserves a job, uploads straight to storage, starts it, polls and returns a `Conversion` with `save()`, `saveAll()`, `blob()` and signed `outputs`.
- `convertUrl(url, { to })` converts a file convt downloads from a public https URL, such as a presigned S3 or R2 link, so nothing is uploaded from your machine.
- Every conversion is created with an `Idempotency-Key`, so a retry within 24 hours (while the job exists) never reserves or bills twice; after that the key is free and a replay starts a new job. Rate limits (`429`) and gateway errors are retried with backoff, honoring `Retry-After`. Pass your own `idempotencyKey` (an order id, say) to resume after a crash; it never cancels that job for you. The SDK sends every file's SHA-256 with the job, so a retry under the same key with different bytes is refused.
- Abort with `signal` or `timeoutMs` and the SDK cancels the job, which releases its reservation. Cancellation is best effort: a job that already succeeded is billed, and the error then says so and carries `jobId`. With your own `idempotencyKey` the SDK stops waiting instead and leaves cancelling to you (`convt.cancel(error.jobId)`), since another call may share that job.
- Errors are `ConvtError` with a stable `code`, the HTTP `status`, the `requestId` to quote to support and, once a job exists, its `jobId` and `idempotencyKey`.
- A keyed create whose answer was lost, unreadable or a `5xx` throws `create_outcome_unknown` with `idempotencyKey` and no `jobId`: retry with that key to get the job if it exists, without reserving twice. The underlying error (with `status` and `requestId`) is `error.cause`.

```ts
const out = await convt.convert(file, {
  to: "webp",
  options: { quality: 80, max_size: 2048 },
  onProgress: (job) => console.log(job.status),
});
```

## Webhooks

Pass `webhookUrl` and convt POSTs a signed event when the job finishes. Verify it with the raw body:

```ts
import { verifyWebhook } from "@convt/sdk";

const { secret } = await convt.webhookSecret(); // store this once
const event = await verifyWebhook({
  body: await request.text(),
  signature: request.headers.get("convt-signature"),
  secret,
});
if (event.type === "job.succeeded") console.log(event.data.outputs);
```

## Lower-level calls

`create`, `start`, `status`, `cancel` and `download` map one to one onto the [REST routes](https://convt.app/docs/api). `formats` (also at `@convt/sdk/formats`) lists every format id.

## Requirements

Node 20.3 or newer, or Bun: both stream files from disk. Other runtimes read a file passed by path into memory. Run the SDK on your server; a `cvt_live_` key never belongs in browser code.

## License

AGPL-3.0-only, like the rest of [convt](https://github.com/opencoredev/convt).
