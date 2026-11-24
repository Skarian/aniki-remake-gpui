# Playnite behavior

The launch and return flow follows Playnite's fullscreen mode. Reference source: [JosefNemec/Playnite at `e2e6efc`](https://github.com/JosefNemec/Playnite/tree/e2e6efcda311fea38fcd92f61c8ca90811d97f28).

| Behavior | Playnite | This launcher |
| --- | --- | --- |
| Start an executable | `ProcessStarter.StartProcess`: shell execute with arguments and working directory | `launch.rs`: `ShellExecuteExW`, keeping the process handle |
| Emulator lifetime | `GenericGameController.StartEmulatorProcess`: process tree | Launched PID plus descendants |
| Process tree | `ProcessMonitor.MonitorProcessTree`: keep running related IDs | Follow descendants, drop exited IDs each scan |
| Storefront games | Library plugin's play controller | The same plugin DLL, in a worker process ([plugin host](plugin-host.md)) |
| Return from a game | `FullscreenAppViewModel.Controllers_Stopped`: remove status view, refocus Details | Same Details page, collection and scroll position |
| Restore focus | `WindowFactory.RestoreWindow`: restore, activate, attach input threads | `focus.rs`: the same Win32 calls |
| Window during play | Stays open; minimizes if `MinimizeAfterGameStartup` is on | Stays open with the status screen (Playnite's `MinimizeAfterGameStartup=false` behavior) |

## Launch sequence

1. The status screen shows the game's background, logo and an orbit animation. Its artwork is resolved before launch, and the launch is dispatched on the next frame. If artwork is slow, an opaque fallback appears after two seconds.
2. The launch cover stays topmost until a game window is ready: visible, not minimized, responsive, at least 320×180, not a splash or crash reporter, and stable for 1.5 seconds. Checks run every 350 ms. This matches the readiness check of the Aniki ReMake theme's helper, without a separate splash window.
3. Focus passes to the game and the launcher drops topmost. The status screen stays underneath. Music and video stop, and the animation pauses while the launcher is in the background.
4. If no window is ready after 90 seconds, the cover is released and tracking continues. A window that becomes ready later still gets focus.

Back before Started cancels the launch without killing anything. Back after Started dismisses the cover and turns off automatic return for that session.

## Tracking and return

Native tracking scans immediately, then every two seconds, and ends at the first complete scan with no tracked process. Before handoff, descendants are found every 350 ms. Failed scans count as unknown; five failures in a row release tracking with an error and do not claim the game exited.

When the game exits, the launcher waits one second, then returns the same window to Details and takes focus. Held buttons must be released before input resumes. If you browsed elsewhere during play, your place is kept.

An emulator that closes its window but keeps running is still a running game, as in Playnite. Opening the launcher again shows its status screen; Back returns to the library without stopping tracking. Only one game session runs at a time.

## Differences from Playnite

- Rendering uses GPUI, FFmpeg and SDL instead of WPF.
- Not implemented: scripts, multi-action selection, advanced tracking modes, extension hooks, and playtime or database writes. Unsupported actions show an error.
- Process IDs and missing paths are checked before dispatch, so a bad action fails early.
- A second start of the same executable activates the open window. While a game runs, the extra process waits for the session to end, so external launchers such as Apollo see a live process.

Launch-variable expansion follows Playnite's `GameExtensions.cs` and is covered by its MIT notice in [THIRD_PARTY_NOTICES.txt](../THIRD_PARTY_NOTICES.txt).

## Earlier Steam tracking

Before the [plugin host](plugin-host.md) (2026-11-21), Steam games launched natively with `steam.exe -silent "steam://rungameid/<id>"` and were tracked by install directory. That migration was checked on 2026-11-10 against two Playnite reference traces of HELLDIVERS 2 sampled every 200 ms; replaying them at two-second sampling, across the tested phase offsets, gave exactly one start and one stop. The code remains only for plans cached by that build.
