// Local Polar-compatible receipt sink. Point only owned test workers at this.
const events = new Map<string, unknown>();
let duplicate = 0;
const server = Bun.serve({
  hostname: "127.0.0.1",
  port: Number(process.env.LOAD_METER_PORT ?? 0),
  async fetch(request) {
    const url = new URL(request.url);
    if (url.pathname === "/state" && request.method === "GET")
      return Response.json({ events: [...events.values()], duplicate });
    if (url.pathname === "/v1/events/ingest" && request.method === "POST") {
      const body = (await request.json()) as { events: { external_id: string }[] };
      let inserted = 0;
      let duplicates = 0;
      for (const event of body.events) {
        if (events.has(event.external_id)) {
          duplicates++;
          duplicate++;
        } else {
          events.set(event.external_id, event);
          inserted++;
        }
      }
      return Response.json({ inserted, duplicates });
    }
    return new Response("Not found", { status: 404 });
  },
});
console.log(server.url.href);
