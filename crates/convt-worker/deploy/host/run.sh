#!/bin/sh
# Runs one production convt-worker container on a Docker host that passes the
# sandbox gate. Railway grants no CAP_SYS_ADMIN, no writable cgroup and no mknod,
# so the worker cannot run there (docs/p9-cloud-plan.md). This mirrors
# crates/convt-worker/sandbox/process-gate.py: the container's own cgroup is bound
# writable for per-job limits, and nothing else of the host is shared.
#
# ENV_FILE holds DATABASE_URL (pointing at 127.0.0.1:$PG_LOCAL_PORT with
# sslmode=disable), PG_UPSTREAM_HOST, PG_UPSTREAM_PORT and the S3_* variables.
# CA_FILE is the pinned Railway Postgres chain; pg-tls-forward.py verifies it.
#
#   run.sh NAME IMAGE ENV_FILE CA_FILE
set -eu
name=$1 image=$2 env_file=$3 ca_file=$4
here=$(cd "$(dirname "$0")" && pwd -P)
docker run -d --name "$name" --restart unless-stopped \
  --label convt.role=prod-worker \
  --memory 10g --memory-swap 10g --cpus 4 --pids-limit 1024 \
  --cap-add SYS_ADMIN --security-opt apparmor=unconfined --cgroupns host \
  --env-file "$env_file" \
  -v "$ca_file:/run/convt/db-ca.pem:ro" \
  -v "$here/pg-tls-forward.py:/run/convt/pg-tls-forward.py:ro" \
  --entrypoint /usr/bin/python3 "$image" -c '
import os, pathlib, socket, subprocess, time
current = pathlib.Path("/proc/self/cgroup").read_text().strip().split("0::")[1]
assert current != "/" and ".." not in pathlib.PurePosixPath(current).parts
subprocess.run(["mount", "--bind", "/sys/fs/cgroup" + current, "/sys/fs/cgroup"], check=True, timeout=10)
subprocess.run(["mount", "-o", "remount,bind,rw", "/sys/fs/cgroup"], check=True, timeout=10)
os.environ["CONVT_SANDBOX_CGROUP_ROOT"] = "/sys/fs/cgroup"
port = os.environ.get("PG_LOCAL_PORT", "6432")
subprocess.Popen(["/usr/bin/python3", "/run/convt/pg-tls-forward.py", port,
                  os.environ.pop("PG_UPSTREAM_HOST"), os.environ.pop("PG_UPSTREAM_PORT"),
                  "/run/convt/db-ca.pem"])
for _ in range(50):
    try:
        socket.create_connection(("127.0.0.1", int(port)), 1).close()
        break
    except OSError:
        time.sleep(0.1)
os.execv("/usr/local/bin/convt-worker-entrypoint", ["/usr/local/bin/convt-worker-entrypoint"])
'
