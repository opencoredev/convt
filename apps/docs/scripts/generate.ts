import { apiBase } from "../api-host.ts";

// Overlay that puts the shared host on the reference and Try it panel.
const servers = `overlay: 1.1.0
info:
  title: convt servers
  version: 1.0.0
# Generated from api-host.ts. Do not edit.
actions:
  - target: $.servers
    remove: true
  - target: $
    update:
      servers:
        - url: ${apiBase}
          description: Production API
`;

await Bun.write(new URL("../openapi/servers.yaml", import.meta.url), servers);
await import("./generate-formats.ts");
