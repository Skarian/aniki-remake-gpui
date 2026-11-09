//! One window per executable path. Repeat invocations signal the existing
//! window and wait for its lifetime; no host, IPC pipes, or replacement UI.
use std::{path::Path, time::Instant};
use windows::{
    Win32::{Foundation::*, System::Threading::*, UI::WindowsAndMessaging::*},
    core::PCWSTR,
};

// Serialize normal sessions before creating any window. A second invocation
// remains a lightweight lifetime follower (important for external launchers),
// but can never create another UI or black cover. Kernel objects disappear
// automatically, including after crashes; there is no stale PID/lock file.
pub struct Instance {
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
    pub fn open() -> anyhow::Result<Self> {
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
    pub fn acquire_or_follow(&mut self, base: &Path, clock: Instant) -> anyhow::Result<bool> {
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
    pub fn activation_requested(&self) -> bool {
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn key_matches_windows_spelling() {
        assert_eq!(
            instance_key(Path::new("E:/Games/Aniki.exe")),
            instance_key(Path::new(r"e:\games\aniki.exe"))
        );
        assert_ne!(
            instance_key(Path::new("E:/Games/Aniki.exe")),
            instance_key(Path::new("E:/Test/Aniki.exe"))
        );
    }
}
