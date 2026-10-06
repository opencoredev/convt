#!/usr/bin/env bash
# Per-checkout MinIO. Only these labeled resources are eligible for removal.
set -euo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd -P)
hash=$(printf '%s' "$root" | sha256sum | cut -c1-12)
name=convt-s3-$hash
file=$root/.convt-dev/p9-storage.env
owned() { [[ $(docker inspect --format '{{index .Config.Labels "convt.checkout"}}' "$name") == "$root" ]] || { echo 'ownership guard refused' >&2; exit 1; }; }
case ${1:-up} in
  down) if docker inspect "$name" >/dev/null 2>&1; then owned; docker rm -f "$name" >/dev/null; fi; exit ;;
  env) cat "$file"; exit ;;
  up) ;;
  *) echo 'usage: storage-dev.sh up|down|env' >&2; exit 1 ;;
esac
mkdir -p "$root/.convt-dev";umask 077
if [[ ! -f $file ]]; then
  printf 'S3_ACCESS_KEY=convt_local_%s\nS3_SECRET_KEY=%s\nS3_BUCKET=convt-jobs\nS3_REGION=us-east-1\n' "$hash" "$(openssl rand -hex 24)" > "$file"
fi
# Existing local configurations also need explicit MinIO addressing.
sed -i '/^S3_PATH_STYLE=/d' "$file"
printf 'S3_PATH_STYLE=true\n' >> "$file"
set -a; . "$file";set +a
bind=${CONVT_STORAGE_BIND:-127.0.0.1}
if ! docker inspect "$name" >/dev/null 2>&1; then
  timeout 120 docker run -d --name "$name" --label "convt.checkout=$root" --label convt.role=storage-dev -p "$bind::9000" -e MINIO_ROOT_USER="$S3_ACCESS_KEY" -e MINIO_ROOT_PASSWORD="$S3_SECRET_KEY" --tmpfs /data:rw,size=8g,uid=10001,gid=10001 -e MINIO_API_CORS_ALLOW_ORIGIN="${CONVT_STORAGE_ORIGIN:-*}" convt-minio:local server /data >/dev/null
else owned;docker start "$name" >/dev/null;fi
port=$(docker inspect --format '{{(index (index .NetworkSettings.Ports "9000/tcp") 0).HostPort}}' "$name")
endpoint=http://$bind:$port
# Replace only our own endpoint line, leaving credentials private.
sed -i '/^S3_ENDPOINT=/d' "$file";printf 'S3_ENDPOINT=%s\n' "$endpoint" >> "$file"
for _ in $(seq 40);do curl -fsS "$endpoint/minio/health/live" >/dev/null 2>&1 && break;sleep 0.25;done
curl -fsS "$endpoint/minio/health/live" >/dev/null
# Build this private local image from the pinned source using sandbox/build-minio.sh.
export S3_ENDPOINT=$endpoint
(cd "$root" && CONVT_STORAGE_DEV=1 RUST_S3_SKIP_LOCATION_CONSTRAINT=true timeout 180 cargo run -q -p convt-server --example init_storage)
printf 'storage ready; configuration: %s\n' "$file"
