// bun run license:keygen <path outside the repo>
//
// Generates the production license signing key offline. Writes the 32-byte seed
// (base64url, the `.convt-dev/license.key` format) to the given path with mode 0600
// and prints only the public key. The seed goes into Leo's password manager and
// becomes the LICENSE_SIGNING_KEY secret on convt-billing; the public key becomes
// LICENSE_PUBLIC_KEY there and CONVT_LICENSE_PUBKEY for release builds (P11).
// Refuses a path inside this checkout, so the seed can never be committed.

import { existsSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";

import { repoRoot } from "@convt/db/env";
import { base64urlEncode, importSigningKey, publicKeyOf } from "@convt/license";

const target = process.argv[2];
if (!target) {
  console.error("usage: bun run license:keygen <path outside the repository>");
  process.exit(2);
}
const path = resolve(target);
if (path === repoRoot || path.startsWith(`${repoRoot}/`)) {
  console.error(`license:keygen: refusing to write inside the checkout (${repoRoot})`);
  process.exit(2);
}
if (existsSync(path)) {
  console.error(`license:keygen: ${path} exists; it is never overwritten`);
  process.exit(2);
}
const seed = crypto.getRandomValues(new Uint8Array(32));
writeFileSync(path, `${base64urlEncode(seed)}\n`, { mode: 0o600, flag: "wx" });
console.log(`seed written to ${path} (mode 0600)`);
console.log(`public key: ${await publicKeyOf(await importSigningKey(seed))}`);
