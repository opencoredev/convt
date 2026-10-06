// Keep the public reference byte-for-byte identical to the server's source spec.
const server = new URL("../../../crates/convt-server/openapi.json", import.meta.url);
const destination = new URL("../../../apps/web/src/generated/openapi.json", import.meta.url);
const spec = await Bun.file(server).json();
if (spec.openapi !== "3.1.0" || !spec.paths["/v1/jobs"])
  throw new Error("Invalid convt OpenAPI source");
await Bun.write(destination, await Bun.file(server).text());
