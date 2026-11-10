use crate::library::LaunchPlan;
use std::{
    collections::HashSet,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::Sender,
    },
    time::{Duration, Instant},
};
use windows::{
    Win32::{
        Foundation::{CloseHandle, ERROR_NO_MORE_FILES, HANDLE, HWND, LPARAM, RECT},
        System::{
            Com::*, Diagnostics::ToolHelp::*, Registry::*, RemoteDesktop::ProcessIdToSessionId,
            Threading::*,
        },
        UI::{
            Shell::{
                SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
                ShellExecuteExW,
            },
            WindowsAndMessaging::*,
        },
    },
    core::{PCWSTR, PWSTR, w},
};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum LaunchEvent {
    Started,
    Ready(u64),
    ReadinessExpired,
    Ended,
    Failed(String),
}

/// AnikiHelper likewise waits for a stable, usable game window before removing
/// its splash. Process creation or a fleeting bootstrap window is insufficient.
#[derive(Default)]
struct Readiness {
    candidate: Option<(u64, Instant)>,
}
impl Readiness {
    fn update(&mut self, candidate: Option<u64>, now: Instant) -> Option<u64> {
        match (self.candidate, candidate) {
            (Some((old, since)), Some(current)) if old == current => {
                (now.duration_since(since) >= Duration::from_millis(1500)).then_some(current)
            }
            (_, Some(current)) => {
                self.candidate = Some((current, now));
                None
            }
            (_, None) => {
                self.candidate = None;
                None
            }
        }
    }
}
fn ready_window(pids: &HashSet<u32>) -> Option<u64> {
    struct Search<'a> {
        pids: &'a HashSet<u32>,
        best: Option<(u64, i64)>,
    }
    unsafe extern "system" fn visit(hwnd: HWND, param: LPARAM) -> windows::core::BOOL {
        let search = unsafe { &mut *(param.0 as *mut Search<'_>) };
        unsafe {
            let mut pid = 0;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            if !search.pids.contains(&pid)
                || !IsWindowVisible(hwnd).as_bool()
                || IsIconic(hwnd).as_bool()
                || IsHungAppWindow(hwnd).as_bool()
            {
                return true.into();
            }
            let mut rect = RECT::default();
            if GetClientRect(hwnd, &mut rect).is_err() || rect.right < 320 || rect.bottom < 180 {
                return true.into();
            }
            let mut title = [0u16; 256];
            let len = GetWindowTextW(hwnd, &mut title);
            let title = String::from_utf16_lossy(&title[..len.max(0) as usize]).to_lowercase();
            if title.contains("splash") || title.contains("crash reporter") {
                return true.into();
            }
            let area = rect.right as i64 * rect.bottom as i64;
            if search.best.is_none_or(|(_, old)| area > old) {
                search.best = Some((hwnd.0 as usize as u64, area));
            }
        }
        true.into()
    }
    let mut search = Search { pids, best: None };
    unsafe {
        let _ = EnumWindows(
            Some(visit),
            LPARAM((&mut search as *mut Search<'_>) as isize),
        );
    }
    search.best.map(|(hwnd, _)| hwnd)
}
#[derive(Clone, Debug)]
pub struct Process {
    pub pid: u32,
    pub parent: u32,
    pub path: String,
}
pub fn processes() -> windows::core::Result<Vec<Process>> {
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)?;
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut out = Vec::new();
        let mut next = Process32FirstW(snap, &mut entry);
        while next.is_ok() {
            let mut session = 0;
            if entry.th32ProcessID != 0
                && !(ProcessIdToSessionId(entry.th32ProcessID, &mut session).is_ok()
                    && session == 0)
            {
                let mut path = String::new();
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
                        path = String::from_utf16_lossy(&buffer[..len as usize]);
                    }
                    let _ = CloseHandle(handle);
                }
                out.push(Process {
                    pid: entry.th32ProcessID,
                    parent: entry.th32ParentProcessID,
                    path,
                });
            }
            next = Process32NextW(snap, &mut entry);
        }
        let _ = CloseHandle(snap);
        match next {
            Err(error) if error.code() != ERROR_NO_MORE_FILES.to_hresult() => Err(error),
            _ => Ok(out),
        }
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
impl StartedProcess {
    fn pid(&self) -> Result<u32, String> {
        let pid = unsafe { GetProcessId(self.0) };
        (pid != 0)
            .then_some(pid)
            .ok_or_else(|| "The launch returned no trackable process.".into())
    }
}
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
fn registry_string(root: HKEY, key: &str, name: &str) -> Option<String> {
    let key = wide(key);
    let name = wide(name);
    let mut buffer = [0u16; 32768];
    let mut size = std::mem::size_of_val(&buffer) as u32;
    unsafe {
        RegGetValueW(
            root,
            PCWSTR(key.as_ptr()),
            PCWSTR(name.as_ptr()),
            RRF_RT_REG_SZ,
            None,
            Some(buffer.as_mut_ptr().cast()),
            Some(&mut size),
        )
        .ok()
        .ok()?;
    }
    let len = buffer.iter().position(|c| *c == 0).unwrap_or(buffer.len());
    Some(String::from_utf16_lossy(&buffer[..len]))
}
fn steam_client(live: &[Process]) -> Option<String> {
    let registered = registry_string(HKEY_CURRENT_USER, r"Software\Valve\Steam", "SteamExe")
        .into_iter()
        .chain(
            [r"SOFTWARE\WOW6432Node\Valve\Steam", r"SOFTWARE\Valve\Steam"]
                .into_iter()
                .filter_map(|key| {
                    registry_string(HKEY_LOCAL_MACHINE, key, "InstallPath").map(|path| {
                        Path::new(&path)
                            .join("steam.exe")
                            .to_string_lossy()
                            .into_owned()
                    })
                }),
        );
    registered
        .chain(
            live.iter()
                .filter(|p| {
                    Path::new(&p.path).file_name().is_some_and(|name| {
                        name.to_string_lossy().eq_ignore_ascii_case("steam.exe")
                    })
                })
                .map(|p| p.path.clone()),
        )
        .find(|path| Path::new(path).is_file())
}
fn steam_plan(client: String, app: &str) -> Result<LaunchPlan, String> {
    if app.is_empty() || !app.bytes().all(|b| b.is_ascii_digit()) || app.parse::<u64>().is_err() {
        return Err(format!("Invalid Steam game ID: {app}"));
    }
    Ok(LaunchPlan {
        kind: "process".into(),
        executable: client,
        arguments: format!("-silent \"steam://rungameid/{app}\""),
        ..Default::default()
    })
}
struct ShellApartment;
impl Drop for ShellApartment {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}
#[derive(Default)]
struct TrackingFailures(u8);
impl TrackingFailures {
    fn observe(&mut self, success: bool) -> bool {
        self.0 = if success { 0 } else { self.0 + 1 };
        self.0 >= 5
    }
}
pub fn start(plan: LaunchPlan, tx: Sender<LaunchEvent>, cancel: Arc<AtomicBool>) {
    std::thread::spawn(move || {
        if cancel.load(Ordering::Relaxed) {
            return;
        }
        let fail = |message: String| {
            let _ = tx.send(LaunchEvent::Failed(message));
        };
        if !plan.error.is_empty() {
            fail(plan.error);
            return;
        }
        // Shell execution can invoke COM handlers. Each launch worker needs its
        // own apartment, independent of GPUI's UI thread initialization.
        if let Err(error) =
            unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) }.ok()
        {
            fail(format!(
                "Could not initialize Windows launch services: {error}"
            ));
            return;
        }
        let _apartment = ShellApartment;
        let before = match processes() {
            Ok(live) => live,
            Err(error) => {
                fail(format!("Could not inspect running processes: {error}"));
                return;
            }
        };
        let baseline: HashSet<u32> = before.iter().map(|p| p.pid).collect();
        let mut tracked = HashSet::new();
        let mut _child: Option<StartedProcess> = None;
        if plan.kind == "steam" {
            // Match Playnite's SteamPlayController: invoke the Steam executable
            // directly with its URI, rather than relying on a URL association.
            let Some(client) = steam_client(&before) else {
                fail("Steam's executable could not be found. Open Steam and try again.".into());
                return;
            };
            let launch = match steam_plan(client, &plan.steam_id) {
                Ok(launch) => launch,
                Err(error) => {
                    fail(error);
                    return;
                }
            };
            if cancel.load(Ordering::Relaxed) {
                return;
            }
            match start_process(&launch) {
                Ok(child) => _child = Some(child),
                Err(error) => {
                    fail(format!(
                        "Steam could not launch app {}: {error} (client: {})",
                        plan.steam_id, launch.executable
                    ));
                    return;
                }
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
            if cancel.load(Ordering::Relaxed) {
                return;
            }
            match start_process(&plan) {
                Ok(c) => {
                    let pid = match c.pid() {
                        Ok(pid) => pid,
                        Err(error) => {
                            fail(error);
                            return;
                        }
                    };
                    tracked.insert(pid);
                    let _ = tx.send(LaunchEvent::Started);
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
        let mut started = plan.kind == "process";
        let mut uncovered = false;
        let mut expired = false;
        let mut next_lifecycle = clock;
        let mut failures = TrackingFailures::default();
        let mut readiness = Readiness::default();
        let mut gone = 0u8;
        let install = plan
            .install_dir
            .trim_end_matches(['\\', '/'])
            .to_lowercase()
            + "\\";
        loop {
            // Kept Steam fusion retains its original sleep-first cadence.
            if plan.kind == "steam" {
                std::thread::sleep(Duration::from_millis(350));
            }
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            let now = Instant::now();
            let lifecycle = plan.kind == "steam" || now >= next_lifecycle;
            if lifecycle {
                next_lifecycle = now + Duration::from_secs(2);
            }
            let live = match processes() {
                Ok(live) => {
                    if lifecycle {
                        failures.observe(true);
                    }
                    live
                }
                Err(error) => {
                    if lifecycle && failures.observe(false) {
                        fail(format!(
                            "Process tracking failed; the game is now untracked: {error}"
                        ));
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(350));
                    continue;
                }
            };
            descendants(&live, &mut tracked);
            let game_pids: HashSet<u32> = live
                .iter()
                .filter(|p| matches_game(p, &tracked, &baseline, plan.kind == "steam", &install))
                .map(|p| p.pid)
                .collect();
            let matched = !game_pids.is_empty();
            let active = matched || (plan.kind == "steam" && steam_running(&plan.steam_id));
            if !uncovered {
                if let Some(hwnd) = readiness.update(ready_window(&game_pids), Instant::now()) {
                    uncovered = true;
                    let _ = tx.send(LaunchEvent::Ready(hwnd));
                } else if !expired && clock.elapsed() > Duration::from_secs(90) {
                    expired = true;
                    let _ = tx.send(LaunchEvent::ReadinessExpired);
                }
            }
            if lifecycle && active {
                gone = 0;
                if !started {
                    started = true;
                    let _ = tx.send(LaunchEvent::Started);
                }
            } else if lifecycle
                && !active
                && started
                && end_observation_allowed(
                    plan.kind == "steam",
                    uncovered || expired,
                    clock.elapsed(),
                )
            {
                gone += 1;
                if plan.kind == "process" || gone >= 3 {
                    let _ = tx.send(LaunchEvent::Ended);
                    break;
                }
            } else if plan.kind == "steam"
                && !active
                && !started
                && clock.elapsed() > Duration::from_secs(90)
            {
                fail("The game did not start within 90 seconds. Return to the library and try again.".into());
                break;
            }
            if plan.kind != "steam" {
                std::thread::sleep(if uncovered || expired {
                    Duration::from_secs(2)
                } else {
                    Duration::from_millis(350)
                });
            }
        }
    });
}

// Steam bootstrap/anti-cheat processes can exit before the game process exists.
// Keep the launch attempt alive until readiness or the existing startup deadline.
fn end_observation_allowed(steam: bool, uncovered: bool, elapsed: Duration) -> bool {
    !steam || uncovered || elapsed >= Duration::from_secs(90)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancelled_worker_exits_before_preflight_or_dispatch() {
        let (tx, rx) = std::sync::mpsc::channel();
        start(
            LaunchPlan {
                error: "must not be dispatched".into(),
                ..Default::default()
            },
            tx,
            Arc::new(AtomicBool::new(true)),
        );
        assert!(matches!(
            rx.recv_timeout(Duration::from_secs(1)),
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected)
        ));
    }
    #[test]
    fn snapshot_failure_is_unknown_until_five_consecutive_observations() {
        let mut failures = TrackingFailures::default();
        for _ in 0..4 {
            assert!(!failures.observe(false));
        }
        assert!(!failures.observe(true));
        for _ in 0..4 {
            assert!(!failures.observe(false));
        }
        assert!(failures.observe(false));
    }
    #[test]
    fn missing_process_handle_cannot_seed_a_system_process_tree() {
        assert!(StartedProcess(HANDLE::default()).pid().is_err());
    }
    #[test]
    fn native_membership_survives_an_unknown_image_path() {
        let mut tracked = HashSet::from([10]);
        descendants(
            &[Process {
                pid: 10,
                parent: 9,
                path: String::new(),
            }],
            &mut tracked,
        );
        assert_eq!(tracked, HashSet::from([10]));
    }
    #[test]
    fn steam_bootstrap_gap_is_not_a_completed_game_session() {
        assert!(!end_observation_allowed(
            true,
            false,
            Duration::from_secs(5)
        ));
        assert!(!end_observation_allowed(
            true,
            false,
            Duration::from_secs(89)
        ));
        assert!(end_observation_allowed(
            true,
            false,
            Duration::from_secs(90)
        ));
        assert!(end_observation_allowed(true, true, Duration::from_secs(5)));
        assert!(end_observation_allowed(
            false,
            false,
            Duration::from_secs(5)
        ));
    }
    #[test]
    fn replacement_and_disappearing_windows_restart_readiness() {
        let now = Instant::now();
        let mut r = Readiness::default();
        assert_eq!(r.update(Some(1), now), None);
        assert_eq!(r.update(Some(1), now + Duration::from_millis(1400)), None);
        assert_eq!(r.update(Some(2), now + Duration::from_millis(1500)), None);
        assert_eq!(r.update(None, now + Duration::from_millis(2900)), None);
        assert_eq!(r.update(Some(2), now + Duration::from_millis(3000)), None);
        assert_eq!(
            r.update(Some(2), now + Duration::from_millis(4500)),
            Some(2)
        );
    }
    #[test]
    fn steam_id_cannot_inject_additional_client_arguments() {
        assert!(
            steam_plan("steam.exe".into(), "4356430")
                .unwrap()
                .arguments
                .contains("steam://rungameid/4356430")
        );
        for id in ["", "1\" -shutdown", "abc", "18446744073709551616"] {
            assert!(steam_plan("steam.exe".into(), id).is_err());
        }
    }
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
