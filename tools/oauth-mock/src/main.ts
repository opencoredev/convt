// bun tools/oauth-mock/src/main.ts. scripts/dev-web.sh starts it with MOCK_PORT
// from .convt-dev/services.env. MOCK_HOST defaults to 127.0.0.1; MOCK_PUBLIC_URL
// is the address a browser uses, when that differs (a tailnet preview).

import { createMock } from "./mock";

const host = process.env.MOCK_HOST ?? "127.0.0.1";
const port = Number(process.env.MOCK_PORT ?? 0);
const server = Bun.serve({
  hostname: host,
  port,
  fetch: () => new Response("starting", { status: 503 }),
});
const internalUrl = `http://${host}:${server.port}`;
const mock = await createMock({ internalUrl, publicUrl: process.env.MOCK_PUBLIC_URL || undefined });
server.reload({ fetch: mock.fetch });
console.log(`oauth-mock: listening on ${internalUrl}`);
