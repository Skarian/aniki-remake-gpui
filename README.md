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

The launcher keeps one fullscreen GPUI window visible. It presents the game's still background, logo, and orbit animation before dispatching launch. It holds that cover until a visible, responsive game window has remained stable for 1.5 seconds, then releases topmost status and hands over focus. That status screen remains underneath throughout gameplay. Music and video stop; only the game's background, logo, and Details cover remain cached. The animation pauses while the launcher is inactive. Back cancels an attempt before Started is observed, or dismisses the cover and disables automatic return after Started. Cancellation never kills a game; a late-starting game can run untracked. The 90-second readiness limit uncovers the desktop without abandoning process tracking; a late ready window can still receive focus.

When the tracked process tree exits, the **same window** returns to that game's Details page, collection, and scroll position. B returns Home. Held controller/keyboard input must be released first. If you deliberately browsed elsewhere during gameplay, your navigation is preserved. Automatic sessions deliberately bring the game forward and reclaim focus on return, including over another app. Back opts out. Completion runs independently of rendering, with a nonblocking one-second return delay; process polling can add up to two seconds. Ordinary browsing and return clear topmost; only the launch cover uses it. There is no black-cover window, minimization, replacement frontend, or return fade from black. A second invocation activates the existing window rather than creating a competing fullscreen interface.

An emulator that has closed its window but still has a live process is still running, as in Playnite. Opening the launcher again restores its existing window; Back returns from the running-game screen to the library without killing the game or discarding tracking. Starting another game is blocked until that tracked session finishes.

See [Playnite alignment](docs/playnite-alignment.md) for the source comparison and intentional differences.

## Compatibility and limits

Steam launches invoke the installed Steam client with `-silent "steam://rungameid/<id>"`, matching Playnite's Steam plugin without relying on Windows URL associations. Direct executable and emulator launches resolve Playnite profiles and Windows arguments. Direct launches use Windows shell execution and track the launched process tree. Steam follows Playnite?s directory tracker: the first matching process starts the session; the first later empty observation ends it. It scans immediately, then makes lifetime decisions every two seconds. A pre-existing matching process counts, and a lingering one keeps the session open. A never-started attempt remains cancellable; there is no startup deadline. Windows launch errors include their underlying error details.

This is not full Playnite compatibility. Non-Steam store integrations, launch scripts, multi-action / multi-ROM selection, advanced custom tracking, library editing, achievements, downloads, and playtime writeback are not implemented. Unsupported actions produce an error. An emulator left running after its window closes may need to finish exiting or be closed before another instance can start; the launcher does not force-kill it.

Trailers use bounded FFmpeg decoding at up to 1080p/30 fps. The default is two decoder/filter/output threads; `video_threads: 0` enables FFmpeg auto sizing. Artwork uses a bounded cache, and media stops on focus loss or game launch. Exact long-duration trailer audio/video synchronization has not been measured.

Music and trailer audio use FFmpeg and a bounded SDL stereo queue. Music loops, and switching selections closes the previous output before starting another. `music_volume` is linear gain from 0 to 1 (default 0.5); existing normalized Playnite tracks are read without rewriting them. Quiet introductions retain their source dynamics. No microphone is used by the optional `tests/audio_probe.py` speaker-loopback loudness test.

## Verification

- `build.ps1 -Test`: Rust tests for launch resolution, process tracking, input, cache ownership, and motion.
- `python tests/retained_lifecycle.py --exe <built-exe> --expected-sha256 <recorded-hash>`: real-window tests proving the launcher PID and HWND survive two game cycles each with Xbox and virtual DualShock 4 input; also covers lingering processes, explicit restore, duplicate invocation, missing executables, owner crashes, and normal exit. Builds its own WinForms fixture and needs no game library.
- Add `--artwork-from-cache` to exercise that lifecycle with real local background, logo, and cover images from the existing cache. The test reads those files without changing the library.
- `tests/selection_flow.py`: full preview navigation, Details actions, menu restrictions, and video with virtual Xbox / DualShock 4 controllers. See the script's startup instructions.

Controller integration tests require Python's `vgamepad` package and an existing ViGEm driver. They do not install drivers. Tests create isolated copies under `artifacts`; do not run interactive tests while using another controller application.

`--trace-motion` and `--trace-memory` write local diagnostics under `artifacts`. Render callbacks are not physical display scanout or Apollo delivery.

Rust panics in GUI builds are recorded in `artifacts/panic.log` with their source location and a backtrace, even when no terminal is attached.

For visual inspection without launching games, use `--preview-screen home|details|menu|launching|running|error|empty|empty-library|loading` with optional `--preview-input keyboard|xbox|playstation`. Preview mode does not edit configuration or refresh the library.

## Attribution

Inspired by Aniki's minimal fullscreen presentation. Game media belongs to its respective owners and is read from the user's library.

Launch-variable behavior and tests were adapted from [Playnite](https://github.com/JosefNemec/Playnite), reference commit `e2e6efcda311fea38fcd92f61c8ca90811d97f28`, particularly `GameExtensions.cs`, `GameTests.cs`, and `GenericGameController.cs`. The complete upstream test suite has not been ported.

Input glyphs are [Meritite Union input-prompts](https://github.com/meritite-union/input-prompts), CC0; editable sources and provenance are in `assets/prompts`. GPUI is Apache-2.0 with changes recorded in `vendor/GPUI-PATCH.txt`. SDL is zlib licensed. See [THIRD_PARTY_NOTICES.txt](THIRD_PARTY_NOTICES.txt).
