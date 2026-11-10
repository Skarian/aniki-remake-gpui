# Playnite lifecycle alignment

Reference: [JosefNemec/Playnite at e2e6efc](https://github.com/JosefNemec/Playnite/tree/e2e6efcda311fea38fcd92f61c8ca90811d97f28). Inspected the existing pinned checkout read-only; did not fetch or modify it.

| Concern | Playnite source | This launcher |
| --- | --- | --- |
| Direct executable launch | `source/Playnite/Common/ProcessStarter.cs`, `StartProcess`: `ProcessStartInfo` with shell execution, arguments, and working directory | `src/launch.rs`, `start_process`: `ShellExecuteExW`, argument string, working directory, retained process handle |
| Default emulator lifetime | `source/Playnite/Controllers/GenericGameController.cs`, `StartEmulatorProcess`: `MonitorProcessTree` | Launched PID plus descendants; no unrelated executable-directory matching for direct launches |
| Process-tree membership | `source/Playnite/Common/ProcessMonitor.cs`, `MonitorProcessTree`: retain currently running related IDs | Follow descendants and prune exited IDs on each observation |
| Details on return | `source/Playnite.FullscreenApp/ViewModels/FullscreenAppViewModel.cs`, `Controllers_Stopped`: remove status view and refocus existing Details/list | Preserve the existing view state, selection, collection, and Details scroll position |
| Restore focus | `source/Playnite/Windows/WindowFactory.cs`, `RestoreWindow`: show/restore, activate, temporarily attach input threads | `src/focus.rs` uses the equivalent Win32 operations and explicitly manages fullscreen topmost state |
| UI lifetime | Existing application/window remains alive; `GamesEditor.cs` minimizes only when `Fullscreen.MinimizeAfterGameStartup` is enabled | One visible GPUI window with game-status artwork underneath gameplay; no minimization, black-cover host, or frontend restart |

## Intentional differences

- The renderer and media are GPUI/FFmpeg/SDL rather than WPF. The retained fullscreen surface and selected background/logo/cover cost more memory than a minimized window. Video, audio, outgoing transitions, and unrelated artwork are released during gameplay.
- This matches Playnite's supported `MinimizeAfterGameStartup=false` mode; Playnite defaults that option to true. Status artwork is resolved before launch, and dispatch is queued from the status render for the following frame. Missing or stalled artwork has an opaque fallback after two seconds. Launching-to-Running and return transitions do not fade the whole scene to black.
- The orbit animation updates at roughly 30 Hz while active and retains its last frame while inactive. No repeated z-order enforcement is used. Focus restoration does not displace an unrelated foreground app. Held input is gated on focus regain and return.
- Process polling is 350 ms with three absent samples. The referenced Playnite generic controller defaults to 2-second polling; its fullscreen stop path also adds a 1-second restore delay. The polling/restore timing is not claimed to be identical.
- Steam invokes the installed client directly with `-silent "steam://rungameid/<id>"`, matching `SteamPlayController.Play` in the locally installed `SteamLibrary_Builtin/SteamLibrary.dll`. Registry client discovery falls back to an existing Steam process. Launch workers initialize COM and preserve the underlying Windows error. Installation-directory matching and Steam running-app state provide lifetime tracking; this is not a port of the entire extension.
- Advanced tracking modes, hooks, extension callbacks, and playtime/database writes are not implemented. Unsupported launch scripts/actions are rejected.
- Repeated launches use a Windows named mutex/event to activate the retained window. Extra invocations wait for that session to end for external launcher compatibility.

A window disappearing does **not** mean a game process exited. The temporary black-cover recovery heuristic was removed with that architecture. If an emulator leaves its process running after closing its window, opening the launcher again restores its existing window; Back dismisses its running status without stopping process tracking.

## Installed Aniki splash comparison (2026-11-09)

The local theme is Aniki ReMake 3.6.0 with AnikiHelper 1.7.0. Read-only inspection of the installed helper assembly and configuration showed a separate opaque, topmost splash window. This PC has that feature enabled, with a 2.4-second minimum and a 90-second maximum. Its readiness service checks the same candidate process/window three times at 500 ms intervals and filters hidden, minimized, hung, small, and splash-like windows. Thus core Playnite's window behavior alone does not describe this PC's launch transition.

Our existing status window supplies that protection without a second splash window: a tracked game's visible, non-minimized, responsive client window (at least 320×180, excluding splash/crash-reporter titles) must remain the candidate for 1.5 seconds. Polling is 350 ms. Replaced or disappearing candidates restart the interval. Then focus is handed over and the status window loses topmost status. An unrelated foreground application is not forcibly displaced. Back can dismiss the cover, and the 90-second timeout releases it while tracking continues. This approximates Aniki's readiness behavior; it does not duplicate all of its title/class/coverage heuristics or its fixed minimum duration.

Launch resolution and variable-expansion behavior adapted from Playnite is covered by its MIT notice in `THIRD_PARTY_NOTICES.txt`. Interactive tests in `tests/retained_lifecycle.py` cover retained PID/HWND visibility, foreground ownership, lingering/crashed games, manual navigation, and held Xbox/PS return input. These tests take control of the desktop and must be run only when it is available. Source/unit checks cannot establish frame-by-frame absence of desktop exposure; real Steam/splash and exclusive-fullscreen transitions also require runtime checks.
