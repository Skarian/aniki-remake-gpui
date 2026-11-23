use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use windows::Win32::UI::Input::XboxController::*;
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Input {
    Left,
    Right,
    Up,
    Down,
    Accept,
    Back,
    PreviousPlatform,
    NextPlatform,
    Menu,
    Music,
    Video,
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Kind {
    #[default]
    Xbox,
    PlayStation,
}
#[derive(Clone, Copy, Default)]
struct Sample {
    ready: bool,
    connected: bool,
    buttons: u16,
    kind: Kind,
    device: i32,
}
pub struct Controller {
    previous: u16,
    held: Option<Input>,
    repeat: Instant,
    pub connected: bool,
    pub kind: Kind,
    device: i32,
    sample: Arc<Mutex<Sample>>,
    stop: Arc<AtomicBool>,
    require_neutral: bool,
}
impl Controller {
    pub fn new() -> Self {
        Self {
            previous: 0,
            held: None,
            repeat: Instant::now(),
            connected: false,
            kind: Kind::Xbox,
            device: -1,
            sample: Arc::new(Mutex::new(Sample::default())),
            stop: Arc::new(AtomicBool::new(false)),
            require_neutral: false,
        }
    }
    pub fn start(base: PathBuf) -> Self {
        let controller = Self::new();
        let sample = controller.sample.clone();
        let stop = controller.stop.clone();
        // Device enumeration can be slow. Keep SDL entirely off the first-frame path.
        std::thread::spawn(move || {
            let diagnostic = base.join("artifacts/controller-backend.log");
            if let Err(error) = unsafe { sdl_worker(sample, stop) } {
                let _ = std::fs::write(
                    diagnostic,
                    format!("SDL unavailable; Xbox fallback active: {error:#}"),
                );
            }
        });
        controller
    }
    pub fn poll(&mut self) -> Vec<Input> {
        let sample = *self.sample.lock().unwrap();
        // Keep Windows' direct XInput path for Xbox hot-plug. SDL owns native
        // PlayStation input. Choose one source per poll to avoid doubled actions
        // when Steam/DS4Windows also exposes an Xbox translation of the same pad.
        let mut state = XINPUT_STATE::default();
        let found = (0..4).any(|i| unsafe { XInputGetState(i, &mut state) == 0 });
        let xbox_active = state.Gamepad.wButtons.0 != 0
            || state.Gamepad.sThumbLX.unsigned_abs() > 18000
            || state.Gamepad.sThumbLY.unsigned_abs() > 18000;
        if sample.ready
            && sample.connected
            && (!found
                || sample.kind == Kind::PlayStation
                    && (sample.buttons != 0 || self.kind == Kind::PlayStation && !xbox_active))
        {
            self.connected = sample.connected;
            self.kind = sample.kind;
            if self.device != sample.device {
                self.previous = 0;
                self.held = None;
                self.device = sample.device;
            }
            return self.process(sample.buttons, Instant::now());
        }
        self.connected = found;
        if !found {
            self.previous = 0;
            self.held = None;
            return vec![];
        }
        self.kind = Kind::Xbox;
        if self.device != -2 {
            self.device = -2;
            self.previous = 0;
            self.held = None;
        }
        let mut buttons = state.Gamepad.wButtons.0;
        if state.Gamepad.bLeftTrigger > 30 {
            buttons |= 0x400;
        }
        if state.Gamepad.bRightTrigger > 30 {
            buttons |= 0x800;
        }
        if state.Gamepad.sThumbLX > 18000 {
            buttons |= XINPUT_GAMEPAD_DPAD_RIGHT.0
        }
        if state.Gamepad.sThumbLX < -18000 {
            buttons |= XINPUT_GAMEPAD_DPAD_LEFT.0
        }
        if state.Gamepad.sThumbLY > 18000 {
            buttons |= XINPUT_GAMEPAD_DPAD_UP.0
        }
        if state.Gamepad.sThumbLY < -18000 {
            buttons |= XINPUT_GAMEPAD_DPAD_DOWN.0
        }
        self.process(buttons, Instant::now())
    }
    pub fn require_release(&mut self) {
        self.require_neutral = true;
    }
    fn process(&mut self, buttons: u16, now: Instant) -> Vec<Input> {
        if self.require_neutral {
            self.require_neutral = buttons != 0;
            self.previous = buttons;
            self.held = None;
            return vec![];
        }
        let mappings = [
            (XINPUT_GAMEPAD_DPAD_LEFT.0, Input::Left),
            (XINPUT_GAMEPAD_DPAD_RIGHT.0, Input::Right),
            (XINPUT_GAMEPAD_DPAD_UP.0, Input::Up),
            (XINPUT_GAMEPAD_DPAD_DOWN.0, Input::Down),
            (XINPUT_GAMEPAD_A.0, Input::Accept),
            (XINPUT_GAMEPAD_B.0, Input::Back),
            (XINPUT_GAMEPAD_LEFT_SHOULDER.0, Input::PreviousPlatform),
            (XINPUT_GAMEPAD_RIGHT_SHOULDER.0, Input::NextPlatform),
            (XINPUT_GAMEPAD_START.0, Input::Menu),
            (XINPUT_GAMEPAD_X.0, Input::Music),
            (XINPUT_GAMEPAD_Y.0, Input::Video),
        ];
        let edges = buttons & !self.previous;
        self.previous = buttons;
        let mut out = Vec::new();
        for (bit, input) in mappings {
            if edges & bit != 0 {
                out.push(input);
            }
        }
        let direction = mappings[..4]
            .iter()
            .find(|(bit, _)| buttons & bit != 0)
            .map(|(_, input)| *input);
        if direction != self.held {
            self.held = direction;
            self.repeat = now + Duration::from_millis(360);
        } else if direction.is_some() && now >= self.repeat {
            out.push(direction.unwrap());
            self.repeat = now + Duration::from_millis(120);
        }
        out
    }
}
impl Drop for Controller {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

// SDL is statically linked into this executable.
// It supplies native DS4/DualSense HID and standard Xbox mappings; no remapper.
unsafe fn sdl_worker(sample: Arc<Mutex<Sample>>, stop: Arc<AtomicBool>) -> anyhow::Result<()> {
    use sdl2_sys::{SDL_GameControllerAxis::*, SDL_GameControllerButton::*, SDL_bool::SDL_FALSE};
    use sdl2_sys::{
        SDL_GameControllerClose as close, SDL_GameControllerEventState as events,
        SDL_GameControllerGetAttached as attached, SDL_GameControllerGetAxis as axis,
        SDL_GameControllerGetButton as button, SDL_GameControllerGetType as kind,
        SDL_GameControllerOpen as open, SDL_GameControllerUpdate as update,
        SDL_InitSubSystem as init, SDL_IsGameController as is_controller,
        SDL_JoystickEventState as joystick_events, SDL_JoystickGetDeviceInstanceID as instance,
        SDL_NumJoysticks as count, SDL_PumpEvents as pump, SDL_QuitSubSystem as quit,
        SDL_SetHint as hint,
    };
    type Pad = *mut sdl2_sys::SDL_GameController;
    unsafe {
        hint(
            c"SDL_JOYSTICK_ALLOW_BACKGROUND_EVENTS".as_ptr(),
            c"1".as_ptr(),
        );
        hint(c"SDL_JOYSTICK_HIDAPI_PS4".as_ptr(), c"1".as_ptr());
        hint(c"SDL_JOYSTICK_HIDAPI_PS5".as_ptr(), c"1".as_ptr());
        if init(0x2000) != 0 {
            anyhow::bail!("SDL controller initialization failed");
        }
        events(0); // Poll state; don't accumulate an unused event queue.
        joystick_events(0);
        let mut pads: Vec<(i32, Pad)> = Vec::new();
        let mut current = -1;
        while !stop.load(Ordering::Relaxed) {
            pump();
            update();
            pads.retain(|(_, p)| {
                if attached(*p) == SDL_FALSE {
                    close(*p);
                    false
                } else {
                    true
                }
            });
            for index in 0..count() {
                let id = instance(index);
                if is_controller(index) != SDL_FALSE && !pads.iter().any(|(n, _)| *n == id) {
                    let p = open(index);
                    if !p.is_null() {
                        pads.push((id, p));
                    }
                }
            }
            let mut next = Sample {
                ready: true,
                device: -1,
                ..Default::default()
            };
            for &(id, p) in &pads {
                let mut bits = 0u16;
                for (sdl, xinput) in [
                    (SDL_CONTROLLER_BUTTON_A, 0x1000),
                    (SDL_CONTROLLER_BUTTON_B, 0x2000),
                    (SDL_CONTROLLER_BUTTON_X, 0x4000),
                    (SDL_CONTROLLER_BUTTON_Y, 0x8000),
                    (SDL_CONTROLLER_BUTTON_BACK, 0x20),
                    (SDL_CONTROLLER_BUTTON_START, 0x10),
                    (SDL_CONTROLLER_BUTTON_LEFTSTICK, 0x40),
                    (SDL_CONTROLLER_BUTTON_RIGHTSTICK, 0x80),
                    (SDL_CONTROLLER_BUTTON_LEFTSHOULDER, 0x100),
                    (SDL_CONTROLLER_BUTTON_RIGHTSHOULDER, 0x200),
                    (SDL_CONTROLLER_BUTTON_DPAD_UP, 1),
                    (SDL_CONTROLLER_BUTTON_DPAD_DOWN, 2),
                    (SDL_CONTROLLER_BUTTON_DPAD_LEFT, 4),
                    (SDL_CONTROLLER_BUTTON_DPAD_RIGHT, 8),
                ] {
                    if button(p, sdl) != 0 {
                        bits |= xinput;
                    }
                }
                let (x, y) = (
                    axis(p, SDL_CONTROLLER_AXIS_LEFTX),
                    axis(p, SDL_CONTROLLER_AXIS_LEFTY),
                );
                if axis(p, SDL_CONTROLLER_AXIS_TRIGGERLEFT) > 4000 {
                    bits |= 0x400;
                }
                if axis(p, SDL_CONTROLLER_AXIS_TRIGGERRIGHT) > 4000 {
                    bits |= 0x800;
                }
                if x > 18000 {
                    bits |= 8;
                }
                if x < -18000 {
                    bits |= 4;
                }
                if y > 18000 {
                    bits |= 2;
                }
                if y < -18000 {
                    bits |= 1;
                }
                if !next.connected || id == current || bits != 0 {
                    next = Sample {
                        ready: true,
                        connected: true,
                        buttons: bits,
                        kind: if matches!(kind(p) as i32, 3 | 4 | 7) {
                            Kind::PlayStation
                        } else {
                            Kind::Xbox
                        },
                        device: id,
                    };
                    if bits != 0 {
                        current = id;
                        break;
                    }
                }
            }
            *sample.lock().unwrap() = next;
            std::thread::sleep(Duration::from_millis(8));
        }
        for (_, p) in pads {
            close(p);
        }
        quit(0x2000);
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "Initializes SDL; run explicitly with --test-threads=1"]
    fn static_sdl_virtual_controller_reaches_worker() {
        use sdl2_sys::*;
        unsafe {
            SDL_SetMainReady();
            assert_eq!(SDL_InitSubSystem(SDL_INIT_GAMECONTROLLER), 0);
            let index = SDL_JoystickAttachVirtual(
                SDL_JoystickType::SDL_JOYSTICK_TYPE_GAMECONTROLLER,
                6,
                15,
                0,
            );
            assert!(index >= 0);
            let pad = SDL_JoystickOpen(index);
            assert!(!pad.is_null());
            let id = SDL_JoystickInstanceID(pad);
            let sample = Arc::new(Mutex::new(Sample::default()));
            let stop = Arc::new(AtomicBool::new(false));
            let (worker_sample, worker_stop) = (sample.clone(), stop.clone());
            let worker = std::thread::spawn(move || sdl_worker(worker_sample, worker_stop));
            assert_eq!(SDL_JoystickSetVirtualButton(pad, 0, 1), 0);
            let deadline = Instant::now() + Duration::from_secs(3);
            let passed = loop {
                let state = *sample.lock().unwrap();
                if state.ready && state.device == id && state.buttons & 0x1000 != 0 {
                    break true;
                }
                if Instant::now() >= deadline {
                    break false;
                }
                std::thread::sleep(Duration::from_millis(10));
            };
            stop.store(true, Ordering::Relaxed);
            worker.join().unwrap().unwrap();
            SDL_JoystickClose(pad);
            SDL_JoystickDetachVirtual(index);
            SDL_QuitSubSystem(SDL_INIT_GAMECONTROLLER);
            assert!(
                passed,
                "Virtual A button did not reach the production SDL worker"
            );
        }
    }
    #[test]
    fn recovery_button_must_be_released_before_confirming() {
        let mut c = Controller::new();
        c.require_release();
        let n = Instant::now();
        assert!(c.process(XINPUT_GAMEPAD_A.0, n).is_empty());
        assert!(
            c.process(XINPUT_GAMEPAD_A.0, n + Duration::from_secs(1))
                .is_empty()
        );
        assert!(c.process(0, n + Duration::from_secs(2)).is_empty());
        assert_eq!(
            c.process(XINPUT_GAMEPAD_A.0, n + Duration::from_secs(3)),
            vec![Input::Accept]
        );
    }
    #[test]
    fn held_game_exit_back_cannot_skip_return_details() {
        let mut c = Controller::new();
        let now = Instant::now();
        c.process(XINPUT_GAMEPAD_B.0, now);
        c.require_release();
        assert!(
            c.process(XINPUT_GAMEPAD_B.0, now + Duration::from_secs(1))
                .is_empty()
        );
        assert!(c.process(0, now + Duration::from_secs(2)).is_empty());
        assert_eq!(
            c.process(XINPUT_GAMEPAD_B.0, now + Duration::from_secs(3)),
            vec![Input::Back]
        );
    }
    #[test]
    fn held_accept_never_launches_twice() {
        let mut c = Controller::new();
        let now = Instant::now();
        assert_eq!(c.process(XINPUT_GAMEPAD_A.0, now), vec![Input::Accept]);
        assert!(
            c.process(XINPUT_GAMEPAD_A.0, now + Duration::from_secs(1))
                .is_empty()
        );
    }
    #[test]
    fn directions_repeat_after_delay() {
        let mut c = Controller::new();
        let n = Instant::now();
        assert_eq!(
            c.process(XINPUT_GAMEPAD_DPAD_RIGHT.0, n),
            vec![Input::Right]
        );
        assert!(
            c.process(XINPUT_GAMEPAD_DPAD_RIGHT.0, n + Duration::from_millis(100))
                .is_empty()
        );
        assert_eq!(
            c.process(XINPUT_GAMEPAD_DPAD_RIGHT.0, n + Duration::from_millis(400)),
            vec![Input::Right]
        );
    }
}
