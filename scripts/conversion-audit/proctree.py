"""subprocess.run with a timeout that also stops the child's descendants.

convt starts FFmpeg and LibreOffice in their own process groups, so killing convt
alone on a timeout leaves a stalled encoder running while the audit moves on.
"""

import os
import signal
import subprocess
import time
from pathlib import Path


def _descendants(pid: int) -> list[int]:
    children: dict[int, list[int]] = {}
    for d in Path("/proc").iterdir():
        if not d.name.isdigit():
            continue
        try:
            # The command name is in parentheses and may hold spaces; ppid follows it.
            ppid = int((d / "stat").read_text().rsplit(")", 1)[1].split()[1])
        except (OSError, IndexError, ValueError):
            continue
        children.setdefault(ppid, []).append(int(d.name))
    found, todo = [], [pid]
    while todo:
        for c in children.get(todo.pop(), []):
            found.append(c)
            todo.append(c)
    return found


def _signal(pids, sig):
    for p in pids:
        try:
            os.kill(p, sig)
        except ProcessLookupError:
            pass


def run(args, *, timeout, capture_output=False, text=False, check=False, **kw):
    """Like subprocess.run, but a timeout stops the whole process tree.

    The child gets SIGTERM first so convt can cancel its own work, then anything
    left after five seconds gets SIGKILL.
    """
    if capture_output:
        kw.setdefault("stdout", subprocess.PIPE)
        kw.setdefault("stderr", subprocess.PIPE)
    with subprocess.Popen(args, text=text, **kw) as p:
        try:
            out, err = p.communicate(timeout=timeout)
        except subprocess.TimeoutExpired:
            tree = _descendants(p.pid)
            _signal([p.pid, *tree], signal.SIGTERM)
            deadline = time.monotonic() + 5
            while time.monotonic() < deadline and p.poll() is None:
                time.sleep(0.1)
            _signal([p.pid, *tree, *_descendants(p.pid)], signal.SIGKILL)
            out, err = p.communicate()
            raise subprocess.TimeoutExpired(p.args, timeout, out, err) from None
    r = subprocess.CompletedProcess(p.args, p.returncode, out, err)
    if check:
        r.check_returncode()
    return r
