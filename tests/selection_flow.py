"""Real controller navigation on --preview-screen home --preview-game-index 0
--trace-motion. Preview stages launch; it never starts a game. Pass xbox or ps.
"""
import json
import sys
import time
from pathlib import Path

root = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(root / '.tools/python'))
import vgamepad as vg

family = sys.argv[1] if len(sys.argv) > 1 else 'xbox'
app_root = Path(sys.argv[2]).resolve() if len(sys.argv) > 2 else root
log = app_root / 'artifacts/session.jsonl'
def events():
    return [json.loads(line) for line in log.read_text(encoding='utf-8').splitlines()]

initial = events()
pid = next(e['pid'] for e in reversed(initial) if e['event'] == 'motion_frame')
start = next(json.loads(e['detail']) for e in reversed(initial) if e['event'] == 'motion_frame')
assert start['page'] == 'home' and not start['menu'] and start['selected'] == 0, 'Start on Home, first game, menu closed'
before = len(initial)
checks = []
if family == 'ps':
    pad = vg.VDS4Gamepad()
    b = vg.DS4_BUTTONS
    buttons = {'accept': b.DS4_BUTTON_CROSS, 'back': b.DS4_BUTTON_CIRCLE,
               'menu': b.DS4_BUTTON_OPTIONS, 'next': b.DS4_BUTTON_SHOULDER_RIGHT}
else:
    pad = vg.VX360Gamepad()
    b = vg.XUSB_BUTTON
    buttons = {'accept': b.XUSB_GAMEPAD_A, 'back': b.XUSB_GAMEPAD_B,
               'menu': b.XUSB_GAMEPAD_START, 'next': b.XUSB_GAMEPAD_RIGHT_SHOULDER,
               'right': b.XUSB_GAMEPAD_DPAD_RIGHT, 'left': b.XUSB_GAMEPAD_DPAD_LEFT}
time.sleep(1)

def press(action, settle=.45, hold=.09):
    if family == 'ps' and action in ('left', 'right'):
        d = vg.DS4_DPAD_DIRECTIONS
        pad.directional_pad(direction=d.DS4_BUTTON_DPAD_EAST if action == 'right' else d.DS4_BUTTON_DPAD_WEST)
        pad.update(); time.sleep(hold)
        pad.directional_pad(direction=d.DS4_BUTTON_DPAD_NONE)
    else:
        pad.press_button(button=buttons[action]); pad.update(); time.sleep(hold)
        pad.release_button(button=buttons[action])
    pad.update(); time.sleep(settle)

def check(name, page, menu=False, action=None):
    own = [e for e in events()[before:] if e['pid'] == pid]
    frame = next(json.loads(e['detail']) for e in reversed(own) if e['event'] == 'motion_frame')
    ok = frame['page'] == page and frame['menu'] == menu
    if action is not None:
        ok = ok and frame['details_action'] == action
    checks.append({'check': name, 'passed': ok, 'page': frame['page'], 'menu': frame['menu']})
    assert ok, checks[-1]

try:
    press('accept', hold=.75)
    check('Held Select opens Details without launching', 'details', action=0)
    press('menu')
    check('Menu unavailable in Details', 'details')
    press('right')
    check('Watch video focus', 'details', action=1)
    press('accept', settle=3)
    check('Watch video opens fullscreen', 'video')
    own = [e for e in events()[before:] if e['pid'] == pid]
    audio = any(e['event'] == 'trailer_audio' and 'playing' in e['detail'] for e in own)
    checks.append({'check': 'Trailer PCM queued to SDL audio device', 'passed': audio})
    assert audio, 'Trailer audio did not start'
    press('menu')
    check('Menu unavailable in video', 'video')
    press('back')
    check('Video returns to Details', 'details', action=1)
    press('left')
    press('accept')
    check('Play starts staged launch from Details', 'launching')
    press('menu')
    check('Menu unavailable while launching', 'launching')
    press('back')
    check('Staged launch returns Home', 'home')
    for _ in range(3):
        press('next')
    check('Empty favorites', 'empty')
    own = [json.loads(e['detail']) for e in events()[before:] if e['pid'] == pid and e['event'] == 'motion_frame']
    stable = all(f['page_progress'] == 1 for f in own if f['page'] == 'empty')
    checks.append({'check': 'Empty collection has no whole-page animation', 'passed': stable})
    assert stable
    press('accept')
    check('Browse games returns Home', 'home')
    press('menu')
    check('Menu works on Home', 'home', menu=True)
    press('back')
    check('Menu closes on Home', 'home')
finally:
    pad.reset(); pad.update()
    result = {'family': family, 'pid': pid, 'checks': checks, 'passed': len(checks) == 15 and all(c['passed'] for c in checks)}
    (app_root / f'artifacts/selection-flow-{family}.json').write_text(json.dumps(result, indent=2), encoding='utf-8')
    print(json.dumps(result, indent=2))
