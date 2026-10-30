"""subprocess.run with a timeout that also stops the child's descendants.

convt starts FFmpeg and LibreOffice in their own process groups, so killing convt
alone on a timeout leaves a stalled encoder running while the audit moves on.
"""

import os
import signal
import subprocess
import time
from pathlib import Path

# A process is (pid, start time in clock ticks): the pair can't be reused, so a
# pid recorded minutes ago is never signalled once another process holds it.
Proc = tuple[int, int]


def _stat(pid: int) -> tuple[int, int] | None:
    """(ppid, start time) of a live process, or None."""
    try:
        # The command name is in parentheses and may hold spaces; fields follow it.
        fields = Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].split()
        return int(fields[1]), int(fields[19])
    except (OSError, IndexError, ValueError):
        return None


def _descendants(pid: int) -> list[Proc]:
    children: dict[int, list[Proc]] = {}
    for d in Path("/proc").iterdir():
        if d.name.isdigit() and (st := _stat(int(d.name))):
            children.setdefault(st[0], []).append((int(d.name), st[1]))
    found, todo = [], [pid]
    while todo:
        for c in children.get(todo.pop(), []):
            found.append(c)
            todo.append(c[0])
    return found


def _signal(procs, sig):
    for pid, start in procs:
        st = _stat(pid)
        if st and st[1] == start:
            try:
                os.kill(pid, sig)
            except ProcessLookupError:
                pass


def run(args, *, timeout, capture_output=False, text=False, check=False, **kw):
    """Like subprocess.run, but a timeout stops the whole process tree.

    Descendants are recorded every second while the child runs, so one whose
    parent already exited (and was reparented) is still found. On timeout the
    child gets SIGTERM first so convt can cancel its own work, then anything left
    after five seconds gets SIGKILL, and the final pipe read is bounded too.
    """
    if capture_output:
        kw.setdefault("stdout", subprocess.PIPE)
        kw.setdefault("stderr", subprocess.PIPE)
    with subprocess.Popen(args, text=text, **kw) as p:
        deadline = time.monotonic() + timeout
        seen: set[Proc] = set()
        while True:
            try:
                out, err = p.communicate(timeout=max(0.0, min(1.0, deadline - time.monotonic())))
                break
            except subprocess.TimeoutExpired:
                seen.update(_descendants(p.pid))
                if time.monotonic() < deadline:
                    continue
            seen.update(_descendants(p.pid))
            # Popen signals its own child safely: the pid stays ours until it is reaped.
            p.terminate()
            _signal(seen, signal.SIGTERM)
            grace = time.monotonic() + 5
            while time.monotonic() < grace and p.poll() is None:
                time.sleep(0.1)
            p.kill()
            _signal(seen | set(_descendants(p.pid)), signal.SIGKILL)
            try:
                out, err = p.communicate(timeout=10)
            except subprocess.TimeoutExpired:
                # Something we never saw still holds the pipes; give up on them.
                out = err = None
            raise subprocess.TimeoutExpired(p.args, timeout, out, err) from None
    r = subprocess.CompletedProcess(p.args, p.returncode, out, err)
    if check:
        r.check_returncode()
    return r
