"""Retained-window lifecycle regression: real windows, virtual Xbox and DS4.
No personal library is needed. Owns only artifacts/retained-window/fixture.
"""
import argparse, ctypes as c, hashlib, json, shutil, subprocess, sys, time
from pathlib import Path
root=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(root/'.tools/python'))
import vgamepad as vg
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--exe',type=Path,default=root/'target/release/aniki-remake-gpui.exe')
parser.add_argument('--expected-sha256',required=True,help='Hash recorded after the intended build')
parser.add_argument('--label',default='candidate')
parser.add_argument('--modes',help='Comma-separated mode or controller:mode selections')
parser.add_argument('--artwork-from-cache',action='store_true')
parser.add_argument('--foreground-setup',action='store_true',help='Allow explicit initial activation; does not certify automatic startup')
parser.add_argument('--fixture-source',type=Path,default=root/'tests/WindowFixture.cs',help='Explicit scratch fixture variant for negative-control validation')
parser.add_argument('--startup-diagnostic',action='store_true',help='Read-only startup snapshots; never certifies a passing case')
args=parser.parse_args()
binary=args.exe.resolve()
digest=hashlib.sha256(binary.read_bytes()).hexdigest()
if digest.lower()!=args.expected_sha256.lower(): raise SystemExit('Executable does not match the intended build hash')
app=root/'artifacts/retained-window'/f'run-{time.time_ns()}'/'fixture'
app.mkdir(parents=True,exist_ok=True)
for d in ('cache','artifacts'): (app/d).mkdir(exist_ok=True)
revision=subprocess.check_output(['git','-c',f'safe.directory={root.as_posix()}','rev-parse','HEAD'],cwd=root,text=True).strip()
dirty=subprocess.check_output(['git','-c',f'safe.directory={root.as_posix()}','status','--porcelain'],cwd=root,text=True)
report=dict(label=args.label,exe=str(binary),sha256=digest,head=revision,worktree_status=dirty,status='setup',cases=[])
report['initial_foreground']='explicit setup' if args.foreground_setup else 'automatic'
result_path=app.parent/'results.json'
result_path.write_text(json.dumps(report,indent=2))
print(f'Results: {result_path}',flush=True)
shutil.copy2(binary,app/'aniki-remake-gpui.exe')
assert hashlib.sha256((app/'aniki-remake-gpui.exe').read_bytes()).hexdigest()==digest
shutil.copy2(root/'SDL2.dll',app/'SDL2.dll')
shutil.copytree(root/'assets',app/'assets',dirs_exist_ok=True)
report['fixture_source']=str(args.fixture_source.resolve())
report['fixture_sha256']=hashlib.sha256(args.fixture_source.read_bytes()).hexdigest()
subprocess.run([r'C:\Windows\Microsoft.NET\Framework64\v4.0.30319\csc.exe','/nologo','/target:winexe','/reference:System.Windows.Forms.dll','/reference:System.Drawing.dll','/out:'+str(app/'WindowFixture.exe'),str(args.fixture_source)],check=True)
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

def visible_order():
    order=[]
    @cbtype
    def visit(hwnd,_):
        if u.IsWindowVisible(hwnd): order.append(hwnd)
        return True
    u.EnumWindows(visit,None)
    return order

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

results=report['cases']
artwork={}
if args.artwork_from_cache:
    cached=json.loads((root/'cache/library.json').read_text(encoding='utf-8-sig'))
    source=next(g for g in cached['games'] if all(g.get(k) and Path(g[k]).is_file() for k in ('background','logo','cover')))
    artwork={k:source[k] for k in ('background','logo','cover')}
cases=[('xbox','immediate'),('xbox','late'),('xbox','crash'),('xbox','missing'),('xbox','delayed'),('xbox','fullscreen'),('xbox','noactivate'),('xbox','exit'),('xbox','minimized-return'),('xbox','close'),('xbox','game-crash'),('xbox','held-back'),('xbox','held-accept'),('ps','exit'),('ps','held-back'),('ps','held-accept')]
cases.append(('xbox','pending'))
cases.append(('xbox','cancel-before-dispatch'))
if args.modes:
    requested=set(args.modes.split(','))
    available={mode for _,mode in cases}|{f'{family}:{mode}' for family,mode in cases}
    if requested-available: raise SystemExit(f'Unknown cases: {requested-available}')
    cases=[(family,mode) for family,mode in cases if mode in requested or f'{family}:{mode}' in requested]
else:
    # This explicit real-game lane is not part of the personal-library-free suite.
    cases=[(family,mode) for family,mode in cases if mode!='pending']
report['requested_cases']=[f'{family}:{mode}' for family,mode in cases]
report['status']='running'
for family,mode in cases:
    if log.exists(): log.rename(app/'artifacts'/f'session-{time.time_ns()}.jsonl')
    marker=app/f'{family}-{mode}.hwnd'
    for suffix in ('','.transition','.ready'): Path(str(marker)+suffix).unlink(missing_ok=True)
    fixture_mode='exit' if mode.startswith('held-') or mode=='minimized-return' else mode
    game=dict(id='fixture',name='Lifecycle fixture',platform='PC',launch=dict(kind='process',executable=str(app/'WindowFixture.exe'),arguments=f'{fixture_mode} "{marker}"',working_dir=str(app),install_dir=str(app)))
    game.update(artwork)
    if mode=='pending':
        # A valid but empty directory exercises the accepted never-matching
        # Steam contract. The real game is outside it and must never be killed.
        empty=app/'never-matching';empty.mkdir(exist_ok=True)
        cached=json.loads((root/'cache/library.json').read_text(encoding='utf-8-sig'))
        dave=next(g for g in cached['games'] if g['name']=='DAVE THE DIVER')
        game['launch']=dict(dave['launch'],install_dir=str(empty))
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
    owner=None; followers=[]; handle=None; sentinels=[]
    output=open(app/'artifacts'/f'stderr-{family}-{mode}.log','wb')
    try:
        owner=subprocess.Popen([str(app/'aniki-remake-gpui.exe'),'--trace-motion','--trace-memory'],cwd=app,stdout=output,stderr=subprocess.STDOUT)
        wait(lambda r:any(e['event']=='startup_focus' for e in r))
        own=[w for w in windows() if w[1]==owner.pid]
        assert len(own)==1,own
        hwnd=own[0][0]
        if args.startup_diagnostic:
            class GuiInfo(c.Structure):
                _fields_=[('size',c.c_ulong),('flags',c.c_ulong),('active',c.c_void_p),('focus',c.c_void_p),('capture',c.c_void_p),('menu',c.c_void_p),('move',c.c_void_p),('caret',c.c_void_p),('rect',c.c_long*4)]
            u.GetGUIThreadInfo.argtypes=[c.c_ulong,c.POINTER(GuiInfo)]
            u.OpenInputDesktop.argtypes=[c.c_ulong,c.c_bool,c.c_ulong];u.OpenInputDesktop.restype=c.c_void_p
            u.GetUserObjectInformationW.argtypes=[c.c_void_p,c.c_int,c.c_void_p,c.c_ulong,c.POINTER(c.c_ulong)]
            u.CloseDesktop.argtypes=[c.c_void_p]
            observations=[]
            for delay in (0,.5,1,3):
                time.sleep(delay)
                foreground=u.GetForegroundWindow();pid=c.c_ulong()
                u.GetWindowThreadProcessId(foreground,c.byref(pid))
                title=c.create_unicode_buffer(512);u.GetWindowTextW(c.c_void_p(foreground),title,512)
                sid=c.c_ulong();k.ProcessIdToSessionId(owner.pid,c.byref(sid))
                tid=u.GetWindowThreadProcessId(c.c_void_p(hwnd),None);gui=GuiInfo();gui.size=c.sizeof(gui)
                gui_ok=u.GetGUIThreadInfo(tid,c.byref(gui))
                desktop=u.OpenInputDesktop(0,False,1);name=c.create_unicode_buffer(512);size=c.c_ulong()
                desktop_ok=bool(desktop and u.GetUserObjectInformationW(desktop,2,name,c.sizeof(name),c.byref(size)))
                if desktop:u.CloseDesktop(desktop)
                observations.append(dict(time=time.time(),launcher_pid=owner.pid,launcher_hwnd=hwnd,foreground_hwnd=foreground,foreground_pid=pid.value,foreground_title=title.value,iconic=bool(u.IsIconic(hwnd)),topmost=bool(u.GetWindowLongW(hwnd,-20)&8),gui_ok=bool(gui_ok),gui_active=gui.active,gui_focus=gui.focus,input_desktop_available=desktop_ok,input_desktop_name=name.value,launcher_session=sid.value,active_console_session=k.WTSGetActiveConsoleSessionId()))
            report['startup_diagnostic']=dict(user=subprocess.check_output(['whoami'],text=True).strip(),observations=observations)
            report['status']='diagnostic-only'
            continue
        if args.foreground_setup: print(f'Foreground setup: pid={owner.pid} hwnd={hwnd}',flush=True)
        wait(lambda _:u.GetForegroundWindow()==hwnd,timeout=90 if args.foreground_setup else 20)
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
        wait(lambda r:page(r)=='home')
        press('accept',.7)
        wait(lambda r:page(r)=='details')
        if mode=='cancel-before-dispatch':
            # Both edges are sampled together: Accept queues the render, Back
            # cancels in that input batch before the next rendered callback.
            pad.press_button(button=buttons['accept']);pad.press_button(button=buttons['back']);pad.update()
            time.sleep(.12);pad.reset();pad.update()
            wait(lambda r:any(e['event']=='launch_requested' for e in r) and page(r)=='details')
            time.sleep(.6)
            assert not any(e['event'] in ('status_presented','game_started','launch_failed') for e in events()),'Cancelled launch dispatched'
            assert not marker.exists(),'Cancelled fixture was created'
            assert not (u.GetWindowLongW(hwnd,-20)&0x8),'Pre-dispatch cancel retained topmost'
        if mode=='pending':
            restores_before_cancel=sum(e['event']=='native_restored' for e in events())
            for attempt in range(2):
                press('accept')
                wait(lambda r:sum(e['event']=='status_presented' for e in r)==attempt+1)
                time.sleep(.5)
                assert not any(e['event']=='game_started' for e in events())
                press('back')
                assert page(events())=='details'
                assert not (u.GetWindowLongW(hwnd,-20)&0x8),'Pending cancellation retained topmost'
            time.sleep(3)
            assert not any(e['event'] in ('game_started','focus_released','launch_failed') for e in events()),'Abandoned worker delivered stale lifecycle events'
            assert sum(e['event']=='native_restored' for e in events())==restores_before_cancel,'Abandoned worker restored focus'
            report['pending_note']='Two real Steam dispatches, empty monitored directory; Dave is untracked and requires normal external close.'
        if mode=='missing':
            press('accept')
            wait(lambda r:any(e['event']=='launch_failed' for e in r) and page(r)=='error')
            wait(lambda _:u.GetForegroundWindow()==hwnd)
            assert not u.IsIconic(hwnd)
            assert not (u.GetWindowLongW(hwnd,-20)&0x8),'Failed launch retained topmost'
            press('back');assert page(events())=='details'
        for cycle in range(0 if mode in ('missing','pending','cancel-before-dispatch') else 2 if mode=='exit' else 1):
            assert page(events())=='details','Play requires the Details page'
            press('accept')
            if mode=='immediate':
                wait(lambda r:any(e['event']=='game_ended' for e in r) and page(r)=='details',timeout=8)
                assert not any(e['event']=='launch_failed' for e in events())
                assert not (u.GetWindowLongW(hwnd,-20)&0x8),'Immediate exit retained topmost'
                continue
            if mode=='late':
                wait(lambda r:any(e['event']=='readiness_timeout' for e in r),timeout=94)
                assert not (u.GetWindowLongW(hwnd,-20)&0x8),'Expiry retained topmost'
            if mode=='delayed':
                wait(lambda r:any(e['event']=='game_started' for e in r))
                assert u.GetWindowLongW(hwnd,-20)&0x8,'Launcher uncovered during splash'
                # A stale marker cannot pass this run, and the assertion is made
                # throughout the splash rather than only after eventual handoff.
                def splash_ready(rows):
                    if Path(str(marker)+'.ready').exists(): return True
                    assert not any(e['event']=='focus_released' for e in rows),'Handoff preceded fixture readiness'
                    return False
                wait(splash_ready)
            rows=wait(lambda r:sum(e['event']=='focus_released' for e in r)==cycle+1)
            if mode=='delayed':
                assert Path(str(marker)+'.ready').exists(),'Splash was mistaken for ready game'
            assert owner.poll() is None and u.IsWindow(hwnd) and not u.IsIconic(hwnd)
            assert u.IsWindowVisible(hwnd)
            assert not (u.GetWindowLongW(hwnd,-20)&0x8),'Launcher stayed topmost over game'
            game_hwnd=int(marker.read_text())
            assert u.GetForegroundWindow()==game_hwnd
            order=visible_order()
            assert order.index(game_hwnd)<order.index(hwnd),'Foreground game is occluded by launcher'
            launches=[e for e in rows if e['event'] in ('status_presented','game_started')]
            assert [e['event'] for e in launches[-2:]]==['status_presented','game_started']
            wait(lambda r:page(r)=='running')
            if mode=='minimized-return':
                u.ShowWindow(c.c_void_p(hwnd),6)
                wait(lambda _:u.IsIconic(hwnd))
                sentinel_marker=app/f'{family}-{mode}-sentinel.hwnd'
                sentinel_marker.unlink(missing_ok=True)
                sentinels.append(subprocess.Popen([str(app/'WindowFixture.exe'),'sentinel',str(sentinel_marker)],cwd=app))
                wait(lambda _:sentinel_marker.exists())
                sentinel_hwnd=int(sentinel_marker.read_text())
                wait(lambda _:u.GetForegroundWindow()==sentinel_hwnd)
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
                sentinel_marker=app/f'{family}-{mode}-sentinel.hwnd'
                sentinel_marker.unlink(missing_ok=True)
                sentinels.append(subprocess.Popen([str(app/'WindowFixture.exe'),'sentinel',str(sentinel_marker)],cwd=app))
                wait(lambda _:sentinel_marker.exists())
                sentinel_hwnd=int(sentinel_marker.read_text())
                wait(lambda _:u.GetForegroundWindow()==sentinel_hwnd)
                restores_before_stop=sum(e['event']=='native_restored' for e in events())
                k.TerminateProcess(handle,0); k.CloseHandle(handle);handle=None
            expected='home' if mode=='close' else 'details'
            rows=wait(lambda r:sum(e['event']=='return_focus' for e in r)==cycle+1 and page(r)==expected)
            if mode.startswith('held-'):
                time.sleep(.8)
                assert page(events())=='details','Held game-exit input skipped Details'
                assert sum(e['event']=='launch_requested' for e in events())==1,'Held confirm relaunched game'
                pad.reset();pad.update();time.sleep(.2)
            if mode=='close':
                time.sleep(.5)
                assert u.GetForegroundWindow()==sentinel_hwnd,'Manual abandon reclaimed foreground'
                assert sum(e['event']=='native_restored' for e in events())==restores_before_stop,'Manual abandon scheduled native restore'
            else:
                wait(lambda _:u.GetForegroundWindow()==hwnd)
                assert u.IsWindow(hwnd) and not u.IsIconic(hwnd) and u.GetForegroundWindow()==hwnd
            assert not (u.GetWindowLongW(hwnd,-20)&0x8),'Return retained topmost'
            assert [w[0] for w in windows() if w[1]==owner.pid]==[hwnd]
            assert sum(e['event']=='snapshot_loaded' for e in rows)==1
        # Duplicate invocation while browsing activates this exact window.
        n=sum(e['event']=='session_activated' for e in events())
        follower=subprocess.Popen([str(app/'aniki-remake-gpui.exe')],cwd=app);followers.append(follower)
        wait(lambda r:sum(e['event']=='session_activated' for e in r)>n)
        # Activation is deferred; the diagnostic precedes actual foreground.
        wait(lambda _:u.GetForegroundWindow()==hwnd)
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
    except BaseException as error:
        results.append(dict(controller=family,mode=mode,passed=False,error=f'{type(error).__name__}: {error}'))
        report['status']='failed'
        raise
    finally:
        pad.reset();pad.update()
        if handle:k.TerminateProcess(handle,0);k.CloseHandle(handle)
        if owner and owner.poll() is None:owner.terminate();owner.wait(timeout=5)
        for f in followers:
            if f.poll() is None:f.terminate();f.wait(timeout=5)
        for sentinel in sentinels:
            if sentinel.poll() is None:sentinel.terminate();sentinel.wait(timeout=5)
        output.close()
        time.sleep(.5)
        result_path.write_text(json.dumps(report,indent=2))
if not args.startup_diagnostic:report['status']='passed'
result_path.write_text(json.dumps(report,indent=2))
