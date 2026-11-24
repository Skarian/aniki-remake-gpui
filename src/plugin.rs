//! One selected Playnite library DLL, isolated in another instance of this exe.
use crate::library::LaunchPlan;
use serde::{Deserialize, Serialize};
use std::{
    io::{BufRead, Write},
    os::windows::process::CommandExt,
    process::{Child, Command, Stdio},
    sync::mpsc,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Request {
    pub root: String,
    #[serde(default)]
    pub data_root: String,
    pub module: String,
    pub game: serde_json::Value,
}
#[derive(Debug, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    Automatic { plan: LaunchPlan },
    Started { pid: u32 },
    Stopped,
    Error { message: String },
}
pub struct Worker {
    child: Child,
    pub events: mpsc::Receiver<Event>,
}
impl Worker {
    pub fn start(request: &Request) -> Result<Self, String> {
        let mut child = Command::new(std::env::current_exe().map_err(|e| e.to_string())?)
            .arg("--plugin-worker")
            .creation_flags(0x08000000)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("Cannot start plugin worker: {e}"))?;
        let (tx, events) = mpsc::channel();
        let output = child.stdout.take().unwrap();
        let mut worker = Self { child, events };
        // Keep stdin open: EOF also stops an orphaned worker if the UI crashes.
        writeln!(
            worker.child.stdin.as_mut().unwrap(),
            "{}",
            serde_json::to_string(request).unwrap()
        )
        .map_err(|e| e.to_string())?;
        std::thread::spawn(move || {
            for line in std::io::BufReader::new(output).lines() {
                let Ok(line) = line else { break };
                // Plugins may write their own console output. Only our envelope is IPC.
                if let Some(json) = line.strip_prefix("ANIKI:") {
                    let event = serde_json::from_str(json).unwrap_or_else(|e| Event::Error {
                        message: format!("Invalid plugin response: {e}"),
                    });
                    if tx.send(event).is_err() {
                        break;
                    }
                }
            }
        });
        Ok(worker)
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        // Kill only the host, never its game/client children. Dispose is not a
        // cancellation contract for every Playnite plugin.
        let _ = self.child.kill();
        let _ = self.child.wait();
        let scratch = std::env::temp_dir().join(format!("aniki-plugin-{}", self.child.id()));
        let _ = std::fs::remove_dir_all(scratch);
    }
}

pub fn worker_main() -> i32 {
    let run = || -> anyhow::Result<u32> {
        use windows::{
            Win32::System::ClrHosting::*,
            core::{GUID, Interface, PCWSTR, w},
        };
        let mut json = String::new();
        std::io::stdin().read_line(&mut json)?;
        let _: Request = serde_json::from_str(&json)?;
        let assembly = std::env::current_exe()?
            .parent()
            .unwrap()
            .join("importer/PlayniteFullscreen.PluginHost.dll");
        anyhow::ensure!(
            assembly.is_file(),
            "Plugin host is missing. Run importer/build.ps1."
        );
        let wide = |s: &str| s.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
        let assembly = wide(&assembly.to_string_lossy());
        let json = wide(&json);
        unsafe {
            let mut raw = std::ptr::null_mut();
            CorBindToRuntimeEx(
                w!("v4.0.30319"),
                w!("wks"),
                0,
                &GUID::from_u128(0x90f1a06e_7712_4762_86b5_7a5eba6bdb02),
                &ICLRRuntimeHost::IID,
                &mut raw,
            )?;
            let host = ICLRRuntimeHost::from_raw(raw);
            host.Start()?;
            Ok(host.ExecuteInDefaultAppDomain(
                PCWSTR(assembly.as_ptr()),
                w!("PlayniteFullscreenPluginHost"),
                w!("Run"),
                PCWSTR(json.as_ptr()),
            )?)
        }
    };
    match run() {
        Ok(code) => code as i32,
        Err(e) => {
            println!(
                "ANIKI:{}",
                serde_json::json!({"event":"error", "message":format!("Plugin runtime: {e:#}")})
            );
            1
        }
    }
}
