// bun tools/billing-mock/src/main.ts. scripts/dev-web.sh starts it with the port,
// webhook secret and URLs from .convt-dev/services.env. It binds loopback only:
// anything else is refused, because the mock signs webhooks that convt-billing
// trusts and accepts any card.

import { createBillingMock } from "./mock";

const host = process.env.MOCK_HOST ?? "127.0.0.1";
if (!["127.0.0.1", "localhost", "::1"].includes(host)) {
  console.error(`billing-mock: refusing to bind ${host}; only loopback is allowed`);
  process.exit(2);
}
const need = (name: string) => {
  const v = process.env[name];
  if (!v) {
    console.error(`billing-mock: ${name} is not set`);
    process.exit(2);
  }
  return v;
};
const port = Number(process.env.MOCK_PORT ?? 0);
const server = Bun.serve({
  hostname: host,
  port,
  fetch: () => new Response("starting", { status: 503 }),
});
const internalUrl = `http://${host === "::1" ? "[::1]" : host}:${server.port}`;
const mock = createBillingMock({
  publicUrl: process.env.MOCK_PUBLIC_URL || internalUrl,
  accessToken: need("POLAR_ACCESS_TOKEN"),
  resendApiKey: need("RESEND_API_KEY"),
  webhook: {
    secret: need("POLAR_WEBHOOK_SECRET"),
    url: process.env.WEBHOOK_URL || null,
    scheme: process.env.WEBHOOK_SCHEME === "legacy" ? "legacy" : "standard",
    retryBaseMs: Number(process.env.WEBHOOK_RETRY_BASE_MS ?? 1000),
  },
  mailpitUrl: process.env.MAILPIT_URL || undefined,
});
server.reload({ fetch: mock.fetch });
console.log(`billing-mock: listening on ${internalUrl}`);
