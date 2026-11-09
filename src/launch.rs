use crate::library::LaunchPlan;
use std::{
    collections::HashSet,
    path::Path,
    sync::mpsc::Sender,
    time::{Duration, Instant},
};
use windows::{
    Win32::{
        Foundation::{CloseHandle, HANDLE, HWND},
        System::{Diagnostics::ToolHelp::*, Registry::*, Threading::*},
        UI::{
            Shell::{
                SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
                ShellExecuteExW, ShellExecuteW,
            },
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
    // Like Playnite's MonitorProcessTree, retain only the current generation.
    // Keeping dead parents forever can adopt unrelated processes after PID reuse.
    tracked.retain(|pid| live.iter().any(|p| p.pid == *pid));
}
fn matches_game(
    p: &Process,
    tracked: &HashSet<u32>,
    baseline: &HashSet<u32>,
    steam: bool,
    install: &str,
) -> bool {
    tracked.contains(&p.pid)
        || (steam
            && !baseline.contains(&p.pid)
            && install.len() > 3
            && p.path.to_lowercase().starts_with(install))
}
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
struct StartedProcess(HANDLE);
impl Drop for StartedProcess {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
// Playnite ProcessStarter.StartProcess uses ProcessStartInfo's shell-execute
// default. Use the same Windows launch facility, keeping its process handle.
// Game processes do not inherit launcher diagnostic/automation pipe handles.
fn start_process(plan: &LaunchPlan) -> windows::core::Result<StartedProcess> {
    let executable = wide(&plan.executable);
    let arguments = wide(&plan.arguments);
    let directory = if plan.working_dir.is_empty() {
        Path::new(&plan.executable)
            .parent()
            .unwrap_or(Path::new("."))
            .to_string_lossy()
            .into_owned()
    } else {
        plan.working_dir.clone()
    };
    let directory = wide(&directory);
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC | SEE_MASK_FLAG_NO_UI,
        lpVerb: w!("open"),
        lpFile: PCWSTR(executable.as_ptr()),
        lpParameters: PCWSTR(arguments.as_ptr()),
        lpDirectory: PCWSTR(directory.as_ptr()),
        nShow: SW_SHOWNORMAL.0,
        ..Default::default()
    };
    unsafe {
        ShellExecuteExW(&mut info)?;
    }
    Ok(StartedProcess(info.hProcess))
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
        let mut _child: Option<StartedProcess> = None;
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
            match start_process(&plan) {
                Ok(c) => {
                    tracked.insert(unsafe { GetProcessId(c.0) });
                    _child = Some(c)
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
            let game_pids: HashSet<u32> = live
                .iter()
                .filter(|p| matches_game(p, &tracked, &baseline, plan.kind == "steam", &install))
                .map(|p| p.pid)
                .collect();
            let matched = !game_pids.is_empty();
            let active = matched || (plan.kind == "steam" && steam_running(&plan.steam_id));
            if active {
                gone = 0;
                if !started {
                    started = true;
                    let _ = tx.send(LaunchEvent::Started);
                }
                // Only minimize the launcher after a tracked game's actual
                // foreground window appears, never just on process creation.
                if !focused {
                    let foreground = unsafe { GetForegroundWindow() };
                    let mut pid = 0;
                    unsafe {
                        GetWindowThreadProcessId(foreground, Some(&mut pid));
                    }
                    if game_pids.contains(&pid) {
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
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn direct_launch_tracks_tree_not_unrelated_files_in_its_folder() {
        let p = Process {
            pid: 11,
            parent: 9,
            path: "c:\\games\\other.exe".into(),
        };
        let tracked = HashSet::from([10]);
        let baseline = HashSet::new();
        assert!(!matches_game(&p, &tracked, &baseline, false, "c:\\games\\"));
        assert!(matches_game(&p, &tracked, &baseline, true, "c:\\games\\"));
        assert!(!matches_game(
            &p,
            &tracked,
            &HashSet::from([11]),
            true,
            "c:\\games\\"
        ));
        assert!(!matches_game(&p, &tracked, &baseline, true, "c:\\game\\"));
    }
    #[test]
    fn exited_ids_cannot_adopt_a_later_unrelated_process() {
        let mut tracked = HashSet::from([10]);
        descendants(&[], &mut tracked);
        assert!(tracked.is_empty());
        descendants(
            &[Process {
                pid: 11,
                parent: 10,
                path: "unrelated".into(),
            }],
            &mut tracked,
        );
        assert!(tracked.is_empty());
    }
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
