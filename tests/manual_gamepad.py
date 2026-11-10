"""Interactive virtual controller for observed, supervised game QA.
One JSON command per stdin line: {"button":"accept"}, {"stick":[0,1],"seconds":1}.
Never edits emulator/controller profiles. Exiting disconnects only this device.
"""
import ctypes as c, json, sys, time
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / '.tools/python'))
import vgamepad as vg
family = sys.argv[1] if len(sys.argv) > 1 else 'xbox'
evidence=Path(sys.argv[2]) if len(sys.argv)>2 else None
if evidence:
    class XPad(c.Structure):
        _fields_=[('buttons',c.c_ushort),('lt',c.c_ubyte),('rt',c.c_ubyte),('lx',c.c_short),('ly',c.c_short),('rx',c.c_short),('ry',c.c_short)]
    class XState(c.Structure):_fields_=[('packet',c.c_ulong),('pad',XPad)]
    xi=c.WinDLL('xinput1_4');xi.XInputGetState.argtypes=[c.c_ulong,c.POINTER(XState)]
    def xstates():
        found={}
        for i in range(4):
            state=XState()
            if xi.XInputGetState(i,c.byref(state))==0:found[f'xinput:{i}']=state.pad.buttons
        return found
    previous_x=set(xstates())
    sdl=c.CDLL(str(Path(__file__).resolve().parents[1]/'SDL2.dll'))
    sdl.SDL_SetHint.argtypes=[c.c_char_p,c.c_char_p]
    sdl.SDL_JoystickOpen.argtypes=[c.c_int];sdl.SDL_JoystickOpen.restype=c.c_void_p
    sdl.SDL_JoystickInstanceID.argtypes=[c.c_void_p]
    sdl.SDL_JoystickName.argtypes=[c.c_void_p];sdl.SDL_JoystickName.restype=c.c_char_p
    sdl.SDL_JoystickNumButtons.argtypes=[c.c_void_p]
    sdl.SDL_JoystickGetButton.argtypes=[c.c_void_p,c.c_int];sdl.SDL_JoystickGetButton.restype=c.c_ubyte
    sdl.SDL_JoystickClose.argtypes=[c.c_void_p]
    sdl.SDL_SetHint(b'SDL_JOYSTICK_ALLOW_BACKGROUND_EVENTS',b'1')
    sdl.SDL_SetHint(b'SDL_JOYSTICK_HIDAPI_PS4',b'1')
    sdl.SDL_SetHint(b'SDL_JOYSTICK_HIDAPI_PS5',b'1')
    assert sdl.SDL_Init(0x2000)==0
    def inventory():
        sdl.SDL_PumpEvents();sdl.SDL_GameControllerUpdate()
        return [sdl.SDL_JoystickOpen(i) for i in range(sdl.SDL_NumJoysticks())]
    previous=inventory();previous_ids={sdl.SDL_JoystickInstanceID(h) for h in previous}
pad = vg.VDS4Gamepad() if family == 'ps' else vg.VX360Gamepad()
if evidence:
    virtual_ids=set();devices=[]
    for _ in range(30):
        time.sleep(.1);devices=inventory()
        virtual_ids=({f'sdl:{sdl.SDL_JoystickInstanceID(h)}' for h in devices if sdl.SDL_JoystickInstanceID(h) not in previous_ids}|(set(xstates())-previous_x))
        if virtual_ids:break
    assert virtual_ids,('No new virtual device observed',previous_ids,previous_x,virtual_ids)
    def emit(row):
        with evidence.open('a') as f:f.write(json.dumps(dict(utc=time.time(),**row))+'\n')
    def active():
        sdl.SDL_PumpEvents();sdl.SDL_GameControllerUpdate()
        return ({f'sdl:{sdl.SDL_JoystickInstanceID(h)}' for h in devices if any(sdl.SDL_JoystickGetButton(h,i) for i in range(sdl.SDL_JoystickNumButtons(h)))}|{i for i,bits in xstates().items() if bits})
    emit(dict(type='inventory',virtual_ids=list(virtual_ids),xinput_slots=list(xstates()),devices=[dict(id=f'sdl:{sdl.SDL_JoystickInstanceID(h)}',name=sdl.SDL_JoystickName(h).decode(errors='replace')) for h in devices]))
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
        if evidence: neutral_before=not active()
        for action in command.get('buttons', [command['button']] if 'button' in command else []):
            if family=='ps' and action in ('up','down','left','right'):
                d=vg.DS4_DPAD_DIRECTIONS
                pad.directional_pad(direction={'up':d.DS4_BUTTON_DPAD_NORTH,'down':d.DS4_BUTTON_DPAD_SOUTH,'left':d.DS4_BUTTON_DPAD_WEST,'right':d.DS4_BUTTON_DPAD_EAST}[action])
            else: pad.press_button(button=buttons[action])
        if 'stick' in command:
            x,y=command['stick']; pad.left_joystick_float(x_value_float=x,y_value_float=y)
        if 'trigger' in command:
            pad.right_trigger_float(value_float=max(0., min(1., float(command['trigger']))))
        pad.update()
        duration=min(command.get('seconds',.12),5)
        if evidence:
            observed=set();until=time.monotonic()+duration
            while time.monotonic()<until:
                observed|=active();time.sleep(.01)
            emit(dict(type='action',command=command,neutral_before=neutral_before,active_ids=list(observed),attributable=neutral_before and bool(observed) and observed<=virtual_ids))
        else:time.sleep(duration)
        pad.reset();pad.update()
        print('released '+json.dumps(command),flush=True)
finally:
    pad.reset();pad.update()
    if evidence:
        for handle in devices+previous:sdl.SDL_JoystickClose(handle)
        sdl.SDL_Quit()
