"""Exercise the real XInput path through the installed ViGEm driver.
Run only while aniki-remake-gpui is foreground. Never installs or changes drivers.
"""
import sys, time, json
from pathlib import Path
root=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(root/'.tools/python'))
import vgamepad as vg
log=root/'artifacts/session.jsonl'
before=len(log.read_text(encoding='utf-8').splitlines())
pad=vg.VX360Gamepad()
time.sleep(2)
def press(button,hold=.1):
    pad.press_button(button=button);pad.update();time.sleep(hold)
    pad.release_button(button=button);pad.update();time.sleep(.2)
sequence=[vg.XUSB_BUTTON.XUSB_GAMEPAD_DPAD_RIGHT,vg.XUSB_BUTTON.XUSB_GAMEPAD_DPAD_RIGHT,
 vg.XUSB_BUTTON.XUSB_GAMEPAD_DPAD_LEFT,vg.XUSB_BUTTON.XUSB_GAMEPAD_RIGHT_SHOULDER,
 vg.XUSB_BUTTON.XUSB_GAMEPAD_RIGHT_SHOULDER,vg.XUSB_BUTTON.XUSB_GAMEPAD_LEFT_SHOULDER,
 vg.XUSB_BUTTON.XUSB_GAMEPAD_START,vg.XUSB_BUTTON.XUSB_GAMEPAD_DPAD_DOWN,
 vg.XUSB_BUTTON.XUSB_GAMEPAD_A,vg.XUSB_BUTTON.XUSB_GAMEPAD_A,vg.XUSB_BUTTON.XUSB_GAMEPAD_B]
for button in sequence:press(button)
pad.reset();pad.update();time.sleep(.3)
events=[json.loads(s) for s in log.read_text(encoding='utf-8').splitlines()[before:]]
inputs=[e['detail'] for e in events if e['event']=='input']
expected=['Right','Right','Left','NextPlatform','NextPlatform','PreviousPlatform','Menu','Down','Accept','Accept','Back']
result={'expected':expected,'observed':inputs,'passed':inputs==expected,'selection_events':[e for e in events if e['event']=='selection']}
(root/'artifacts/controller-test.json').write_text(json.dumps(result,indent=2),encoding='utf-8')
print(json.dumps(result,indent=2))
assert result['passed'], 'Controller input sequence differed; check aniki-remake-gpui focus.'

if "--hold" in sys.argv: time.sleep(60)
