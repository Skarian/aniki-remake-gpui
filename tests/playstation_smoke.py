"""Exercise a native virtual DualShock 4, without Xbox translation or new drivers.
Run with the launcher foreground. Use --hold to keep PS prompts visible for review.
"""
import sys, time, json
from pathlib import Path
root=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(root/'.tools/python'))
import vgamepad as vg
log=root/'artifacts/session.jsonl'
before=len(log.read_text(encoding='utf-8').splitlines())
pad=vg.VDS4Gamepad()
time.sleep(2)
def press(button):
    pad.press_button(button=button);pad.update();time.sleep(.12)
    pad.release_button(button=button);pad.update();time.sleep(.22)
def direction(value):
    pad.directional_pad(direction=value);pad.update();time.sleep(.12)
    pad.directional_pad(direction=vg.DS4_DPAD_DIRECTIONS.DS4_BUTTON_DPAD_NONE);pad.update();time.sleep(.22)
d=vg.DS4_DPAD_DIRECTIONS;b=vg.DS4_BUTTONS
direction(d.DS4_BUTTON_DPAD_EAST)
direction(d.DS4_BUTTON_DPAD_WEST)
press(b.DS4_BUTTON_SHOULDER_RIGHT)
press(b.DS4_BUTTON_SHOULDER_LEFT)
press(b.DS4_BUTTON_OPTIONS)
direction(d.DS4_BUTTON_DPAD_SOUTH)
press(b.DS4_BUTTON_CROSS)
press(b.DS4_BUTTON_CROSS)
press(b.DS4_BUTTON_CIRCLE)
direction(d.DS4_BUTTON_DPAD_NORTH)
direction(d.DS4_BUTTON_DPAD_SOUTH)
press(b.DS4_BUTTON_CIRCLE)
press(b.DS4_BUTTON_SQUARE)
press(b.DS4_BUTTON_SQUARE)
press(b.DS4_BUTTON_TRIANGLE)
press(b.DS4_BUTTON_TRIANGLE)
pad.reset();pad.update();time.sleep(.3)
events=[json.loads(s) for s in log.read_text(encoding='utf-8').splitlines()[before:]]
inputs=[e['detail'] for e in events if e['event']=='input']
expected=['Right','Left','NextPlatform','PreviousPlatform','Menu','Down','Accept','Accept','Back','Up','Down','Back','Music','Music','Video','Video']
native_ps=any(e['event']=='controller' and 'kind=PlayStation' in e['detail'] for e in events)
result={'expected':expected,'observed':inputs,'native_playstation_detected':native_ps,'passed':inputs==expected and native_ps}
(root/'artifacts/playstation-test.json').write_text(json.dumps(result,indent=2),encoding='utf-8')
print(json.dumps(result,indent=2),flush=True)
assert result['passed'], 'Native PS input sequence or controller identity did not match.'
if '--hold' in sys.argv: time.sleep(45)

