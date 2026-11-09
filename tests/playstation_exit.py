"""Close the foreground launcher using only a virtual DualShock 4."""
import sys,time,json
from pathlib import Path
root=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(root/'.tools/python'))
import vgamepad as vg
log=root/'artifacts/session.jsonl'
before=len(log.read_text(encoding='utf-8').splitlines())
p=vg.VDS4Gamepad();time.sleep(2)
def press(b):
    p.press_button(button=b);p.update();time.sleep(.12)
    p.release_button(button=b);p.update();time.sleep(.2)
press(vg.DS4_BUTTONS.DS4_BUTTON_OPTIONS)
for _ in range(3):
    p.directional_pad(direction=vg.DS4_DPAD_DIRECTIONS.DS4_BUTTON_DPAD_SOUTH);p.update();time.sleep(.12)
    p.directional_pad(direction=vg.DS4_DPAD_DIRECTIONS.DS4_BUTTON_DPAD_NONE);p.update();time.sleep(.2)
press(vg.DS4_BUTTONS.DS4_BUTTON_CROSS)
events=[json.loads(s) for s in log.read_text(encoding='utf-8').splitlines()[before:]]
observed=[e['detail'] for e in events if e['event']=='input']
result={'observed':observed,'passed':observed==['Menu','Down','Down','Down','Accept']}
(root/'artifacts/playstation-exit-test.json').write_text(json.dumps(result,indent=2))
print(json.dumps(result));assert result['passed']
