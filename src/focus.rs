//! Fullscreen ownership and game handoff. Restore follows Playnite's
//! WindowFactory.RestoreWindow (MIT), including temporary thread attachment.
use gpui::Window;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows::Win32::{
    Foundation::HWND,
    System::Threading::{AttachThreadInput, GetCurrentThreadId},
    UI::{Input::KeyboardAndMouse::SetFocus, WindowsAndMessaging::*},
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
/// Never reclaim focus over an unrelated application after a game exits.
/// A normal game exit exposes our retained window, or briefly the shell.
pub fn may_restore(window: &Window) -> bool {
    unsafe {
        let foreground = GetForegroundWindow();
        let mut class = [0u16; 128];
        let len = GetClassNameW(foreground, &mut class);
        return_restore_policy(
            hwnd(window) == Some(foreground),
            foreground.0.is_null(),
            &String::from_utf16_lossy(&class[..len.max(0) as usize]),
        )
    }
}
fn return_restore_policy(ours: bool, no_window: bool, class: &str) -> bool {
    ours || no_window || matches!(class, "Progman" | "WorkerW" | "Shell_TrayWnd")
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
pub fn restore(window: &mut Window) {
    if let Some(h) = hwnd(window) {
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
                Some(if window.is_fullscreen() {
                    HWND_TOPMOST
                } else {
                    HWND_TOP
                }),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
            );
            let _ = SetForegroundWindow(h);
            let _ = SetFocus(Some(h));
            if attached {
                let _ = AttachThreadInput(this, other, false);
            }
        }
    }
    window.activate_window();
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn game_exit_restores_exposed_launcher_but_not_over_other_work() {
        assert!(return_restore_policy(true, false, "GPUI"));
        assert!(return_restore_policy(false, true, ""));
        assert!(return_restore_policy(false, false, "WorkerW"));
        assert!(!return_restore_policy(false, false, "Chrome_WidgetWin_1"));
        assert!(!return_restore_policy(false, false, "SDL_app"));
    }
}
