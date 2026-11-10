//! Fullscreen ownership and game handoff. Restore follows Playnite's
//! WindowFactory.RestoreWindow (MIT), including temporary thread attachment.
use gpui::Window;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows::Win32::{
    Foundation::HWND,
    System::Threading::{AttachThreadInput, GetCurrentThreadId},
    UI::{
        Input::KeyboardAndMouse::{SetActiveWindow, SetFocus},
        WindowsAndMessaging::*,
    },
};

pub(crate) fn hwnd(window: &Window) -> Option<HWND> {
    match HasWindowHandle::window_handle(window).ok()?.as_raw() {
        RawWindowHandle::Win32(handle) => Some(HWND(handle.hwnd.get() as *mut _)),
        _ => None,
    }
}
pub fn release(window: &Window) {
    if let Some(h) = hwnd(window) {
        unsafe {
            let _ = SetWindowPos(
                h,
                Some(HWND_NOTOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
            let _ = AllowSetForegroundWindow(ASFW_ANY);
        }
    }
}
pub fn permit_game_activation() {
    unsafe {
        let _ = AllowSetForegroundWindow(ASFW_ANY);
    }
}
pub fn cover(window: &Window) {
    if let Some(h) = hwnd(window) {
        unsafe {
            let _ = SetWindowPos(
                h,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
    }
}
/// Remove the already-presented cover only when the game's window is stable.
/// Automatic console sessions deliberately activate the game.
pub fn handoff(window: &Window, game: u64) -> bool {
    let target = HWND(game as usize as *mut _);
    unsafe {
        let foreground = GetForegroundWindow();
        // NOTOPMOST inserts the launcher at the top of the ordinary band. Do
        // this first, then put the game above it even if it already has focus.
        release(window);
        if IsWindow(Some(target)).as_bool() {
            let other = GetWindowThreadProcessId(foreground, None);
            let this = GetCurrentThreadId();
            let attached =
                other != 0 && other != this && AttachThreadInput(this, other, true).as_bool();
            let _ = SetForegroundWindow(target);
            let _ = SetWindowPos(
                target,
                Some(HWND_TOP),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
            );
            if attached {
                let _ = AttachThreadInput(this, other, false);
            }
            return true;
        }
    }
    false
}
/// Sample action keys on focus regain. Held keys must go neutral before input.
pub fn action_keys_down() -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
    [
        0x0d, 0x1b, 0x70, 0x09, 0x51, 0x45, 0x4d, 0x56, 0x25, 0x26, 0x27, 0x28,
    ]
    .iter()
    .any(|key| unsafe { GetAsyncKeyState(*key) < 0 })
}
/// Run outside GPUI entity/window updates: restoring can synchronously draw.
pub fn restore(handle: usize) {
    let h = HWND(handle as *mut _);
    if unsafe { IsWindow(Some(h)).as_bool() } {
        unsafe {
            if IsIconic(h).as_bool() {
                let _ = ShowWindow(h, SW_RESTORE);
            }
            let foreground = GetForegroundWindow();
            let other = GetWindowThreadProcessId(foreground, None);
            let this = GetCurrentThreadId();
            let attached =
                other != 0 && other != this && AttachThreadInput(this, other, true).as_bool();
            let _ = SetWindowPos(
                h,
                Some(HWND_NOTOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
            );
            if !SetForegroundWindow(h).as_bool() {
                for band in [HWND_TOPMOST, HWND_NOTOPMOST] {
                    let _ = SetWindowPos(h, Some(band), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE);
                }
                let _ = BringWindowToTop(h);
            }
            let _ = SetActiveWindow(h);
            let _ = SetFocus(Some(h));
            if attached {
                let _ = AttachThreadInput(this, other, false);
            }
        }
    }
}
pub fn state(window: &Window) -> String {
    hwnd(window)
        .map(|h| unsafe {
            format!(
                "foreground={};topmost={}",
                GetForegroundWindow() == h,
                GetWindowLongW(h, GWL_EXSTYLE) as u32 & WS_EX_TOPMOST.0 != 0
            )
        })
        .unwrap_or_default()
}
