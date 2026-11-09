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
