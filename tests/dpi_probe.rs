//! Hidden GPUI window: DPI changes must update the viewport even at unchanged size.
//! Does not change monitor settings or touch the running launcher.
use gpui::{prelude::*, *};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows::Win32::{
    Foundation::{HWND, LPARAM, RECT, WPARAM},
    UI::WindowsAndMessaging::*,
};

struct Probe;
impl Render for Probe {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

fn main() {
    Application::new().run(|cx| {
        let handle = cx.open_window(WindowOptions {
            show: false,
            focus: false,
            window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                point(px(0.), px(0.)), size(px(640.), px(360.)),
            ))),
            ..Default::default()
        }, |_, cx| cx.new(|_| Probe)).unwrap();
        let hwnd = handle.update(cx, |_, window, _| {
            match window.window_handle().unwrap().as_raw() {
                RawWindowHandle::Win32(h) => h.hwnd.get(),
                _ => panic!("Windows only"),
            }
        }).unwrap();
        cx.spawn(async move |cx| {
            // Run outside an App/Window update: native messages call back into GPUI.
            Timer::after(std::time::Duration::from_millis(100)).await;
            let hwnd = HWND(hwnd as *mut _);
            unsafe { SetWindowPos(hwnd, None, 0, 0, 640, 360, SWP_NOACTIVATE | SWP_NOZORDER).unwrap(); }
            let mut failures = 0;
            for borderless in [false, true] {
                if borderless {
                    unsafe {
                        SetWindowLongPtrW(hwnd, GWL_STYLE, WS_POPUP.0 as isize);
                        SetWindowPos(hwnd, None, 0, 0, 640, 360,
                            SWP_NOACTIVATE | SWP_NOZORDER | SWP_FRAMECHANGED).unwrap();
                    }
                }
                for dpi in [96usize, 144, 192, 96] {
                    let mut outer = RECT::default();
                    unsafe {
                        GetWindowRect(hwnd, &mut outer).unwrap();
                        SendMessageW(hwnd, WM_DPICHANGED,
                            Some(WPARAM(dpi | (dpi << 16))),
                            Some(LPARAM((&outer as *const RECT) as isize)));
                    }
                    let mut client = RECT::default();
                    unsafe { GetClientRect(hwnd, &mut client).unwrap(); }
                    let (viewport, scale) = handle.update(cx, |_, window, _| {
                        (window.viewport_size(), window.scale_factor())
                    }).unwrap();
                    let expected_scale = dpi as f32 / 96.;
                    let expected = size(px((client.right - client.left) as f32 / expected_scale),
                        px((client.bottom - client.top) as f32 / expected_scale));
                    let passed = viewport == expected && scale == expected_scale;
                    println!("borderless={borderless} dpi={dpi} actual={viewport:?}@{scale} expected={expected:?}@{expected_scale} pass={passed}");
                    failures += usize::from(!passed);
                }
            }
            // A nonzero exit makes a stale viewport a test failure, not a log warning.
            std::process::exit(if failures == 0 { 0 } else { 1 });
        }).detach();
    });
}
