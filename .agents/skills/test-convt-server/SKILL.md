---
name: test-convt-server
description: Run and check the convt cloud API (crates/convt-server, Axum) and worker (crates/convt-worker) locally. Use when changing either crate, adding an API route, or when asked to verify the cloud service.
---

# Test the convt cloud service

`convt-server` is an Axum API deployed to Railway. Implemented routes are `GET /health` (returns `ok`) and `GET /v1/formats` (JSON list of formats). Job upload and status routes are TODOs. `convt-worker` only logs which engines are ready and waits for Ctrl-C; its Postgres queue and object storage are not wired up. Neither needs credentials or a database today.

## Launch the server

Railway injects `PORT`; locally pick a free port so two worktrees do not collide. The server binds `0.0.0.0`, so it is reachable from the network while it runs.

```sh
work=$(mktemp -d /tmp/convt-server.XXXXXX)
port=18080
cargo build -p convt-server
PORT=$port ./target/debug/convt-server >"$work/server.log" 2>&1 &
echo $! > "$work/server.pid"
for _ in $(seq 40); do curl -fsS "http://127.0.0.1:$port/health" && break; sleep 0.25; done
```

Readiness: `/health` returns `ok` and `server.log` has a `listening on` line with your port. If the bind fails with "address in use", pick another port; do not kill the other listener.

## Evidence

```sh
curl -fsS "http://127.0.0.1:$port/v1/formats" | python3 -c 'import json,sys; d=json.load(sys.stdin); print(len(d), d[0])'
curl -s -o /dev/null -w '%{http_code}\n' "http://127.0.0.1:$port/nope"   # expect 404
```

For a new route, test the success case, an error case, and the response shape. For anything CI should keep checking, write an in-process test that calls `app()` with `tower::ServiceExt::oneshot` (add `tower` with the `util` feature as a dev-dependency; it is not one yet).

## Worker

```sh
./target/debug/convt-worker   # or: cargo run -p convt-worker
```

Expect one `engine ready` line per engine and `engine unavailable` warnings for the rest, matching `./target/debug/convt engines`. Stop it with Ctrl-C, or `kill -INT` the PID you started.

## Cleanup

`kill "$(cat "$work/server.pid")"`, then `rm -rf "$work"`. Never deploy to Railway or point the service at a real database or bucket without the user's explicit authorization.
