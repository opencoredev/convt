"""Exercise the Railway sandbox in one container, without a nested daemon."""
import json
import os
import pathlib
import subprocess
import tempfile
import uuid

ROOT = pathlib.Path(__file__).resolve().parents[3]
IMAGE = os.environ.get('CONVT_PROCESS_IMAGE', 'convt-railway-worker:local')
SETUP = r'''
import os, pathlib, subprocess
current=pathlib.Path('/proc/self/cgroup').read_text().strip().split('0::')[1]
assert current!='/' and '..' not in pathlib.PurePosixPath(current).parts
subprocess.run(['mount','--bind','/sys/fs/cgroup'+current,'/sys/fs/cgroup'],check=True,timeout=10)
subprocess.run(['mount','-o','remount,bind,rw','/sys/fs/cgroup'],check=True,timeout=10)
os.environ['CONVT_SANDBOX_CGROUP_ROOT']='/sys/fs/cgroup'
'''
PROGRAM = r'''
import ctypes, os, pathlib, select, signal, subprocess, time
ctypes.CDLL(None).prctl(36, 1, 0, 0, 0) # reap only our adopted process groups
pathlib.Path('/parent-secret').write_text('gate-only-secret')
subprocess.run(['/usr/local/bin/convt-worker-entrypoint', '--sandbox-test'], check=True, timeout=90)
worker = subprocess.Popen(['/opt/convt/convt-worker', '--sandbox-orphan'], stdout=subprocess.PIPE, text=True)
try:
    assert select.select([worker.stdout], [], [], 20)[0], 'orphan probe readiness timeout'
    pgid = int(worker.stdout.readline())
    time.sleep(.3)
    worker.kill()
    worker.wait(timeout=5)
    deadline = time.monotonic()+5
    while time.monotonic()<deadline:
        try:
            while os.waitpid(-pgid, os.WNOHANG)[0]>0: pass
        except ChildProcessError: pass
        try: os.killpg(pgid,0)
        except ProcessLookupError:
            print('PASS supervisor death kills whole tree', flush=True)
            break
        time.sleep(.05)
    else: raise AssertionError('orphan process group survived')
finally:
    if worker.poll() is None: worker.kill(); worker.wait(timeout=5)
'''


def run(extra, args, expected_success=True):
    name = 'convt-process-gate-' + uuid.uuid4().hex[:12]
    command = ['docker', 'run', '--name', name, '--rm', '--network', 'none',
               '--label', f'convt.checkout={ROOT}', '--label', 'convt.role=process-gate',
               '--memory', '8g', '--cpus', '4', '--pids-limit', '512',
               '--cap-add','SYS_ADMIN','--security-opt','apparmor=unconfined','--cgroupns','host',
               '-e', 'CONVT_GATE_SECRET=gate-only-secret'] + extra + ['--entrypoint','/usr/bin/python3',IMAGE,'-c',
               SETUP + (PROGRAM if args == ['-c',PROGRAM] else '\nimport sys\nsys.exit(subprocess.run(["/usr/local/bin/convt-worker-entrypoint"]+'+repr(args)+',timeout=90).returncode)')]
    try:
        result = subprocess.run(command, text=True, capture_output=True, timeout=180)
        print(result.stdout, end='')
        if (result.returncode == 0) != expected_success:
            raise AssertionError(result.stderr or f'unexpected exit {result.returncode}')
        return result.stdout, result.stderr
    finally:
        subprocess.run(['docker', 'rm', '-f', name], capture_output=True, timeout=15)


# A normal restricted container must fail closed, without adding privileges or
# binding cgroups to repair the host behind the operator's back.
name = 'convt-process-restricted-' + uuid.uuid4().hex[:12]
try:
    rejected = subprocess.run(['docker','run','--rm','--name',name,'--network','none',
        '--label',f'convt.checkout={ROOT}','--label','convt.role=process-gate',
        IMAGE,'--sandbox-gate'],capture_output=True,text=True,timeout=45)
    assert rejected.returncode != 0 and ('cgroup' in rejected.stderr or 'resource' in rejected.stderr), rejected.stderr
    print('PASS restricted host refuses resource startup', flush=True)
finally:
    subprocess.run(['docker','rm','-f',name],capture_output=True,timeout=15)

run([], ['-c', PROGRAM])
with tempfile.TemporaryDirectory(prefix='convt-unsupported-kernel-') as directory:
    profile = pathlib.Path(directory)/'seccomp.json'
    profile.write_text(json.dumps({'defaultAction':'SCMP_ACT_ALLOW', 'syscalls':[
        {'names':['landlock_restrict_self','seccomp'], 'action':'SCMP_ACT_ERRNO', 'errnoRet':1}]}))
    extra = ['--security-opt', 'seccomp='+str(profile)]
    _, error = run(extra, ['--sandbox-gate'], False)
    assert 'sandbox startup probe failed' in error
    print('PASS unsupported kernel refuses startup')
    output, _ = run(extra+['-e','CONVT_SANDBOX_ALLOW_UNSAFE=1'], ['--sandbox-gate'])
    report=json.loads(output)
    assert report['unsafe_override'] and not report['landlock'] and not report['seccomp']
    print('PASS explicit unsafe override reports missing protections')
