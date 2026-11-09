"""Retained-window lifecycle regression: real windows, virtual Xbox and DS4.
No personal library is needed. Owns only artifacts/retained-window/fixture.
"""
import ctypes as c, json, shutil, subprocess, sys, time
from pathlib import Path
root=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(root/'.tools/python'))
import vgamepad as vg
app=root/'artifacts/retained-window/fixture'
app.mkdir(parents=True,exist_ok=True)
for d in ('cache','artifacts'): (app/d).mkdir(exist_ok=True)
shutil.copy2(root/'target/release/aniki-remake-gpui.exe',app/'aniki-remake-gpui.exe')
shutil.copy2(root/'SDL2.dll',app/'SDL2.dll')
shutil.copytree(root/'assets',app/'assets',dirs_exist_ok=True)
subprocess.run([r'C:\Windows\Microsoft.NET\Framework64\v4.0.30319\csc.exe','/nologo','/target:winexe','/reference:System.Windows.Forms.dll','/reference:System.Drawing.dll','/out:'+str(app/'WindowFixture.exe'),str(root/'tests/WindowFixture.cs')],check=True)
(app/'aniki-remake-gpui.json').write_text(json.dumps(dict(playnite_root=str(app/'no-playnite'),fullscreen=True,music=False,video=False)))
log=app/'artifacts/session.jsonl'
u=c.WinDLL('user32'); k=c.WinDLL('kernel32')
u.GetForegroundWindow.restype=c.c_void_p
u.IsIconic.argtypes=[c.c_void_p]; u.IsWindow.argtypes=[c.c_void_p]
u.IsWindowVisible.argtypes=[c.c_void_p]
u.GetWindowLongW.argtypes=[c.c_void_p,c.c_int]
u.GetWindowThreadProcessId.argtypes=[c.c_void_p,c.POINTER(c.c_ulong)]
k.OpenProcess.argtypes=[c.c_ulong,c.c_bool,c.c_ulong]; k.OpenProcess.restype=c.c_void_p
k.TerminateProcess.argtypes=[c.c_void_p,c.c_uint]; k.CloseHandle.argtypes=[c.c_void_p]
cbtype=c.WINFUNCTYPE(c.c_bool,c.c_void_p,c.c_void_p)

def windows():
    out=[]
    @cbtype
    def visit(hwnd,_):
        title=c.create_unicode_buffer(256); cls=c.create_unicode_buffer(256); pid=c.c_ulong()
        u.GetWindowTextW(c.c_void_p(hwnd),title,256);u.GetClassNameW(c.c_void_p(hwnd),cls,256)
        u.GetWindowThreadProcessId(hwnd,c.byref(pid))
        if title.value=='aniki-remake-gpui' or cls.value=='AnikiSessionBlack': out.append((hwnd,pid.value,cls.value))
        return True
    u.EnumWindows(visit,None)
    return out

def events():
    if not log.exists(): return []
    return [json.loads(s) for s in log.read_text(encoding='utf-8').splitlines(keepends=True) if s.endswith('\n')]

def wait(predicate,timeout=20):
    end=time.monotonic()+timeout
    while time.monotonic()<end:
        rows=events()
        if predicate(rows): return rows
        if owner is not None and owner.poll() is not None:
            raise AssertionError(f'Launcher exited unexpectedly: {owner.returncode}; last events: {rows[-5:]}')
        time.sleep(.05)
    raise AssertionError('Timed out: '+str(events()[-3:]))

def page(rows):
    frames=[json.loads(e['detail']) for e in rows if e['event']=='motion_frame']
    return frames[-1]['page'] if frames else None

results=[]
artwork={}
if '--artwork-from-cache' in sys.argv:
    cached=json.loads((root/'cache/library.json').read_text(encoding='utf-8-sig'))
    source=next(g for g in cached['games'] if all(g.get(k) and Path(g[k]).is_file() for k in ('background','logo','cover')))
    artwork={k:source[k] for k in ('background','logo','cover')}
for family,mode in [('xbox','crash'),('xbox','missing'),('xbox','exit'),('xbox','close'),('xbox','game-crash'),('xbox','held-back'),('xbox','held-accept'),('ps','exit'),('ps','held-back'),('ps','held-accept')]:
    if log.exists(): log.rename(app/'artifacts'/f'session-{time.time_ns()}.jsonl')
    marker=app/f'{family}-{mode}.hwnd'
    marker.unlink(missing_ok=True); Path(str(marker)+'.transition').unlink(missing_ok=True)
    fixture_mode='exit' if mode.startswith('held-') else mode
    game=dict(id='fixture',name='Lifecycle fixture',platform='PC',launch=dict(kind='process',executable=str(app/'WindowFixture.exe'),arguments=f'{fixture_mode} "{marker}"',working_dir=str(app),install_dir=str(app)))
    game.update(artwork)
    if mode=='missing': game['launch']['executable']=str(app/'MissingGame.exe')
    library=dict(games=[game],exported_utc='',platform_art={})
    def save(): (app/'cache/library.json').write_text(json.dumps(library))
    save()
    pad=vg.VDS4Gamepad() if family=='ps' else vg.VX360Gamepad()
    buttons=({'accept':vg.DS4_BUTTONS.DS4_BUTTON_CROSS,'back':vg.DS4_BUTTONS.DS4_BUTTON_CIRCLE,'menu':vg.DS4_BUTTONS.DS4_BUTTON_OPTIONS} if family=='ps' else {'accept':vg.XUSB_BUTTON.XUSB_GAMEPAD_A,'back':vg.XUSB_BUTTON.XUSB_GAMEPAD_B,'menu':vg.XUSB_BUTTON.XUSB_GAMEPAD_START,'down':vg.XUSB_BUTTON.XUSB_GAMEPAD_DPAD_DOWN})
    def press(action,hold=.09):
        if family=='ps' and action=='down': pad.directional_pad(direction=vg.DS4_DPAD_DIRECTIONS.DS4_BUTTON_DPAD_SOUTH)
        else: pad.press_button(button=buttons[action])
        pad.update();time.sleep(hold);pad.reset();pad.update();time.sleep(.35)
    owner=None; followers=[]; handle=None
    output=open(app/'artifacts'/f'stderr-{family}-{mode}.log','wb')
    try:
        owner=subprocess.Popen([str(app/'aniki-remake-gpui.exe'),'--trace-motion','--trace-memory'],cwd=app,stdout=output,stderr=subprocess.STDOUT)
        wait(lambda r:any(e['event']=='startup_focus' for e in r))
        own=[w for w in windows() if w[1]==owner.pid]
        assert len(own)==1,own
        hwnd=own[0][0]
        wait(lambda _:u.GetForegroundWindow()==hwnd)
        expected_kind='PlayStation' if family=='ps' else 'Xbox'
        wait(lambda r:any(e['event']=='controller' and f'connected=true;kind={expected_kind}' in e['detail'] for e in r))
        # Enumeration is asynchronous; the app must observe a neutral sample
        # before this test intentionally presses its first button.
        pad.reset();pad.update();time.sleep(.2)
        assert not any(w[2]=='AnikiSessionBlack' for w in windows())
        if mode=='crash':
            follower=subprocess.Popen([str(app/'aniki-remake-gpui.exe')],cwd=app);followers.append(follower)
            wait(lambda r:any(e['event']=='session_activated' for e in r))
            owner.terminate();owner.wait(timeout=5)
            follower.wait(timeout=5);assert follower.returncode==0
            assert not u.IsWindow(hwnd)
            results.append(dict(controller=family,mode=mode,passed=True,followers_released=True))
            print(json.dumps(results[-1]),flush=True)
            continue
        press('accept',.7)
        assert page(events())=='details'
        if mode=='missing':
            press('accept')
            wait(lambda r:any(e['event']=='launch_failed' for e in r) and page(r)=='error')
            wait(lambda _:u.GetForegroundWindow()==hwnd)
            assert not u.IsIconic(hwnd)
            press('back');assert page(events())=='details'
        for cycle in range(0 if mode=='missing' else 2 if mode=='exit' else 1):
            press('accept')
            rows=wait(lambda r:sum(e['event']=='game_focused' for e in r)==cycle+1)
            assert owner.poll() is None and u.IsWindow(hwnd) and not u.IsIconic(hwnd)
            assert u.IsWindowVisible(hwnd)
            assert not (u.GetWindowLongW(hwnd,-20)&0x8),'Launcher stayed topmost over game'
            game_hwnd=int(marker.read_text())
            assert u.GetForegroundWindow()==game_hwnd
            launches=[e for e in rows if e['event'] in ('status_presented','game_started')]
            assert [e['event'] for e in launches[-2:]]==['status_presented','game_started']
            wait(lambda r:page(r)=='running')
            if mode.startswith('held-'):
                pad.press_button(button=buttons[mode.removeprefix('held-')]);pad.update()
            assert not any(w[2]=='AnikiSessionBlack' for w in windows())
            assert sum(e['event']=='snapshot_loaded' for e in events())==1
            if mode=='close':
                game_hwnd=int(marker.read_text()); pid=c.c_ulong();u.GetWindowThreadProcessId(game_hwnd,c.byref(pid))
                handle=k.OpenProcess(0x100001,False,pid.value);assert handle
                wait(lambda _:Path(str(marker)+'.transition').exists())
                time.sleep(2.5)
                assert not any(e['event']=='game_ended' for e in events()),'Window close falsely ended process tracking'
                assert page(events())=='running' and u.IsWindowVisible(hwnd) and not u.IsIconic(hwnd)
                follower=subprocess.Popen([str(app/'aniki-remake-gpui.exe')],cwd=app);followers.append(follower)
                wait(lambda r:any(e['event']=='session_activated' for e in r))
                assert not u.IsIconic(hwnd) and u.GetForegroundWindow()==hwnd
                press('back'); assert page(events())=='details'
                press('back'); assert page(events())=='home'
                # Returning to the library does not terminate the game or forget tracking.
                assert not any(e['event']=='game_ended' for e in events())
                k.TerminateProcess(handle,0); k.CloseHandle(handle);handle=None
            expected='home' if mode=='close' else 'details'
            rows=wait(lambda r:sum(e['event']=='return_focus' for e in r)==cycle+1 and page(r)==expected)
            if mode.startswith('held-'):
                time.sleep(.8)
                assert page(events())=='details','Held game-exit input skipped Details'
                assert sum(e['event']=='launch_requested' for e in events())==1,'Held confirm relaunched game'
                pad.reset();pad.update();time.sleep(.2)
            wait(lambda _:u.GetForegroundWindow()==hwnd)
            assert u.IsWindow(hwnd) and not u.IsIconic(hwnd) and u.GetForegroundWindow()==hwnd
            assert [w[0] for w in windows() if w[1]==owner.pid]==[hwnd]
            assert sum(e['event']=='snapshot_loaded' for e in rows)==1
        # Duplicate invocation while browsing activates this exact window.
        n=sum(e['event']=='session_activated' for e in events())
        follower=subprocess.Popen([str(app/'aniki-remake-gpui.exe')],cwd=app);followers.append(follower)
        wait(lambda r:sum(e['event']=='session_activated' for e in r)>n)
        assert [w[0] for w in windows() if w[1]==owner.pid]==[hwnd]
        if page(events())=='details': press('back')
        assert page(events())=='home'
        press('menu')
        for _ in range(3):press('down')
        press('accept');owner.wait(timeout=5);assert owner.returncode==0
        for f in followers:f.wait(timeout=5);assert f.returncode==0
        assert not u.IsWindow(hwnd)
        results.append(dict(controller=family,mode=mode,passed=True,same_process=True,same_window=True,no_black_cover=True))
        print(json.dumps(results[-1]),flush=True)
    finally:
        pad.reset();pad.update()
        if handle:k.TerminateProcess(handle,0);k.CloseHandle(handle)
        if owner and owner.poll() is None:owner.terminate();owner.wait(timeout=5)
        for f in followers:
            if f.poll() is None:f.terminate();f.wait(timeout=5)
        output.close()
        time.sleep(.5)
        (app.parent/'results.json').write_text(json.dumps(results,indent=2))
