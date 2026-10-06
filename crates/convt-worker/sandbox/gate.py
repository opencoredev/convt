"""Adversarial checks with the exact worker isolation flags. No host secrets mounted."""
import json, os, pathlib, subprocess, tempfile, time, uuid
ROOT = pathlib.Path(__file__).resolve().parents[3]
IMAGE = os.environ.get('CONVT_SANDBOX_IMAGE', 'convt-sandbox:local')
owned = []
def run(code, seconds=15, extra=(), internal_deadline=False):
    name = 'convt-gate-' + uuid.uuid4().hex[:16]
    owned.append(name)
    cmd = ['docker', 'run', '--runtime', os.environ.get('CONVT_SANDBOX_RUNTIME','runc'), '--name', name, '--label', f'convt.checkout={ROOT}', '--label', 'convt.role=sandbox-gate', '--network', 'none', '--read-only', '--user', '10001:10001', '--cap-drop', 'ALL', '--security-opt', 'no-new-privileges', '--pids-limit', '64', '--memory', '256m', '--memory-swap', '256m', '--cpus', '1', '--ulimit', 'cpu=3:3', '--ulimit', 'fsize=1048576:1048576', '--tmpfs', '/work:rw,nosuid,nodev,size=64m,uid=10001,gid=10001', '--env', 'HOME=/work', '--entrypoint', '/usr/bin/python3', *extra, IMAGE, '-c', code]
    if internal_deadline:
        cmd[cmd.index('/usr/bin/python3')]='/usr/bin/timeout'
        cmd[cmd.index(IMAGE)+1:]=['--signal=KILL','1','/usr/bin/python3','-c',code]
    p = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, env={'PATH': os.environ['PATH'], 'CONVT_GATE_SECRET': 'must-not-appear'})
    try:
        out, err = p.communicate(timeout=seconds)
    except subprocess.TimeoutExpired:
        subprocess.run(['docker', 'kill', name], capture_output=True, timeout=10, check=False)
        out, err = p.communicate(timeout=10)
    return p.returncode, out.decode(), err.decode()
try:
    checks = {
        'unprivileged, read-only filesystem, no secrets or network': "import os,socket; assert os.getuid()==10001; assert 'CONVT_GATE_SECRET' not in os.environ; assert not os.path.exists('/home/leo'); assert not os.path.exists('/var/run/docker.sock'); assert os.system('touch /etc/escape 2>/dev/null') != 0; s=socket.socket(); s.settimeout(1);\ntry: s.connect(('1.1.1.1',443)); raise AssertionError('network escaped')\nexcept OSError: pass\nprint('PASS isolation')",
        'file-size limit': "import os;\ntry:\n f=open('/work/large','wb'); f.write(b'x'*2097152); f.flush(); raise AssertionError('file limit escaped')\nexcept OSError: pass\nassert os.stat('/work/large').st_size <= 1048576; print('PASS fsize')",
        'memory limit': "x=bytearray(512*1024*1024); print('ESCAPED memory')",
        'CPU limit': "while True: pass",
        'wall timeout kills the whole process tree': "import os,time;\nif os.fork()==0:\n while True: time.sleep(1)\nwhile True: time.sleep(1)",
    }
    for label, code in checks.items():
        rc, out, err = run(code, seconds=5 if 'wall' in label else 15)
        if label.startswith(('memory','CPU','wall')):
            assert rc != 0 and 'ESCAPED' not in out, (label,rc,out,err)
            state = subprocess.check_output(['docker','inspect','--format','{{.State.Running}}',owned[-1]], timeout=10).decode().strip()
            assert state == 'false'
        else:
            assert rc == 0 and 'PASS' in out, (label,rc,out,err)
        print('PASS',label,flush=True)
    started=time.monotonic()
    rc,out,err=run(checks['wall timeout kills the whole process tree'],internal_deadline=True)
    assert rc!=0 and time.monotonic()-started<5,(rc,out,err)
    assert subprocess.check_output(['docker','inspect','--format','{{.State.Running}}',owned[-1]],timeout=10).decode().strip()=='false'
    print('PASS container deadline kills whole tree without worker',flush=True)
    with tempfile.TemporaryDirectory(prefix='convt-gate-input-') as d:
        inp = pathlib.Path(d)/'input.svg'
        inp.write_text('<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32"><rect width="32" height="32" fill="red"/></svg>')
        rc,out,err=run("import subprocess; subprocess.run(['/opt/convt/convt','/input.svg','--to','png','--out-dir','/work','--json'],check=True); import pathlib; from struct import unpack; p=next(pathlib.Path('/work').glob('*.png')); assert p.read_bytes()[:8]==b'\\x89PNG\\r\\n\\x1a\\n'; print('PASS real SVG to PNG')",extra=('--mount',f'type=bind,src={inp},dst=/input.svg,readonly'))
        assert rc==0,(rc,out,err)
        print(out,flush=True)
finally:
    for name in owned:
        subprocess.run(['docker','rm','-f',name],capture_output=True,timeout=10,check=False)
