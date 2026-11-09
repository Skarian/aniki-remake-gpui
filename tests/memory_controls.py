"""Virtual-controller memory/lifecycle exercises for an isolated preview instance."""
import json, sys, time
from pathlib import Path
root = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(root / '.tools/python'))
import vgamepad as vg
app = Path(sys.argv[1]).resolve()
mode = sys.argv[2]
count = int(sys.argv[3]) if len(sys.argv) > 3 else 15
log = app / 'artifacts/session.jsonl'
initial = [json.loads(s) for s in log.read_text(encoding='utf-8').splitlines()]
pid = next(e['pid'] for e in reversed(initial) if e['event'] == 'snapshot_loaded')
pad = vg.VX360Gamepad()
b = vg.XUSB_BUTTON
time.sleep(.6)
def press(button, settle=.4):
    pad.press_button(button=button); pad.update(); time.sleep(.08)
    pad.release_button(button=button); pad.update(); time.sleep(settle)
try:
    if mode == 'browse':
        for _ in range(count): press(b.XUSB_GAMEPAD_DPAD_RIGHT, .65)
    elif mode == 'media':
        # Start on Home with background video off. Each loop starts/stops one decoder.
        for _ in range(count):
            press(b.XUSB_GAMEPAD_Y, 2)
            press(b.XUSB_GAMEPAD_Y, .4)
    elif mode == 'watch':
        press(b.XUSB_GAMEPAD_A)
        for _ in range(count):
            press(b.XUSB_GAMEPAD_Y, 2)
            press(b.XUSB_GAMEPAD_B)
        press(b.XUSB_GAMEPAD_B)
    elif mode == 'launch':
        press(b.XUSB_GAMEPAD_A)
        press(b.XUSB_GAMEPAD_A)
    elif mode == 'quit':
        press(b.XUSB_GAMEPAD_START)
        for _ in range(3): press(b.XUSB_GAMEPAD_DPAD_DOWN)
        press(b.XUSB_GAMEPAD_A)
    else: raise ValueError(mode)
finally:
    pad.reset(); pad.update()
events = [json.loads(s) for s in log.read_text(encoding='utf-8').splitlines()][len(initial):]
events = [e for e in events if e['pid'] == pid]
if mode == 'browse': assert sum(e['event'] == 'selection' for e in events) == count
if mode == 'media': assert sum(e['event'] == 'input' and e['detail'] == 'Video' for e in events) == count * 2
if mode == 'watch':
    assert sum(e['event'] == 'trailer_opened' for e in events) == count
    assert sum(e['event'] == 'trailer_audio' and 'playing' in e['detail'] for e in events) == count
result = dict(pid=pid, mode=mode, count=count, passed=True)
(app / f'artifacts/memory-controls-{mode}.json').write_text(json.dumps(result), encoding='utf-8')
print(json.dumps(result))
