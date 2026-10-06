"""Generate cloud targets from the CLI inside the actual sandbox image."""
import json, pathlib, subprocess, os, tempfile
root=pathlib.Path(__file__).resolve().parents[3]
image=os.environ.get('CONVT_SANDBOX_IMAGE','convt-sandbox:local')
args=['docker','run','--rm','--label',f'convt.checkout={root}','--label','convt.role=capabilities','--network','none','--read-only','--user','10001:10001','--cap-drop','ALL','--security-opt','no-new-privileges','--pids-limit','64','--memory','512m','--cpus','1','--tmpfs','/work:rw,size=64m,uid=10001,gid=10001','--tmpfs','/tmp:rw,size=64m,mode=1777',image]
formats=json.loads(subprocess.check_output(args+['formats','--json'],timeout=30))
for f in formats:
    result=subprocess.run(args+['targets','input.'+f['extensions'][0]],text=True,capture_output=True,timeout=30,check=True)
    f['targets']=result.stdout.splitlines()
manifest={'image_id':subprocess.check_output(['docker','image','inspect','--format','{{.Id}}',image],text=True,timeout=10).strip(),'formats':formats}
for path in ['crates/convt-server/cloud-formats.json','apps/web/src/generated/cloud-formats.json']:
    (root/path).write_text(json.dumps(manifest,indent=2)+'\n')
print('Generated',len(formats),'cloud format target lists from',image)
