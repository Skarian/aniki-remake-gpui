"""Real-window regression tests for lingering processes and black-cover input.

Requires Windows, SDL2.dll, a release build, and vgamepad/ViGEm. Owns and cleans
up only its isolated fixture processes. No personal library or game is needed.
"""
import ctypes as c
import json
import shutil
import subprocess
import sys
import time
from pathlib import Path

root = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(root / '.tools/python'))
import vgamepad as vg
app = root / 'artifacts/session-recovery/fixture'
app.mkdir(parents=True, exist_ok=True)
(app / 'cache').mkdir(exist_ok=True)
(app / 'artifacts').mkdir(exist_ok=True)
for name in ('aniki-remake-gpui.exe', 'SDL2.dll'):
    shutil.copy2(root / ('target/release' if name.endswith('.exe') else '.') / name, app / name)
shutil.copytree(root / 'assets', app / 'assets', dirs_exist_ok=True)
subprocess.run([r'C:\Windows\Microsoft.NET\Framework64\v4.0.30319\csc.exe', '/nologo',
                '/target:winexe', '/reference:System.Windows.Forms.dll', '/reference:System.Drawing.dll',
                '/out:' + str(app / 'WindowFixture.exe'), str(root / 'tests/WindowFixture.cs')], check=True)
(app / 'aniki-remake-gpui.json').write_text(json.dumps(dict(
    playnite_root=str(app / 'no-playnite'), fullscreen=True, music=False, video=False)))
log = app / 'artifacts/session.jsonl'
u = c.WinDLL('user32')
u.GetForegroundWindow.restype = c.c_void_p
u.GetWindowThreadProcessId.argtypes = [c.c_void_p, c.POINTER(c.c_ulong)]
k = c.WinDLL('kernel32')
k.OpenProcess.argtypes = [c.c_ulong, c.c_bool, c.c_ulong]
k.OpenProcess.restype = c.c_void_p
k.TerminateProcess.argtypes = [c.c_void_p, c.c_uint]
k.CloseHandle.argtypes = [c.c_void_p]
results = []

def events():
    if not log.exists(): return []
    lines = log.read_text(encoding='utf-8').splitlines(keepends=True)
    return [json.loads(line) for line in lines if line.endswith('\n')]

def wait(predicate, timeout=20):
    until = time.monotonic() + timeout
    while time.monotonic() < until:
        rows = events()
        if predicate(rows): return rows
        time.sleep(.05)
    raise AssertionError('Lifecycle condition timed out: ' + str(events()[-5:]))

cases = [('close', 'xbox'), ('replace', 'xbox'), ('minimize', 'xbox'), ('minimize', 'ps')]
if len(sys.argv) > 1 and sys.argv[1] == 'keyboard': cases = [('minimize', 'keyboard')]
for mode, family in cases:
    marker = app / f'{mode}-{family}.hwnd'
    marker.unlink(missing_ok=True)
    Path(str(marker)+'.transition').unlink(missing_ok=True)
    if log.exists(): log.rename(app / 'artifacts' / f'session-{time.time_ns()}.jsonl')
    game = dict(id='fixture-id', name='Window recovery fixture', platform='PC', launch=dict(
        kind='process', executable=str(app/'WindowFixture.exe'),
        arguments=f'{mode} "{marker}"', working_dir=str(app), install_dir=''))
    (app/'cache/library.json').write_text(json.dumps(dict(games=[game], exported_utc='', platform_art={})))
    pad = vg.VDS4Gamepad() if family == 'ps' else vg.VX360Gamepad()
    accept = vg.DS4_BUTTONS.DS4_BUTTON_CROSS if family == 'ps' else vg.XUSB_BUTTON.XUSB_GAMEPAD_A
    def press():
        pad.press_button(button=accept); pad.update(); time.sleep(.1)
        pad.release_button(button=accept); pad.update(); time.sleep(.35)
    owner = None
    game_handle = None
    try:
        owner = subprocess.Popen([str(app/'aniki-remake-gpui.exe'), '--trace-motion'], cwd=app)
        wait(lambda r: any(e['event']=='ui_visible' for e in r))
        time.sleep(.5)
        press(); press()
        wait(lambda _: marker.exists())
        hwnd = int(marker.read_text())
        pid = c.c_ulong()
        u.GetWindowThreadProcessId(hwnd, c.byref(pid))
        game_handle = k.OpenProcess(0x100001, False, pid.value)
        assert game_handle
        wait(lambda r: any(e['event']=='ui_exited' and 'state=Playing' in e['detail'] for e in r))
        assert u.GetForegroundWindow() == hwnd, 'Fixture was covered during gameplay'
        # Ordinary game input must not recover or reopen the launcher.
        press()
        assert not any(e['event']=='cover_input_recovery' for e in events())
        wait(lambda _: Path(str(marker)+'.transition').exists())
        if mode in ('replace', 'minimize'):
            time.sleep(2.7)
            assert not any(e['event']=='host_game_ended' for e in events()), 'Returned over a live/minimized game'
        if mode == 'replace':
            assert u.GetForegroundWindow() == hwnd, 'Replacement window lost game focus'
            k.TerminateProcess(game_handle, 0)
        elif mode == 'minimize':
            if family == 'keyboard':
                print('KEYBOARD_READY', flush=True)
            else:
                # Hold through the new UI startup. It must not launch again.
                pad.press_button(button=accept); pad.update()
            wait(lambda r: any(e['event']=='cover_input_recovery' for e in r), timeout=35)
        rows = wait(lambda r: sum(e['event']=='ui_visible' for e in r)==2)
        time.sleep(1)
        rows = events()
        returned = next(e['pid'] for e in reversed(rows) if e['event']=='session_reveal')
        frames = [json.loads(e['detail']) for e in rows if e['pid']==returned and e['event']=='motion_frame']
        assert frames and all(f['page']=='details' for f in frames)
        assert frames[-1]['session_fade'] == 1
        assert sum(e['event']=='host_launch' for e in rows)==1, 'Recovery input launched the game again'
        assert any(e['pid']==returned and e['event']=='session_reveal' and e['detail']=='foreground=true;topmost=true' for e in rows)
        if mode == 'close': assert any(e['event']=='game_windows_closed' for e in rows)
        results.append(dict(mode=mode, input=family, passed=True, restored_page='details'))
        print(json.dumps(results[-1]), flush=True)
    finally:
        pad.reset(); pad.update()
        if game_handle:
            k.TerminateProcess(game_handle, 0)
            k.CloseHandle(game_handle)
        if owner and owner.poll() is None:
            owner.terminate(); owner.wait(timeout=5)
        time.sleep(.6)
        (app.parent / ('keyboard-results.json' if family=='keyboard' else 'results.json')).write_text(json.dumps(results, indent=2))
