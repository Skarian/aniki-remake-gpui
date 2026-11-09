"""Standalone same-executable host tests with a short-lived local fixture game.
Creates only artifacts/session-handoff/fixture. No Playnite edits or real game.
"""
import ctypes as c, json, shutil, subprocess, sys, time
from pathlib import Path
root=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(root/'.tools/python'))
import vgamepad as vg
from compositor_probe import sample as compositor_sample
app=root/'artifacts/session-handoff/fixture'
app.mkdir(parents=True,exist_ok=True)
(app/'cache').mkdir(exist_ok=True); (app/'artifacts').mkdir(exist_ok=True)
shutil.copy2(root/'target/release/aniki-remake-gpui.exe',app/'aniki-remake-gpui.exe')
# A test-only copy avoids the launcher's intentional "already running" check
# against the host's executable path. This is never part of the app package.
shutil.copy2(root/'target/release/aniki-remake-gpui.exe',app/'fixture-game.exe')
shutil.copy2(root/'SDL2.dll',app/'SDL2.dll')
shutil.copytree(root/'assets',app/'assets',dirs_exist_ok=True)
config=dict(playnite_root=str(app/'no-playnite'),music=False,video=False,fullscreen=True)
(app/'aniki-remake-gpui.json').write_text(json.dumps(config),encoding='utf-8')
game=dict(id='lifecycle-fixture',name='Lifecycle test game',platform='PC',launch=dict(kind='process',
    executable=str(app/'fixture-game.exe'),arguments='--fixture-game',
    # Track the spawned fixture PID only. Sharing an install-directory fallback
    # with the host would falsely classify duplicate launcher invocations as games.
    working_dir=str(app),install_dir='',steam_id='',error=''))
lib=dict(games=[game],exported_utc='',platform_art={})
def save_library(): (app/'cache/library.json').write_text(json.dumps(lib),encoding='utf-8')
save_library()
log=app/'artifacts/session.jsonl'
if log.exists(): log.rename(app/'artifacts'/f'session-{time.time_ns()}.jsonl')
pad=vg.VX360Gamepad(); buttons=vg.XUSB_BUTTON
compositor_probes=[]
probe_path=app/'artifacts/compositor-probes.json'
probe_path.write_text('[]')
def press(button,settle=.4):
    pad.press_button(button=button); pad.update(); time.sleep(.09)
    pad.release_button(button=button); pad.update(); time.sleep(settle)
def events():
    if not log.exists(): return []
    data=log.read_text(encoding='utf-8')
    lines=data.splitlines()
    if data and not data.endswith('\n'): lines=lines[:-1]
    return [json.loads(line) for line in lines]
def wait_for(predicate,timeout=20):
    deadline=time.monotonic()+timeout
    while time.monotonic()<deadline:
        rows=events()
        revealed=[e for e in rows if e['event']=='session_reveal']
        if revealed:
            pid=revealed[-1]['pid']
            if not any(e['pid']==pid and e['event']=='session_fade_complete' for e in rows):
                probe=compositor_sample(pid)
                if probe:
                    compositor_probes.append(probe)
                    probe_path.write_text(json.dumps(compositor_probes,indent=2))
        if predicate(rows): return rows
        time.sleep(.1)
    raise AssertionError('Timed out waiting for lifecycle event')
def start():
    p=subprocess.Popen([str(app/'aniki-remake-gpui.exe'),'--trace-memory','--trace-motion'],cwd=app)
    wait_for(lambda rows:any(e['pid']==p.pid and e['event']=='ui_visible' for e in rows))
    return p
def quit_home():
    press(buttons.XUSB_GAMEPAD_START)
    for _ in range(3): press(buttons.XUSB_GAMEPAD_DPAD_DOWN,.15)
    press(buttons.XUSB_GAMEPAD_A)
result={'cycles':[]}
p=None
followers=[]
def duplicate(owner):
    follower=subprocess.Popen([str(app/'aniki-remake-gpui.exe')],cwd=app)
    followers.append(follower)
    rows=wait_for(lambda rows:any(e['pid']==follower.pid and e['event']=='session_attached' for e in rows))
    assert follower.poll() is None, 'external launcher lifetime ended on duplicate open'
    assert not any(e['pid']==follower.pid and e['event']=='host_started' for e in rows)
    assert sum(e['event']=='host_started' and e['pid']==owner.pid for e in rows)==1
    return follower
try:
    p=start()
    duplicate(p)
    wait_for(lambda rows:any(e['event']=='session_activated' for e in rows))
    result['duplicate_browse_reuses_ui']=True
    for cycle in range(2):
        if cycle == 0: press(buttons.XUSB_GAMEPAD_A)
        press(buttons.XUSB_GAMEPAD_A)
        if cycle==0:
            wait_for(lambda rows:any(e['pid']==p.pid and e['event']=='ui_exited' and 'state=Playing' in e['detail'] for e in rows))
            duplicate(p)
            rows=wait_for(lambda rows:any(e['pid']==p.pid and e['event']=='session_activation' and e['detail']=='state=Playing' for e in rows))
            assert sum(e['event']=='snapshot_loaded' for e in rows)==2, 'a duplicate frontend opened during gameplay'
            result['duplicate_game_does_not_reopen_ui']=True
        rows=wait_for(lambda rows:sum(e['pid']==p.pid and e['event']=='ui_visible' for e in rows)>=cycle+2)
        own=[e for e in rows if e['pid']==p.pid]
        assert sum(e['event']=='game_focus' for e in own)==cycle+1
        assert sum(e['event']=='ui_exited' and 'state=Playing' in e['detail'] for e in own)==cycle+1
        returned_pid=next(e['pid'] for e in reversed(rows) if e['event']=='session_reveal')
        frames=[json.loads(e['detail']) for e in rows if e['pid']==returned_pid and e['event']=='motion_frame']
        assert frames and all(f['page']=='details' for f in frames), 'Return did not restore Details'
        result['cycles'].append({'cycle':cycle+1,'frontend_exited':True,'returned':True})
    # ui_visible acknowledges the expired timer; wait for a terminal render
    # too, rather than racing the compositor's next frame callback.
    def terminal_render(rows):
        pid=next(e['pid'] for e in reversed(rows) if e['event']=='session_reveal')
        return any(e['pid']==pid and e['event']=='motion_frame' and json.loads(e['detail'])['session_fade']==1 for e in rows)
    wait_for(terminal_render,timeout=3)
    # Preserve the separate rendering verdict but finish cleanup/crash checks
    # even if the current display's frame cadence misses the smoothness budget.
    trace=subprocess.run([sys.executable,str(root/'tests/session_trace.py'),str(app)],capture_output=True,text=True)
    result['fade_trace_passed']=trace.returncode==0
    result['fade_trace_output']=trace.stdout+trace.stderr
    if trace.returncode==0:
        result['fade_smoothness_measured']=json.loads((app/'artifacts/session-trace.json').read_text())['smoothness_measured']
    press(buttons.XUSB_GAMEPAD_B)
    quit_home(); p.wait(timeout=5); assert p.returncode==0
    for follower in followers:
        follower.wait(timeout=5); assert follower.returncode==0
    result['duplicate_lifetimes_follow_owner']=True
    result['normal_exit_stops_host']=True
    game['launch']['executable']=str(app/'intentionally-missing.exe'); save_library()
    p=start()
    press(buttons.XUSB_GAMEPAD_A); press(buttons.XUSB_GAMEPAD_A)
    rows=wait_for(lambda rows:any(e['event']=='launch_failed' and 'intentionally-missing' in e['detail'] for e in rows))
    assert p.poll() is None
    result['launch_failure_keeps_ui']=True
    press(buttons.XUSB_GAMEPAD_B); quit_home(); p.wait(timeout=5)
    result['failure_exit_stops_host']=p.returncode==0
    # Closing the owning process must close its UI through inherited-pipe EOF.
    p=start()
    crash_follower=duplicate(p)
    rows=events(); ui=next(e['pid'] for e in reversed(rows) if e['event']=='snapshot_loaded')
    k=c.WinDLL('kernel32'); k.OpenProcess.restype=c.c_void_p
    handle=k.OpenProcess(0x100000,False,ui)
    assert handle
    p.terminate(); p.wait(timeout=5)
    crash_follower.wait(timeout=5); assert crash_follower.returncode==0
    result['duplicate_exits_after_owner_crash']=True
    k.WaitForSingleObject.argtypes=[c.c_void_p,c.c_uint]
    assert k.WaitForSingleObject(handle,5000)==0
    k.CloseHandle.argtypes=[c.c_void_p]; k.CloseHandle(handle)
    result['host_exit_closes_frontend']=True
    p=start(); quit_home(); p.wait(timeout=5); assert p.returncode==0
    result['restart_after_owner_crash']=True
    result['lifecycle_passed']=True
    result['passed']=result['fade_trace_passed']
finally:
    pad.reset(); pad.update()
    if p and p.poll() is None: p.terminate(); p.wait(timeout=5)
    for follower in followers:
        if follower.poll() is None: follower.terminate(); follower.wait(timeout=5)
    (root/'artifacts/session-handoff/fixture-results.json').write_text(json.dumps(result,indent=2),encoding='utf-8')
print(json.dumps(result,indent=2))
if not result['passed']: raise SystemExit(1)
