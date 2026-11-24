# Development

## How it works

Playnite Fullscreen GPUI is one Rust executable with a GPUI window, plus two small .NET Framework helpers that read Playnite data.

- **Library.** A managed importer (`PlayniteFullscreen.Import.exe`) reads Playnite's LiteDB database with Playnite's own DLLs and writes `cache/raw.json`. Rust normalizes that into `cache/library.json` and resizes artwork into `cache/artwork`. At startup the cached library is ready within about 20 ms of process start, then a background thread refreshes it when the database changes. An optional Playnite extension (`PlayniteFullscreen.LiveExport.dll`, installed with `importer/install-live-export.ps1 -PlayniteRoot <program-folder> [-PlayniteDataRoot <data-folder>]`) can also push updates to `cache/live.json` while Playnite runs. The importer only reads the database.
- **UI.** GPUI renders Home, Details, the trailer page, the menu and the game status screen in one fullscreen window. Animations request frames only while they run.
- **Input.** XInput handles Xbox controllers. SDL handles PlayStation and other controllers on a background thread, kept off the first-frame path. Keyboard and mouse come from GPUI. All sources work at once, and prompts follow the last device used.
- **Media.** FFmpeg decodes music and trailers in child processes. Audio plays through an SDL queue. Video keeps the source frame rate up to 60 fps and decodes at the smallest of source size, display size and 3840×2160. `ffprobe` supplies the source size; without it, decoding uses 1080p.
- **Launching.** Storefront games launch through the installed Playnite library plugin, hosted in a separate worker process. See [plugin host](plugin-host.md). Emulator and executable actions start directly and are tracked by process tree.
- **Game session.** The window stays open behind the game and shows its status screen. When the tracked game exits, the same window returns to the game's Details page. See [Playnite behavior](playnite-alignment.md).
- **Single instance.** A named mutex and event make a second start activate the open window instead of creating another.

### Source layout

| Path | Contents |
| --- | --- |
| `src/main.rs` | App state, config, input routing, startup |
| `src/screens.rs` | Page layouts |
| `src/motion.rs` | Animation timing |
| `src/library.rs` | Import, cache, refresh, launch plans |
| `src/artwork.rs` | Decoded artwork cache |
| `src/media.rs` | FFmpeg music and video |
| `src/controller.rs` | XInput and SDL controllers |
| `src/launch.rs` | Process start and game tracking |
| `src/plugin.rs` | Plugin worker protocol |
| `src/focus.rs` | Window focus and stacking |
| `src/instance.rs` | Single-instance activation |
| `importer/` | Importer, live exporter and plugin host (C#), built as `PlayniteFullscreen.*` |
| `vendor/gpui` | GPUI 0.2.2 with Windows patches listed in `vendor/GPUI-PATCH.txt` |
| `tests/` | Integration tests and fixtures |

## Build

Requirements: Windows x64, Rust with the MSVC toolchain (pinned in `rust-toolchain.toml`), Visual Studio C++ Build Tools with CMake, the Windows SDK and .NET Framework 4.x.

```powershell
.\importer\build.ps1 -PlayniteRoot 'D:\Playnite'
.\build.ps1 -Test
.\build.ps1 -Release
.\package.ps1
```

`importer/build.ps1` compiles the C# helpers against your Playnite DLLs. `build.ps1` uses a project-local toolchain in `.tools` if present, otherwise your Cargo. SDL and the C runtime are linked statically. Without `-Portable`, `package.ps1` copies the release build to the repository root so you can run it in place; copy `playnite-fullscreen-gpui.example.json` to `playnite-fullscreen-gpui.json` first.

To build the portable folder:

```powershell
.\package.ps1 -Portable -FFmpegRoot <ffmpeg-build> -OutputDirectory <new-folder>
```

The FFmpeg folder needs `bin/ffmpeg.exe`, `bin/ffprobe.exe` and `LICENSE.txt`. The package holds only the launcher, its helpers, prompt assets, the Apollo cover, docs, a clean config and FFmpeg. Playnite DLLs are never packaged. Releases are built by CI; see [releases](releases.md).

## Test

| Command | Covers |
| --- | --- |
| `.\build.ps1 -Test` | Rust unit tests: launch plans, variable expansion, tracking, input, cache, motion |
| `python tests/library_layout.py --importer <importer-exe> --launcher <exe>` | Import from separate data and library folders; database left unchanged |
| `python tests/plugin_worker.py --exe <exe>` | Plugin worker contract against a fixture plugin |
| `python tests/storefront_actions.py --exe <exe> --plugins <plugin-folders>` | Real Epic and GOG plugin DLLs with synthetic games |
| `cargo test --bin playnite-fullscreen-gpui static_sdl_ -- --ignored --test-threads=1` | Virtual controller and silent audio through static SDL (set `ANIKI_TEST_FFMPEG`) |
| `python tests/retained_lifecycle.py --exe <exe> --expected-sha256 <hash>` | Window survives game cycles with Xbox and PS4 input; lingering, crashed and missing games; duplicate starts |
| `python tests/selection_flow.py` | Navigation, Details actions, menu and video with virtual controllers |

Pass `--playnite <program-folder>` to the Python tests when Playnite is not in `D:\Playnite`. For `plugin_worker.py` and `storefront_actions.py`, build the probe first with `cargo build --example launch_probe --locked` and pass `--probe target/debug/examples/launch_probe.exe`.

Controller tests need Python's `vgamepad` package and the ViGEm driver. Lifecycle and selection tests drive the real desktop, so leave the machine alone while they run. All tests write to the ignored `artifacts` folder.

## Diagnostics

- `--preview-screen home|details|menu|launching|running|error|empty|empty-library|loading` shows a screen without launching anything. Add `--preview-input keyboard|xbox|playstation` to choose prompts.
- `--windowed` starts in a window.
- `--trace-motion` and `--trace-memory` write animation and memory traces.
- Panics are written to `artifacts/panic.log` with a backtrace.

See [benchmarks](benchmarks.md) for startup and resource measurement.

## Credits

Launch-variable expansion and its tests follow [Playnite](https://github.com/JosefNemec/Playnite) at `e2e6efc` (`GameExtensions.cs`, `GameTests.cs`, `GenericGameController.cs`), under its MIT license. Button prompts are from [Meritite Union input-prompts](https://github.com/meritite-union/input-prompts) (CC0); sources are in `assets/prompts`. GPUI is Apache-2.0 and SDL is zlib. See [THIRD_PARTY_NOTICES.txt](../THIRD_PARTY_NOTICES.txt).
