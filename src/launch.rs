use crate::library::LaunchPlan;
use std::os::windows::process::CommandExt;
use std::{
    collections::HashSet,
    path::Path,
    process::{Child, Command},
    sync::mpsc::Sender,
    time::{Duration, Instant},
};
use windows::{
    Win32::{
        Foundation::{CloseHandle, HWND},
        System::{Diagnostics::ToolHelp::*, Registry::*, Threading::*},
        UI::{
            Shell::ShellExecuteW,
            WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId, SW_SHOWNORMAL},
        },
    },
    core::{PCWSTR, PWSTR, w},
};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum LaunchEvent {
    Started,
    Focused(u64),
    Ended,
    Failed(String),
}
#[derive(Clone, Debug)]
pub struct Process {
    pub pid: u32,
    pub parent: u32,
    pub path: String,
}
pub fn processes() -> Vec<Process> {
    unsafe {
        let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
            return vec![];
        };
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut out = Vec::new();
        if Process32FirstW(snap, &mut entry).is_ok() {
            loop {
                if let Ok(handle) = OpenProcess(
                    PROCESS_QUERY_LIMITED_INFORMATION,
                    false,
                    entry.th32ProcessID,
                ) {
                    let mut buffer = [0u16; 32768];
                    let mut len = buffer.len() as u32;
                    if QueryFullProcessImageNameW(
                        handle,
                        PROCESS_NAME_WIN32,
                        PWSTR(buffer.as_mut_ptr()),
                        &mut len,
                    )
                    .is_ok()
                    {
                        out.push(Process {
                            pid: entry.th32ProcessID,
                            parent: entry.th32ParentProcessID,
                            path: String::from_utf16_lossy(&buffer[..len as usize]),
                        });
                    }
                    let _ = CloseHandle(handle);
                }
                if Process32NextW(snap, &mut entry).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snap);
        out
    }
}
pub fn descendants(live: &[Process], tracked: &mut HashSet<u32>) {
    loop {
        let before = tracked.len();
        for p in live {
            if tracked.contains(&p.parent) {
                tracked.insert(p.pid);
            }
        }
        if tracked.len() == before {
            break;
        }
    }
}
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
fn steam_running(app: &str) -> bool {
    unsafe {
        let key = wide(&format!(r"Software\Valve\Steam\Apps\{app}"));
        let mut value = 0u32;
        let mut size = 4u32;
        RegGetValueW(
            HKEY_CURRENT_USER,
            PCWSTR(key.as_ptr()),
            w!("Running"),
            RRF_RT_REG_DWORD,
            None,
            Some((&mut value as *mut u32).cast()),
            Some(&mut size),
        )
        .is_ok()
            && value != 0
    }
}
pub fn start(plan: LaunchPlan, tx: Sender<LaunchEvent>) {
    std::thread::spawn(move || {
        let fail = |message: String| {
            let _ = tx.send(LaunchEvent::Failed(message));
        };
        if !plan.error.is_empty() {
            fail(plan.error);
            return;
        }
        let before = processes();
        let baseline: HashSet<u32> = before.iter().map(|p| p.pid).collect();
        let mut tracked = HashSet::new();
        let mut child: Option<Child> = None;
        if plan.kind == "steam" {
            let url = wide(&format!("steam://rungameid/{}", plan.steam_id));
            let result = unsafe {
                ShellExecuteW(
                    Some(HWND::default()),
                    w!("open"),
                    PCWSTR(url.as_ptr()),
                    None,
                    None,
                    SW_SHOWNORMAL,
                )
            };
            if result.0 as isize <= 32 {
                fail("Steam could not open this game. Check that Steam is installed.".into());
                return;
            }
        } else if plan.kind == "process" {
            if !Path::new(&plan.executable).is_file() {
                fail(format!("Executable not found: {}", plan.executable));
                return;
            }
            if before
                .iter()
                .any(|p| p.path.eq_ignore_ascii_case(&plan.executable))
            {
                fail("This emulator or game is already running. Close it before starting another session.".into());
                return;
            }
            let mut command = Command::new(&plan.executable);
            command.raw_arg(&plan.arguments);
            if !plan.working_dir.is_empty() {
                command.current_dir(&plan.working_dir);
            }
            match command.spawn() {
                Ok(c) => {
                    tracked.insert(c.id());
                    child = Some(c)
                }
                Err(e) => {
                    fail(e.to_string());
                    return;
                }
            }
        } else {
            fail("No supported launch action".into());
            return;
        }
        let clock = Instant::now();
        let mut started = false;
        let mut focused = false;
        let mut gone = 0u8;
        let install = plan
            .install_dir
            .trim_end_matches(['\\', '/'])
            .to_lowercase()
            + "\\";
        loop {
            std::thread::sleep(Duration::from_millis(350));
            let live = processes();
            descendants(&live, &mut tracked);
            let matched = live.iter().any(|p| {
                tracked.contains(&p.pid)
                    || (!baseline.contains(&p.pid)
                        && install.len() > 3
                        && p.path.to_lowercase().starts_with(&install))
            });
            let active = matched || (plan.kind == "steam" && steam_running(&plan.steam_id));
            if active {
                gone = 0;
                if !started {
                    started = true;
                    let _ = tx.send(LaunchEvent::Started);
                }
                // Only retire the frontend after a tracked game's actual
                // foreground window appears, never just on process creation.
                if !focused {
                    let foreground = unsafe { GetForegroundWindow() };
                    let mut pid = 0;
                    unsafe {
                        GetWindowThreadProcessId(foreground, Some(&mut pid));
                    }
                    if live.iter().any(|p| {
                        p.pid == pid
                            && (tracked.contains(&p.pid)
                                || (!baseline.contains(&p.pid)
                                    && install.len() > 3
                                    && p.path.to_lowercase().starts_with(&install)))
                    }) {
                        focused = true;
                        let _ = tx.send(LaunchEvent::Focused(foreground.0 as usize as u64));
                    }
                }
            } else if started {
                gone += 1;
                if gone >= 3 {
                    let _ = tx.send(LaunchEvent::Ended);
                    break;
                }
            } else if clock.elapsed() > Duration::from_secs(90) {
                fail("The game did not start within 90 seconds. Return to the library and try again.".into());
                break;
            }
            if let Some(c) = child.as_mut() {
                let _ = c.try_wait();
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tracks_descendants_after_parent_exits() {
        let mut tracked = HashSet::from([10]);
        let first = vec![
            Process {
                pid: 11,
                parent: 10,
                path: "helper".into(),
            },
            Process {
                pid: 12,
                parent: 11,
                path: "game".into(),
            },
        ];
        descendants(&first, &mut tracked);
        assert!(tracked.contains(&12));
        let second = vec![Process {
            pid: 12,
            parent: 11,
            path: "game".into(),
        }];
        descendants(&second, &mut tracked);
        assert!(second.iter().any(|p| tracked.contains(&p.pid)));
    }
    #[test]
    fn unrelated_process_does_not_join_session() {
        let mut tracked = HashSet::from([10]);
        descendants(
            &[Process {
                pid: 11,
                parent: 9,
                path: "other".into(),
            }],
            &mut tracked,
        );
        assert!(!tracked.contains(&11));
    }
}
