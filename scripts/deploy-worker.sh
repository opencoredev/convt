#!/usr/bin/env bash
# Guarded Worker deployment. The staging build must use staging bindings too.
set -euo pipefail
ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
worker=${1:?web or billing required}
shift
environment=production
if [[ ${1:-} == --env && ${2:-} == staging ]]; then
  environment=staging
  shift 2
fi
[[ $# == 0 && ( $worker == web || $worker == billing ) ]] || {
  echo 'deploy: use web|billing [--env staging]' >&2
  exit 1
}
cd "$ROOT/apps/$worker"
# Refuse placeholder origins, ids and database bindings before any network call.
bun - "$environment" <<'JS'
import { parse } from "jsonc-parser";
const raw = parse(await Bun.file("wrangler.jsonc").text());
const env = process.argv[2] === "staging" ? raw.env.staging : raw;
if (env.hyperdrive.some(h => /^0{30}/.test(h.id)) ||
    Object.values(env.vars).some(v => typeof v === "string" &&
      (v.includes("<subdomain>") || v === "SET_IN_WORKER_ENV"))) {
  console.error("deploy: replace the environment's placeholder Hyperdrive ids, origins or SET_IN_WORKER_ENV vars first");
  process.exit(1);
}
// Production convt.app sells cloud conversion: without the API address the
// dashboard converter and the desktop app's cloud jobs answer "not configured".
const httpsOrigin = (value) => {
  try {
    const url = new URL(value);
    return url.protocol === "https:" && url.origin === value;
  } catch {
    return false;
  }
};
if (process.argv[2] === "production" && raw.name === "convt-web" &&
    !httpsOrigin(env.vars.CONVT_API_URL ?? "")) {
  console.error("deploy: set vars.CONVT_API_URL to the cloud API's https origin first");
  process.exit(1);
}
JS
args=()
[[ $environment == staging ]] && args=(--env staging)
if [[ $worker == billing ]]; then
  # The catalog this deploy sells from must have every provider id filled in
  # (CNV-101 leaves PLACEHOLDER ids until the Polar products exist).
  catalog=production
  [[ $environment == staging ]] && catalog=sandbox
  (cd "$ROOT/packages/billing" && bun -e '
import { loadCatalog, placeholderIds } from "./src/catalog";
const ids = placeholderIds(loadCatalog(process.argv[1]));
if (ids.length) {
  console.error(`deploy: the ${process.argv[1]} catalog still has placeholder ids: ${ids.join(", ")}`);
  process.exit(1);
}' "$catalog")
  test -s "$ROOT/.convt-dev/license.pub" || {
    echo 'deploy: .convt-dev/license.pub is missing; the dev-key guard needs it' >&2
    exit 1
  }
  wrangler deploy "${args[@]}" --var "DEV_LICENSE_PUBKEYS:$(cat "$ROOT/.convt-dev/license.pub")"
else
  if [[ $environment == staging ]]; then CLOUDFLARE_ENV=staging bun run build; else bun run build; fi
  wrangler deploy "${args[@]}"
fi
