"""Interactive virtual controller for observed, supervised game QA.
One JSON command per stdin line: {"button":"accept"}, {"stick":[0,1],"seconds":1}.
Never edits emulator/controller profiles. Exiting disconnects only this device.
"""
import json, sys, time
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / '.tools/python'))
import vgamepad as vg
family = sys.argv[1] if len(sys.argv) > 1 else 'xbox'
pad = vg.VDS4Gamepad() if family == 'ps' else vg.VX360Gamepad()
if family == 'ps':
    b=vg.DS4_BUTTONS
    buttons=dict(accept=b.DS4_BUTTON_CROSS,back=b.DS4_BUTTON_CIRCLE,x=b.DS4_BUTTON_SQUARE,y=b.DS4_BUTTON_TRIANGLE,start=b.DS4_BUTTON_OPTIONS,lb=b.DS4_BUTTON_SHOULDER_LEFT,rb=b.DS4_BUTTON_SHOULDER_RIGHT)
else:
    b=vg.XUSB_BUTTON
    buttons=dict(accept=b.XUSB_GAMEPAD_A,back=b.XUSB_GAMEPAD_B,x=b.XUSB_GAMEPAD_X,y=b.XUSB_GAMEPAD_Y,start=b.XUSB_GAMEPAD_START,lb=b.XUSB_GAMEPAD_LEFT_SHOULDER,rb=b.XUSB_GAMEPAD_RIGHT_SHOULDER,up=b.XUSB_GAMEPAD_DPAD_UP,down=b.XUSB_GAMEPAD_DPAD_DOWN,left=b.XUSB_GAMEPAD_DPAD_LEFT,right=b.XUSB_GAMEPAD_DPAD_RIGHT)
print('Controller connected: '+family,flush=True)
try:
    for line in sys.stdin:
        command=json.loads(line)
        if command.get('quit'): break
        for action in command.get('buttons', [command['button']] if 'button' in command else []):
            if family=='ps' and action in ('up','down','left','right'):
                d=vg.DS4_DPAD_DIRECTIONS
                pad.directional_pad(direction={'up':d.DS4_BUTTON_DPAD_NORTH,'down':d.DS4_BUTTON_DPAD_SOUTH,'left':d.DS4_BUTTON_DPAD_WEST,'right':d.DS4_BUTTON_DPAD_EAST}[action])
            else: pad.press_button(button=buttons[action])
        if 'stick' in command:
            x,y=command['stick']; pad.left_joystick_float(x_value_float=x,y_value_float=y)
        if 'trigger' in command:
            pad.right_trigger_float(value_float=max(0., min(1., float(command['trigger']))))
        pad.update();time.sleep(min(command.get('seconds',.12),5));pad.reset();pad.update()
        print('released '+json.dumps(command),flush=True)
finally:
    pad.reset();pad.update()
