"""Real virtual-controller motion regression. Requires foreground launcher with
--preview-screen home --preview-game-index 0 --trace-motion. Never launches a game.
"""
import json
from pathlib import Path
import sys
import time

root = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(root / '.tools/python'))
import vgamepad as vg

log = root / 'artifacts/session.jsonl'
initial = [json.loads(line) for line in log.read_text(encoding='utf-8').splitlines()]
latest = next(e for e in reversed(initial) if e['event'] == 'motion_frame')
assert json.loads(latest['detail'])['selected'] == 0, 'Start with --preview-game-index 0'
pid = latest['pid']
before = len(initial)
pad = vg.VX360Gamepad()
time.sleep(1)

def press(button, settle=.4):
    pad.press_button(button=button); pad.update(); time.sleep(.07)
    pad.release_button(button=button); pad.update(); time.sleep(settle)

b = vg.XUSB_BUTTON
for _ in range(9):
    press(b.XUSB_GAMEPAD_DPAD_RIGHT)
# Retarget a moving rail and shrinking/growing tile without waiting for completion.
for _ in range(4):
    press(b.XUSB_GAMEPAD_DPAD_LEFT, .025)
time.sleep(.4)
press(b.XUSB_GAMEPAD_RIGHT_SHOULDER)
press(b.XUSB_GAMEPAD_LEFT_SHOULDER)
press(b.XUSB_GAMEPAD_DPAD_UP)
press(b.XUSB_GAMEPAD_B)
press(b.XUSB_GAMEPAD_A)
press(b.XUSB_GAMEPAD_A)
press(b.XUSB_GAMEPAD_B)
pad.reset(); pad.update(); time.sleep(.4)

events = [json.loads(line) for line in log.read_text(encoding='utf-8').splitlines()[before:]]
events = [e for e in events if e['pid'] == pid]
frames = [{**json.loads(e['detail']), 'ms': e['ms']} for e in events if e['event'] == 'motion_frame']
first_platform = next(e['ms'] for e in events if e['event'] == 'input' and e['detail'] == 'NextPlatform')
navigation = [f for f in frames if f['ms'] < first_platform]
slots = navigation[0]['slots']
first_edge = next(f['ms'] for f in navigation if f['selected'] == slots)
stationary = all(f['rail'] == 0 and f['rail_target'] == 0 for f in navigation if f['ms'] < first_edge)
rail_intermediate = [f for f in navigation if abs(f['rail'] - round(f['rail'])) > .001]
scale_intermediate = [f for f in navigation if f['scale'] is not None and .01 < f['scale'] < .99]
platform_intermediate = [f for f in frames if .01 < f['platform'] < .99]
pages = {p: any(f['page'] == p and .01 < f['page_progress'] < .99 for f in frames)
         for p in ['details', 'launching', 'home']}
result = {
    'pid': pid, 'visible_slots': slots, 'stationary_until_edge': stationary,
    'rail_intermediate_frames': len(rail_intermediate),
    'selection_intermediate_frames': len(scale_intermediate),
    'platform_intermediate_frames': len(platform_intermediate), 'page_transitions': pages,
    'frames': frames,
}
result['passed'] = stationary and bool(rail_intermediate) and bool(scale_intermediate) and bool(platform_intermediate) and all(pages.values())
(root / 'artifacts/motion-test.json').write_text(json.dumps(result, indent=2), encoding='utf-8')
print(json.dumps({k: v for k, v in result.items() if k != 'frames'}, indent=2))
assert result['passed'], 'Motion regression failed; inspect motion-test.json'
