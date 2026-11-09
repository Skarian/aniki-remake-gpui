# Playnite lifecycle alignment

Reference: [JosefNemec/Playnite at e2e6efc](https://github.com/JosefNemec/Playnite/tree/e2e6efcda311fea38fcd92f61c8ca90811d97f28). Inspected the existing pinned checkout read-only; did not fetch or modify it.

| Concern | Playnite source | This launcher |
| --- | --- | --- |
| Direct executable launch | `source/Playnite/Common/ProcessStarter.cs`, `StartProcess`: `ProcessStartInfo` with shell execution, arguments, and working directory | `src/launch.rs`, `start_process`: `ShellExecuteExW`, argument string, working directory, retained process handle |
| Default emulator lifetime | `source/Playnite/Controllers/GenericGameController.cs`, `StartEmulatorProcess`: `MonitorProcessTree` | Launched PID plus descendants; no unrelated executable-directory matching for direct launches |
| Process-tree membership | `source/Playnite/Common/ProcessMonitor.cs`, `MonitorProcessTree`: retain currently running related IDs | Follow descendants and prune exited IDs on each observation |
| Details on return | `source/Playnite.FullscreenApp/ViewModels/FullscreenAppViewModel.cs`, `Controllers_Stopped`: remove status view and refocus existing Details/list | Preserve the existing view state, selection, collection, and Details scroll position |
| Restore focus | `source/Playnite/Windows/WindowFactory.cs`, `RestoreWindow`: show/restore, activate, temporarily attach input threads | `src/focus.rs` uses the equivalent Win32 operations and explicitly manages fullscreen topmost state |
| UI lifetime | Existing application/window remains alive | One retained GPUI window, minimized during gameplay; no black-cover host or frontend restart |

## Intentional differences

- The renderer and media are GPUI/FFmpeg/SDL rather than WPF. Minimized render surfaces shrink and decoded artwork is released. Keeping the process/window alive uses more memory than completely exiting the frontend, but avoids another lifetime/IPC layer.
- The launcher waits for a tracked game's actual foreground window before minimizing. Process creation alone is not proof the game is visible.
- Process polling is 350 ms with three absent samples. The referenced Playnite generic controller defaults to 2-second polling; its fullscreen stop path also adds a 1-second restore delay. The polling/restore timing is not claimed to be identical.
- Steam uses a focused adapter with the registered Steam URI, installation-directory process matching, and Steam running-app state. It is not a port of Playnite's entire Steam extension.
- Advanced tracking modes, hooks, extension callbacks, and playtime/database writes are not implemented. Unsupported launch scripts/actions are rejected.
- Repeated launches use a Windows named mutex/event to activate the retained window. Extra invocations wait for that session to end for external launcher compatibility.

A window disappearing does **not** mean a game process exited. The temporary black-cover recovery heuristic was removed with that architecture. If an emulator leaves its process running after closing its window, opening the launcher again restores its existing window; Back dismisses its running status without stopping process tracking.

Launch resolution and variable-expansion behavior adapted from Playnite is covered by its MIT notice in `THIRD_PARTY_NOTICES.txt`. Integration tests in `tests/retained_lifecycle.py` verify the same PID/HWND survives launch and return.
