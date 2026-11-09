# aniki-remake-gpui

A minimal Windows fullscreen launcher built with Rust and GPUI. It reads a Playnite library and its existing artwork, logos, trailers, music, and platform images. Playnite does not have to remain open.

Home has one action: select a game. Details contains Play and Watch video. The menu is on Home only. Xbox, native DualShock 4 / DualSense, keyboard, and mouse input are supported. Physical DualSense USB/Bluetooth still needs hardware verification.

## Controls

| Action | Xbox | PlayStation | Keyboard |
| --- | --- | --- | --- |
| Browse / choose action | D-pad / left stick | D-pad / left stick | Arrow keys |
| Change collection | LB / RB | L1 / R1 | Q / E |
| Select / confirm | A | Cross | Enter |
| Back / Home menu | B / Start | Circle / Options | Escape / F1 |
| Music | X | Square | M |
| Watch video | Y | Triangle | V |
| Fullscreen | | | F11 |

## Build and run

Requires Windows x64, Rust with the MSVC toolchain, Visual Studio C++ Build Tools, the Windows SDK, and .NET Framework 4.x. GPUI 0.2.2 is vendored with documented Windows lifecycle patches. Cargo.lock pins the dependency graph.

1. Copy `aniki-remake-gpui.example.json` to `aniki-remake-gpui.json`. Set `playnite_root` to your Playnite installation/data directory and `ffmpeg` to your FFmpeg executable if you want trailers.
2. Place the **x64 SDL2.dll** runtime beside the executable. The tested version is SDL 2.30.5. Playnite may ship a 32-bit DLL, which cannot be used here. See [SDL releases](https://github.com/libsdl-org/SDL/releases) and the bundled third-party notices.
3. Build the read-only importer and launcher:

```powershell
.\importer\build.ps1 -PlayniteRoot 'D:\Playnite'
.\build.ps1 -Test
.\build.ps1 -Release
.\package.ps1
.\aniki-remake-gpui.exe
```

`build.ps1` uses a project-local toolchain under `.tools` when present, otherwise your installed Cargo. The importer uses the LiteDB and Newtonsoft.Json assemblies from your Playnite installation. No game media, personal library snapshots, configuration, or toolchains are included in this repository.

Optionally run `importer/install-live-export.ps1 -PlayniteRoot 'D:\Playnite'`, then restart Playnite. It exports metadata into this project's `cache/live.json`. Re-run after moving the project. Neither importer nor exporter edits Playnite's library. A background refresh preserves the last valid snapshot if the database is locked or an export fails.

## Apollo

Set Apollo's command to the full path of `aniki-remake-gpui.exe` and its working directory to the containing folder. The cover is [assets/apollo/aniki-remake-gpui-cover.png](assets/apollo/aniki-remake-gpui-cover.png). Apollo is optional; the same executable works standalone.

## Game lifecycle

One binary runs a small native session host and a GPUI frontend. Once a tracked game actually owns the foreground window, the frontend exits and releases its renderer, artwork, video, audio, and controller resources. The host retains a black window behind the game.

After game exit, the frontend prepares artwork under black, restores that game's Details page and collection, then fades in over 360 ms. Three inactive process samples debounce process handoffs. If the game previously took focus but has no visible windows for two seconds, the launcher also returns: an emulator can leave its process alive after closing its window. A minimized window still counts as present.

When the black cover has focus, a keyboard, mouse, or controller button brings the launcher back without terminating the game. The recovery input must be released before it can activate an action in the restored UI. Controller recovery initializes only while the cover has focus; the host does not process gameplay input.

Repeated invocations from the same executable path share one session. A repeat launch focuses the browsing UI and does not open another UI over gameplay. The extra invocation waits for the original session to end, preserving external launcher lifetime tracking.

## Compatibility and limits

Steam launches use the registered `steam://rungameid/` handler. Direct executable and emulator launches resolve Playnite profiles and Windows arguments. Tracking follows process descendants and new processes under the game's installation directory, with Steam's running state as an additional signal.

This is not full Playnite compatibility. Non-Steam store integrations, launch scripts, multi-action / multi-ROM selection, advanced custom tracking, library editing, achievements, downloads, and playtime writeback are not implemented. Unsupported actions produce an error. An emulator left running after its window closes may need to finish exiting or be closed before another instance can start; the launcher does not force-kill it.

Trailers use bounded FFmpeg decoding at up to 1080p/30 fps. The default is two decoder/filter/output threads; `video_threads: 0` enables FFmpeg auto sizing. Artwork uses a bounded cache, and media stops on focus loss or game launch. Exact long-duration trailer audio/video synchronization has not been measured.

## Verification

- `build.ps1 -Test`: Rust tests for launch resolution, tracking, input, cache ownership, and motion.
- `python -m unittest discover -s tests -p test_session_trace.py`: fade-checker tests.
- `python tests/session_lifecycle.py`: standalone launch, duplicate invocation, return, failure, and crash cleanup.
- `python tests/session_recovery.py`: real-window tests for lingering processes, replacement windows, minimized games, and Xbox / virtual DualShock 4 recovery. Builds its own WinForms fixture; needs no game library.
- `python tests/session_recovery.py keyboard`: the minimized-window case, waiting for keyboard input on the exposed cover.

Controller integration tests require Python's `vgamepad` package and an existing ViGEm driver. They do not install drivers. Tests create isolated copies under `artifacts`; do not run interactive tests while using another controller application.

`--trace-motion` and `--trace-memory` write local diagnostics under `artifacts`. Fade traces check focus, progress, completion, and render cadence. An occluded Windows compositor cannot establish visible smoothness; the trace reports that limit separately. Render callbacks are not physical display scanout or Apollo delivery.

For visual inspection without launching games, use `--preview-screen home|details|menu|launching|running|error|empty|empty-library|loading` with optional `--preview-input keyboard|xbox|playstation`. Preview mode does not edit configuration or refresh the library.

## Attribution

Inspired by Aniki's minimal fullscreen presentation. Game media belongs to its respective owners and is read from the user's library.

Launch-variable behavior and tests were adapted from [Playnite](https://github.com/JosefNemec/Playnite), reference commit `e2e6efcda311fea38fcd92f61c8ca90811d97f28`, particularly `GameExtensions.cs`, `GameTests.cs`, and `GenericGameController.cs`. The complete upstream test suite has not been ported.

Input glyphs are [Meritite Union input-prompts](https://github.com/meritite-union/input-prompts), CC0; editable sources and provenance are in `assets/prompts`. GPUI is Apache-2.0 with changes recorded in `vendor/GPUI-PATCH.txt`. SDL is zlib licensed. See [THIRD_PARTY_NOTICES.txt](THIRD_PARTY_NOTICES.txt).
