"""Exercise actual Epic/GOG DLL actions with isolated, synthetic installation data.

No store client, URL handler, account, or real game is started. The optional
probe launches only WindowFixture.exe through GOG's real relative-path action.
"""
import argparse
import base64
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

p = argparse.ArgumentParser(description=__doc__)
p.add_argument('--exe', type=Path, required=True)
p.add_argument('--plugins', type=Path, required=True)
p.add_argument('--playnite', type=Path, default=Path('D:/Playnite'))
p.add_argument('--probe', type=Path)
a = p.parse_args()
root = Path(__file__).resolve().parents[1]
out = root / 'artifacts/storefront-action-tests'
out.mkdir(parents=True, exist_ok=True)
scratch = Path(tempfile.mkdtemp(prefix='run-', dir=out))
host = scratch / 'playnite'
host.mkdir()
for name in ('Playnite.SDK.dll', 'Newtonsoft.Json.dll', 'Playnite.DesktopApp.exe'):
    shutil.copy2(a.playnite / name, host / name)
epic = a.plugins / 'EpicGamesLibrary_Builtin/EpicLibrary.dll'
gog = a.plugins / 'GogLibrary_Builtin/GogLibrary.dll'
epic_id = '00000002-dbd1-46c6-b5d0-b1ba559d10e4'
gog_id = 'aebe8b7c-6dc3-4a66-af31-e7375c6b5e9e'
game = scratch / 'game'
game.mkdir()
results = []

def write(path, content):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content if isinstance(content, str) else json.dumps(content), encoding='utf-8')

def request(module, plugin_id, game_id):
    return dict(root=str(host), module=str(module.resolve()), game=dict(
        Id='22222222-2222-2222-2222-222222222222', PluginId=plugin_id,
        GameId=game_id, Name='Synthetic fixture', IsInstalled=True, InstallDirectory=str(game)))

def action(case, payload, env=None, error=None):
    run = subprocess.run([str(a.exe), '--plugin-worker'], input=json.dumps(payload)+'\n',
                         capture_output=True, text=True, timeout=15, env=env,
                         creationflags=subprocess.CREATE_NO_WINDOW)
    events = [json.loads(line[6:]) for line in run.stdout.splitlines() if line.startswith('ANIKI:')]
    assert len(events) == 1, (case, run.returncode, events, run.stderr)
    event = events[0]
    if error:
        assert event['event'] == 'error' and error in event['message'], (case, event)
    else:
        assert run.returncode == 0 and event['event'] == 'automatic', (case, event, run.stderr)
    results.append(dict(case=case, passed=True, event=event))
    return event.get('plan')

catalog = dict(id='fixture-item', namespace='fixture-namespace', releaseInfo=[dict(appId='fixture-app')])
for variant in ('legacy', 'v4', 'v2', 'malformed'):
    area = scratch / variant
    env = dict(os.environ, PROGRAMDATA=str(area/'programdata'), LOCALAPPDATA=str(area/'local'))
    epic_data = area / 'programdata/Epic'
    if variant == 'legacy':
        write(epic_data/'EpicGamesLauncher/Data/Manifests/good.item', dict(
            AppName='fixture-app', CatalogNamespace='fixture-namespace', CatalogItemId='fixture-item'))
        write(epic_data/'EpicGamesLauncher/Data/Manifests/bad.item', '{')
    elif variant == 'v4':
        write(epic_data/'EpicOnlineServicesShared/InstallHelper/InstalledItems/good.egi',
              dict(v4=dict(state='Installed', dir=str(game), artifactId='fixture-app')))
        write(epic_data/'EpicOnlineServicesShared/InstallHelper/InstalledItems/bad.egi', '{')
        write(epic_data/'EpicGamesLauncher/Data/Catalog/catcache.bin',
              base64.b64encode(json.dumps([catalog]).encode()).decode())
    else:
        write(area/'local/Epic Games/Launcher/EpicGamesLauncher.exe', '')
        write(area/'local/com.epicgames.eglv2/cache/prod/catalog-cache.json',
              dict(items=dict(fixture=dict(item=catalog))) if variant == 'v2' else '{')
    plan = action('epic-'+variant, request(epic, epic_id, 'fixture-app'), env,
                  'installation data manifest not found' if variant == 'malformed' else None)
    if plan:
        assert plan['url'] and plan['kind'] == 'directory' and plan['install_dir'] == str(game), plan
        assert plan['executable'] == 'com.epicgames.launcher://apps/fixture-namespace%3Afixture-item%3Afixture-app?action=launch&silent=true', plan

manifest = game / 'goggame-999999999999999.info'
payload = request(gog, gog_id, '999999999999999')
task = dict(isPrimary=True, type='FileTask', path='WindowFixture.exe', arguments='exit "'+str(game/'window.hwnd')+'"')
write(manifest, dict(playTasks=[task]))
plan = action('gog-absolute', payload)
assert Path(plan['executable']) == game/'WindowFixture.exe' and plan['kind'] == 'process', plan
task.update(path='bin/WindowFixture.exe', workingDir='bin')
write(manifest, dict(playTasks=[task]))
plan = action('gog-relative', payload)
assert plan['executable'] == 'WindowFixture.exe' and Path(plan['working_dir']) == game/'bin', plan

if a.probe:
    runner = scratch/'runner'
    (runner/'importer').mkdir(parents=True)
    (game/'bin').mkdir()
    shutil.copy2(a.probe, runner/'launch_probe.exe')
    shutil.copy2(a.exe.parent/'importer/Aniki.PluginHost.dll', runner/'importer/Aniki.PluginHost.dll')
    subprocess.run(['C:/Windows/Microsoft.NET/Framework64/v4.0.30319/csc.exe', '/nologo', '/target:winexe',
                    '/reference:System.Windows.Forms.dll', '/reference:System.Drawing.dll',
                    '/out:'+str(game/'bin/WindowFixture.exe'), str(root/'tests/WindowFixture.cs')], check=True)
    launch = scratch/'relative-plan.json'
    write(launch, dict(kind='plugin', install_dir=str(game), plugin=payload))
    run = subprocess.run([str(runner/'launch_probe.exe'), str(launch)], capture_output=True, text=True, timeout=20)
    events = [json.loads(line) for line in run.stdout.splitlines()]
    assert run.returncode == 0 and events[0] == 'Started' and events[-1] == 'Ended', (run.stderr, events)
    results.append(dict(case='gog-relative-rust-dispatch', passed=True, events=events))

write(manifest, dict(playTasks=[task, task]))
action('gog-multiple-explicit-error', payload, error='2 plugin play actions')
write(manifest, dict(playTasks=[dict(isPrimary=True, type='URLTask', link='https://example.invalid/')]))
action('gog-url-process-explicit-error', payload, error='URL actions require directory tracking')
settings = host/'ExtensionsData'/gog_id/'config.json'
write(settings, dict(StartGamesUsingGalaxy=True))
settings_hash = hashlib.sha256(settings.read_bytes()).hexdigest()
plan = action('gog-galaxy', payload)
assert not plan['url'] and plan['kind'] == 'directory' and plan['install_dir'] == str(game), plan
assert '/gameId=999999999999999' in plan['arguments'] and str(game) in plan['arguments'], plan
assert hashlib.sha256(settings.read_bytes()).hexdigest() == settings_hash, 'Plugin changed original settings'
report = dict(exe_sha256=hashlib.sha256(a.exe.read_bytes()).hexdigest(),
              adapter_sha256=hashlib.sha256((a.exe.parent/'importer/Aniki.PluginHost.dll').read_bytes()).hexdigest(),
              plugins={str(path): hashlib.sha256(path.read_bytes()).hexdigest() for path in (epic, gog)},
              scratch=str(scratch), cases=results)
write(out/'results.json', report)
print(json.dumps(report, indent=2))
