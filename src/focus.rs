//! Fullscreen ownership and game handoff. Restore follows Playnite's
//! WindowFactory.RestoreWindow (MIT), including temporary thread attachment.
use gpui::Window;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows::Win32::{
    Foundation::{HWND, POINT},
    Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTOPRIMARY, MONITORINFO, MonitorFromPoint},
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
// Ordinary focus operations leave geometry unchanged.
fn position(h: HWND, band: HWND, flags: SET_WINDOW_POS_FLAGS) {
    unsafe {
        let _ = SetWindowPos(h, Some(band), 0, 0, 0, 0, flags | SWP_NOMOVE | SWP_NOSIZE);
    }
}
pub fn release(window: &Window) {
    if let Some(h) = hwnd(window) {
        unsafe {
            position(h, HWND_NOTOPMOST, SWP_NOACTIVATE);
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
        position(h, HWND_TOPMOST, SWP_NOACTIVATE);
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
            position(target, HWND_TOP, SWP_SHOWWINDOW);
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
/// Reuse startup's display policy on an explicit fullscreen reactivation.
/// Query Win32 now: Apollo may have added a new primary since GPUI started.
/// Run outside GPUI borrows, since moving/resizing invokes synchronous callbacks.
pub fn follow_primary(handle: usize) -> windows::core::Result<()> {
    let h = HWND(handle as *mut _);
    unsafe {
        let monitor = MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY);
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        GetMonitorInfoW(monitor, &mut info).ok()?;
        if IsIconic(h).as_bool() {
            let _ = ShowWindow(h, SW_RESTORE);
        }
        let rect = info.rcMonitor;
        SetWindowPos(
            h,
            None,
            rect.left,
            rect.top,
            rect.right - rect.left,
            rect.bottom - rect.top,
            SWP_NOZORDER | SWP_NOACTIVATE,
        )
    }
}
/// Run outside GPUI entity/window updates: restoring can synchronously draw.
pub fn restore(handle: usize) {
    let h = HWND(handle as *mut _);
    if unsafe { IsWindow(Some(h)).as_bool() } {
        unsafe {
            if IsIconic(h).as_bool() {
                let _ = ShowWindow(h, SW_RESTORE);
            }
            position(h, HWND_NOTOPMOST, SWP_SHOWWINDOW | SWP_NOACTIVATE);
            if !SetForegroundWindow(h).as_bool() {
                for band in [HWND_TOPMOST, HWND_NOTOPMOST] {
                    position(h, band, SET_WINDOW_POS_FLAGS::default());
                }
            }
            // Playnite attaches after activation, then raises in the normal band.
            let foreground = GetForegroundWindow();
            let other = GetWindowThreadProcessId(foreground, None);
            let this = GetCurrentThreadId();
            let attached =
                other != 0 && other != this && AttachThreadInput(this, other, true).as_bool();
            position(h, HWND_TOP, SWP_SHOWWINDOW);
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

#[cfg(test)]
mod tests {
    use super::*;
    use windows::{Win32::Foundation::RECT, core::w};

    #[test]
    fn reactivation_fits_primary_without_showing_or_activating_window() {
        unsafe {
            // A hidden native window exercises real pixel geometry without
            // opening the launcher, changing monitors, or taking user focus.
            let h = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("STATIC"),
                w!("monitor regression"),
                WS_POPUP,
                -10000,
                -10000,
                320,
                180,
                None,
                None,
                None,
                None,
            )
            .unwrap();
            struct Cleanup(HWND);
            impl Drop for Cleanup {
                fn drop(&mut self) {
                    unsafe {
                        let _ = DestroyWindow(self.0);
                    }
                }
            }
            let _cleanup = Cleanup(h);
            let foreground = GetForegroundWindow();
            let mut info = MONITORINFO {
                cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            let monitor = MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY);
            GetMonitorInfoW(monitor, &mut info).ok().unwrap();
            // Repeating the activation must keep the same exact monitor bounds.
            for _ in 0..2 {
                follow_primary(h.0 as usize).unwrap();
                let mut actual = RECT::default();
                GetWindowRect(h, &mut actual).unwrap();
                assert_eq!(actual, info.rcMonitor);
                assert!(!IsWindowVisible(h).as_bool());
                assert_eq!(GetForegroundWindow(), foreground);
                assert_eq!(GetWindowLongW(h, GWL_EXSTYLE) as u32 & WS_EX_TOPMOST.0, 0);
            }
        }
    }
}
