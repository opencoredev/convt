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
JS
args=()
[[ $environment == staging ]] && args=(--env staging)
if [[ $worker == billing ]]; then
  test -s "$ROOT/.convt-dev/license.pub" || {
    echo 'deploy: .convt-dev/license.pub is missing; the dev-key guard needs it' >&2
    exit 1
  }
  wrangler deploy "${args[@]}" --var "DEV_LICENSE_PUBKEYS:$(cat "$ROOT/.convt-dev/license.pub")"
else
  if [[ $environment == staging ]]; then CLOUDFLARE_ENV=staging bun run build; else bun run build; fi
  wrangler deploy "${args[@]}"
fi
