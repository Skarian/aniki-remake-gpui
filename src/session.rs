//! One executable, two roles. The host never initializes GPUI/SDL/media.
//! Anonymous inherited pipes carry lifecycle messages; no server or Apollo API.
use crate::{
    launch::{self, LaunchEvent},
    library::LaunchPlan,
};
use serde::{Deserialize, Serialize};
use std::{
    io::{BufRead, BufReader, Write},
    os::windows::process::CommandExt,
    path::Path,
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc::{self, Receiver},
    time::{Duration, Instant},
};
use windows::{
    Win32::{
        Foundation::*,
        Graphics::{Dwm::*, Gdi::*},
        System::{LibraryLoader::GetModuleHandleW, Threading::*},
        UI::{HiDpi::*, WindowsAndMessaging::*},
    },
    core::{BOOL, PCWSTR, w},
};

#[derive(Debug, Serialize, Deserialize)]
pub enum UiMessage {
    Ready {
        hwnd: u64,
    },
    Visible,
    Launch {
        plan: LaunchPlan,
        hwnd: u64,
        name: String,
        platform: String,
    },
}
#[derive(Debug, Serialize, Deserialize)]
pub enum HostMessage {
    Reveal,
    Activate,
    Shutdown,
    Game(LaunchEvent),
}

// Serialize normal sessions before creating any window. A second invocation
// remains a lightweight lifetime follower (important for external launchers),
// but can never create another UI or black cover. Kernel objects disappear
// automatically, including after crashes; there is no stale PID/lock file.
struct Instance {
    mutex: HANDLE,
    activate: HANDLE,
    owned: bool,
}
fn instance_key(path: &Path) -> String {
    let normalized = path.to_string_lossy().replace('/', "\\").to_lowercase();
    let hash = normalized
        .bytes()
        .fold(0xcbf29ce484222325u64, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
        });
    format!("Local\\AnikiSession-{hash:016x}")
}
impl Instance {
    fn open() -> anyhow::Result<Self> {
        let key = instance_key(&std::env::current_exe()?.canonicalize()?);
        let mutex_name: Vec<u16> = format!("{key}-owner")
            .encode_utf16()
            .chain(Some(0))
            .collect();
        let event_name: Vec<u16> = format!("{key}-activate")
            .encode_utf16()
            .chain(Some(0))
            .collect();
        unsafe {
            let mutex = CreateMutexW(None, false, PCWSTR(mutex_name.as_ptr()))?;
            let activate = match CreateEventW(None, false, false, PCWSTR(event_name.as_ptr())) {
                Ok(event) => event,
                Err(error) => {
                    let _ = CloseHandle(mutex);
                    return Err(error.into());
                }
            };
            Ok(Self {
                mutex,
                activate,
                owned: false,
            })
        }
    }
    fn acquire_or_follow(&mut self, base: &Path, clock: Instant) -> anyhow::Result<bool> {
        unsafe {
            match WaitForSingleObject(self.mutex, 0) {
                WAIT_OBJECT_0 | WAIT_ABANDONED => {
                    self.owned = true;
                    Ok(true)
                }
                WAIT_TIMEOUT => {
                    let _ = AllowSetForegroundWindow(ASFW_ANY);
                    SetEvent(self.activate)?;
                    crate::log(
                        base,
                        "session_attached",
                        "existing session; no new window",
                        clock,
                    );
                    // This process only waits from here. Release its startup
                    // working set once; do not run a periodic trimming loop.
                    let _ = SetProcessWorkingSetSize(GetCurrentProcess(), usize::MAX, usize::MAX);
                    match WaitForSingleObject(self.mutex, INFINITE) {
                        WAIT_OBJECT_0 | WAIT_ABANDONED => {
                            self.owned = true;
                            crate::log(base, "session_detached", "owner exited", clock);
                            Ok(false)
                        }
                        _ => Err(windows::core::Error::from_win32().into()),
                    }
                }
                _ => Err(windows::core::Error::from_win32().into()),
            }
        }
    }
    fn activation_requested(&self) -> bool {
        unsafe { WaitForSingleObject(self.activate, 0) == WAIT_OBJECT_0 }
    }
}
impl Drop for Instance {
    fn drop(&mut self) {
        unsafe {
            if self.owned {
                let _ = ReleaseMutex(self.mutex);
            }
            let _ = CloseHandle(self.activate);
            let _ = CloseHandle(self.mutex);
        }
    }
}

pub struct Client {
    pub events: Receiver<HostMessage>,
}
impl Client {
    pub fn new() -> Self {
        let (tx, events) = mpsc::channel();
        std::thread::spawn(move || {
            for line in std::io::stdin().lock().lines() {
                let Ok(line) = line else { break };
                if let Ok(event) = serde_json::from_str(&line) {
                    if tx.send(event).is_err() {
                        return;
                    }
                }
            }
            // Host termination (including an external launcher stopping it)
            // must not leave an orphaned fullscreen UI.
            let _ = tx.send(HostMessage::Shutdown);
        });
        Self { events }
    }
    pub fn send(message: &UiMessage) -> std::io::Result<()> {
        let mut out = std::io::stdout().lock();
        serde_json::to_writer(&mut out, message)?;
        out.write_all(b"\n")?;
        out.flush()
    }
}

struct Frontend {
    child: Child,
    input: ChildStdin,
    events: Receiver<UiMessage>,
    started: Instant,
}
impl Frontend {
    fn spawn(args: &[String]) -> anyhow::Result<Self> {
        let mut child = Command::new(std::env::current_exe()?)
            .args(args)
            .arg("--session-ui")
            .env("ANIKI_SESSION_FRONTEND", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .creation_flags(0x08000000)
            .spawn()?;
        let input = child.stdin.take().unwrap();
        let output = child.stdout.take().unwrap();
        let (tx, events) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(output).lines() {
                let Ok(line) = line else { break };
                if let Ok(message) = serde_json::from_str(&line) {
                    if tx.send(message).is_err() {
                        break;
                    }
                }
            }
        });
        unsafe {
            let _ = AllowSetForegroundWindow(child.id());
        }
        Ok(Self {
            child,
            input,
            events,
            started: Instant::now(),
        })
    }
    fn send(&mut self, event: &HostMessage) {
        let _ = serde_json::to_writer(&mut self.input, event);
        let _ = self.input.write_all(b"\n");
        let _ = self.input.flush();
    }
}

struct Cover(HWND);
impl Cover {
    fn new() -> anyhow::Result<Self> {
        unsafe {
            let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
            let instance = GetModuleHandleW(None)?;
            let class = WNDCLASSW {
                lpfnWndProc: Some(cover_proc),
                hInstance: instance.into(),
                lpszClassName: w!("AnikiSessionBlack"),
                hbrBackground: HBRUSH(GetStockObject(BLACK_BRUSH).0),
                ..Default::default()
            };
            RegisterClassW(&class);
            let hwnd = CreateWindowExW(
                // A normal activatable black window becomes Windows' next
                // foreground candidate when the game closes. NOACTIVATE here
                // would let Explorer rise over the cover during exit polling.
                WS_EX_APPWINDOW,
                class.lpszClassName,
                w!("aniki-remake-gpui"),
                WS_POPUP,
                0,
                0,
                GetSystemMetrics(SM_CXSCREEN),
                GetSystemMetrics(SM_CYSCREEN),
                None,
                None,
                Some(instance.into()),
                None,
            )?;
            let disabled = BOOL(1);
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_TRANSITIONS_FORCEDISABLED,
                (&disabled as *const BOOL).cast(),
                std::mem::size_of::<BOOL>() as u32,
            );
            Ok(Self(hwnd))
        }
    }
    fn front(&self) {
        unsafe {
            let _ = SetWindowPos(
                self.0,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
            let _ = UpdateWindow(self.0);
            let _ = DwmFlush();
        }
    }
    fn behind(&self, window: HWND, fit: bool) {
        unsafe {
            let mut rect = RECT::default();
            if fit {
                let _ = GetWindowRect(window, &mut rect);
            }
            // Clear topmost before insertion; never promote the game or alter it.
            let _ = SetWindowPos(
                self.0,
                Some(HWND_NOTOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
            let flags = SWP_NOACTIVATE
                | SWP_SHOWWINDOW
                | if fit {
                    SET_WINDOW_POS_FLAGS(0)
                } else {
                    SWP_NOMOVE | SWP_NOSIZE
                };
            let _ = SetWindowPos(
                self.0,
                Some(window),
                rect.left,
                rect.top,
                rect.right - rect.left,
                rect.bottom - rect.top,
                flags,
            );
            let _ = UpdateWindow(self.0);
            let _ = DwmFlush();
        }
    }
    fn hide(&self) {
        unsafe {
            let _ = ShowWindow(self.0, SW_HIDE);
        }
    }
}
impl Drop for Cover {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.0);
        }
    }
}
unsafe extern "system" fn cover_proc(hwnd: HWND, message: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match message {
        WM_ERASEBKGND => LRESULT(1),
        WM_PAINT => unsafe {
            let mut paint = PAINTSTRUCT::default();
            let dc = BeginPaint(hwnd, &mut paint);
            FillRect(dc, &paint.rcPaint, HBRUSH(GetStockObject(BLACK_BRUSH).0));
            let _ = EndPaint(hwnd, &paint);
            LRESULT(0)
        },
        WM_CLOSE => {
            unsafe {
                PostQuitMessage(0);
            }
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, message, wp, lp) },
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum State {
    Browsing,
    Launching,
    Playing,
    Returning,
}

/// Run the lightweight parent until the user closes the frontend. Game tracking
/// remains in this parent across UI teardown; the same binary rebuilds the UI.
pub fn run(base: &Path, args: &[String]) -> anyhow::Result<()> {
    let clock = Instant::now();
    let mut instance = Instance::open()?;
    if !instance.acquire_or_follow(base, clock)? {
        return Ok(());
    }
    let cover = Cover::new()?;
    cover.front();
    let mut ui = Some(Frontend::spawn(args)?);
    let mut state = State::Browsing;
    let mut closing_ui = false;
    let mut ready = false;
    let mut game_events: Option<Receiver<LaunchEvent>> = None;
    let mut pending_error = None;
    let mut recovery_count = 0;
    let mut resume_args = args.to_vec();
    let mut shutdown_at = None;
    crate::log(base, "host_started", "single_binary", clock);
    loop {
        if instance.activation_requested() {
            // A repeat launch during gameplay must never cover the game.
            if state == State::Browsing && !closing_ui {
                if let Some(ui) = ui.as_mut() {
                    ui.send(&HostMessage::Activate);
                }
            }
            crate::log(
                base,
                "session_activation",
                &format!("state={state:?}"),
                clock,
            );
        }
        unsafe {
            let mut message = MSG::default();
            while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                if message.message == WM_QUIT {
                    return Ok(());
                }
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
        let incoming: Vec<_> = ui
            .as_ref()
            .map(|u| u.events.try_iter().collect())
            .unwrap_or_default();
        for message in incoming {
            match message {
                UiMessage::Ready { hwnd } => {
                    ready = true;
                    crate::log(base, "ui_ready", &format!("hwnd={hwnd}"), clock);
                    if let Some(ui) = ui.as_mut() {
                        if let Some(error) = pending_error.take() {
                            ui.send(&HostMessage::Game(LaunchEvent::Failed(error)));
                        }
                        ui.send(&HostMessage::Reveal);
                    }
                    if state == State::Returning {
                        state = State::Browsing;
                    }
                }
                UiMessage::Visible => {
                    cover.hide();
                    crate::log(base, "ui_visible", "", clock);
                }
                UiMessage::Launch {
                    plan,
                    hwnd,
                    name,
                    platform,
                } if state == State::Browsing => {
                    resume_args = args.to_vec();
                    resume_args.extend(["--resume-platform".into(), platform]);
                    cover.behind(HWND(hwnd as usize as *mut _), true);
                    let (tx, rx) = mpsc::channel();
                    game_events = Some(rx);
                    state = State::Launching;
                    launch::start(plan, tx);
                    crate::log(base, "host_launch", &name, clock);
                }
                _ => {}
            }
        }
        let events: Vec<_> = game_events
            .as_ref()
            .map(|rx| rx.try_iter().collect())
            .unwrap_or_default();
        for event in events {
            match &event {
                LaunchEvent::Started => {
                    state = State::Playing;
                }
                LaunchEvent::Focused(hwnd) => {
                    if state == State::Playing {
                        cover.behind(HWND(*hwnd as usize as *mut _), false);
                        closing_ui = true;
                        shutdown_at = Some(Instant::now());
                        if let Some(ui) = ui.as_mut() {
                            ui.send(&HostMessage::Shutdown);
                        }
                        crate::log(base, "game_focus", &format!("hwnd={hwnd}"), clock);
                    }
                }
                LaunchEvent::Ended => {
                    cover.front();
                    state = State::Returning;
                    game_events = None;
                    closing_ui = true;
                    shutdown_at = Some(Instant::now());
                    if let Some(ui) = ui.as_mut() {
                        ui.send(&HostMessage::Shutdown);
                    }
                    crate::log(base, "host_game_ended", "", clock);
                }
                LaunchEvent::Failed(error) => {
                    game_events = None;
                    if ui.is_some() && !closing_ui {
                        state = State::Browsing;
                        cover.hide();
                    } else {
                        pending_error = Some(error.clone());
                        state = State::Returning;
                        cover.front();
                    }
                }
            }
            if !closing_ui {
                if let Some(ui) = ui.as_mut() {
                    ui.send(&HostMessage::Game(event));
                }
            }
        }
        if let Some(frontend) = ui.as_mut() {
            if closing_ui
                && shutdown_at.is_some_and(|at: Instant| at.elapsed() > Duration::from_secs(5))
            {
                // This is our frontend only, never a game process. A hung view
                // must not prevent return or silently retain its GPU resources.
                let _ = frontend.child.kill();
                let _ = frontend.child.wait();
            }
            if let Some(status) = frontend.child.try_wait()? {
                crate::log(
                    base,
                    "ui_exited",
                    &format!("status={status};state={state:?}"),
                    clock,
                );
                ui = None;
                if !closing_ui && state != State::Playing {
                    if status.success() {
                        break;
                    }
                    if recovery_count >= 1 {
                        anyhow::bail!("The launcher frontend exited unexpectedly twice.");
                    }
                    recovery_count += 1;
                    pending_error =
                        Some("The interface restarted after an unexpected error.".into());
                    state = State::Returning;
                    cover.front();
                }
                closing_ui = false;
            } else if !ready && frontend.started.elapsed() > Duration::from_secs(15) {
                let _ = frontend.child.kill();
                let _ = frontend.child.wait();
                anyhow::bail!("The launcher interface did not become ready within 15 seconds.");
            }
        }
        if ui.is_none() && state == State::Returning {
            ready = false;
            closing_ui = false;
            shutdown_at = None;
            ui = Some(Frontend::spawn(&resume_args)?);
            crate::log(base, "ui_restarted", "", clock);
        }
        std::thread::sleep(Duration::from_millis(if ui.is_some() { 10 } else { 30 }));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn instance_key_matches_windows_path_spelling() {
        assert_eq!(
            instance_key(Path::new("E:/Games/Aniki.exe")),
            instance_key(Path::new("e:\\games\\aniki.exe"))
        );
        assert_ne!(
            instance_key(Path::new("E:/Games/Aniki.exe")),
            instance_key(Path::new("E:/Test/Aniki.exe"))
        );
    }
    #[test]
    fn launch_protocol_preserves_windows_arguments_and_errors() {
        let message = UiMessage::Launch {
            hwnd: 42,
            name: "A game".into(),
            platform: "Nintendo Switch".into(),
            plan: LaunchPlan {
                arguments: "--game \"C:\\Games\\a b.rom\"".into(),
                error: "line one\nline two".into(),
                ..Default::default()
            },
        };
        let encoded = serde_json::to_string(&message).unwrap();
        assert!(!encoded.contains('\n'));
        let UiMessage::Launch { plan, hwnd, .. } = serde_json::from_str(&encoded).unwrap() else {
            panic!()
        };
        assert_eq!(hwnd, 42);
        assert_eq!(plan.arguments, "--game \"C:\\Games\\a b.rom\"");
        assert_eq!(plan.error, "line one\nline two");
    }
}
