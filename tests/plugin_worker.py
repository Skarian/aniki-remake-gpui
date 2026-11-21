"""Real CLR/SDK worker contract checks, without storefront or desktop input."""
import argparse, hashlib, json, shutil, subprocess, tempfile, time
from pathlib import Path

p=argparse.ArgumentParser()
p.add_argument('--exe',type=Path,required=True)
p.add_argument('--playnite',type=Path,default=Path('D:/Playnite'))
p.add_argument('--probe',type=Path)
a=p.parse_args()
root=Path(__file__).resolve().parents[1]
out=root/'artifacts/plugin-worker-tests';out.mkdir(parents=True,exist_ok=True)
framework=Path('C:/Windows/Microsoft.NET/Framework64/v4.0.30319')
dll=out/'PluginFixture.dll'
game_dir=out/'game';game_dir.mkdir(exist_ok=True)
subprocess.run([str(framework/'csc.exe'),'/nologo','/target:library','/out:'+str(dll),
    '/reference:'+str(a.playnite/'Playnite.SDK.dll'),'/reference:'+str(framework/'WPF/PresentationFramework.dll'),
    '/reference:System.Xaml.dll','/reference:'+str(framework/'WPF/WindowsBase.dll'),str(root/'tests/PluginFixture.cs')],check=True)
def request(mode,module=dll):
    return dict(root=str(a.playnite),module=str(module),game=dict(
        Id='22222222-2222-2222-2222-222222222222',PluginId='11111111-1111-1111-1111-111111111111',
        Name='Fixture',GameId=mode,InstallDirectory=str(game_dir),IsInstalled=True))
def spawn(mode,module=dll):
    child=subprocess.Popen([str(a.exe),'--plugin-worker'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,text=True,creationflags=subprocess.CREATE_NO_WINDOW)
    child.stdin.write(json.dumps(request(mode,module))+'\n');child.stdin.flush()
    return child
results=[]
for mode,expected in [('normal',['started','stopped']),('serialization',['started','stopped']),('early-stop',['stopped']),
    ('automatic',['automatic']),('automatic-directory',['automatic']),('throw',['error']),('unsupported',['error']),('multiple',['error']),('crash',[])]:
    child=spawn(mode)
    try:
        child.wait(timeout=12)
        lines=child.stdout.read().splitlines()
        events=[json.loads(x[6:]) for x in lines if x.startswith('ANIKI:')]
        assert [e['event'] for e in events]==expected,(mode,child.returncode,events,child.stderr.read())
        if mode=='normal': assert events[0]['pid']==1234,events
        if mode=='crash': assert child.returncode==17,child.returncode
        if mode=='automatic': assert events[0]['plan']['kind']=='process',events
        results.append(dict(case=mode,passed=True,events=events))
    finally:
        if child.poll() is None: child.kill();child.wait()
# Dropping the parent pipe must stop a custom controller that never sends Started.
child=spawn('pending');time.sleep(1);child.stdin.close()
child.wait(timeout=5);assert child.returncode==0,child.returncode
assert not (Path(tempfile.gettempdir())/('aniki-plugin-'+str(child.pid))).exists(),'Orphan settings copy remains'
results.append(dict(case='parent-disconnect',passed=True))
if a.probe:
    runner=out/'runner';(runner/'importer').mkdir(parents=True,exist_ok=True)
    shutil.copy2(a.probe,runner/'launch_probe.exe')
    shutil.copy2(a.exe.parent/'importer/Aniki.PluginHost.dll',runner/'importer/Aniki.PluginHost.dll')
    subprocess.run([str(framework/'csc.exe'),'/nologo','/target:winexe',
        '/reference:System.Windows.Forms.dll','/reference:System.Drawing.dll',
        '/out:'+str(game_dir/'WindowFixture.exe'),str(root/'tests/WindowFixture.cs')],check=True)
    for mode in ('normal','early-stop','throw','crash','pending','automatic','automatic-directory',
                 'custom-missing-directory','custom-file-directory','automatic-missing-directory'):
        payload=request('normal' if mode.startswith('custom-') else 'automatic-directory' if mode=='automatic-missing-directory' else mode)
        install=game_dir
        if mode.endswith('missing-directory'): install=out/'missing-game'
        if mode=='custom-file-directory': install=dll
        if mode=='automatic-missing-directory': payload['game']['InstallDirectory']=str(install)
        plan=out/(mode+'.json');plan.write_text(json.dumps(dict(kind='plugin',install_dir=str(install),plugin=payload)))
        command=[str(runner/'launch_probe.exe'),str(plan)]
        if mode=='pending': command.append('500')
        run=subprocess.run(command,capture_output=True,text=True,timeout=20)
        assert run.returncode==0,(mode,run.stdout,run.stderr)
        events=[json.loads(line) for line in run.stdout.splitlines()]
        if mode=='pending': assert events==[],events
        elif mode in ('throw','crash','automatic-missing-directory'): assert len(events)==1 and 'Failed' in events[0],events
        elif mode=='early-stop': assert events==['Ended'],events
        else: assert events[0]=='Started' and events[-1]=='Ended',events
        results.append(dict(case='rust-'+mode,passed=True,events=events))
report=dict(sha256=hashlib.sha256(a.exe.read_bytes()).hexdigest(),
    adapter_sha256=hashlib.sha256((a.exe.parent/'importer/Aniki.PluginHost.dll').read_bytes()).hexdigest(),
    probe_sha256=hashlib.sha256(a.probe.read_bytes()).hexdigest() if a.probe else None,cases=results)
(out/'results.json').write_text(json.dumps(report,indent=2))
print(json.dumps(report,indent=2))
