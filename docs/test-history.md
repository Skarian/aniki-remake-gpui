# Test history

These are records of what was tested, when and on which build. Later sections describe newer builds. For current architecture, see [development](development.md).

All runs used one Windows desktop with its existing Playnite library. Controller tests used virtual Xbox and DualShock 4 controllers (ViGEm). Startup and resource figures are in [benchmarks](benchmarks.md).

## 2026-11-23: Portable setup and v0.1.0

Build `82feb27`, which added separate Playnite program, data and library folders and the self-contained package.

- Release build and 41 unit tests passed. Two opt-in static SDL tests passed: virtual controller input through the production worker, and FFmpeg-decoded audio through SDL's silent driver.
- 24 launch-probe tests and 11 plugin worker cases passed, including separate data folders and protection of the original plugin settings file.
- `tests/library_layout.py` imported a generated database from a custom folder with no Playnite DLLs beside the importer, and left the database hash unchanged. The existing portable library still imported.
- The final executable imports neither `SDL2.dll` nor the VC runtime. The package's only DLL is the plugin host.
- The installed app passed a desktop session: keyboard, virtual Xbox and PS4 controls and prompts, trailers, menu, music, and DAVE THE DIVER launch, focus, close and return to Details in the same process.
- The v0.1.0 CI package passed its hash and allowlist checks, import test, 11 worker cases and H.264, AV1 and media-reader checks. See [releases](releases.md#v010).

Path handling follows Playnite's `PlaynitePaths.cs`, `PlaynitePathsAPI.cs` and `ExtensionFactory.cs` at `e2e6efc`.

## 2026-11-21: Plugin host

First build that launched storefront games through installed Playnite plugins (`16e750d`, fixes in `1f7a16f`). See [plugin host](plugin-host.md).

- 39 Rust tests passed.
- 20 worker and launch-loop cases passed: Started and Stopped timing, automatic process and directory actions, JSON serializer methods, missing install folders, errors, unsupported calls, multiple actions, worker crashes, parent disconnect and cancellation.
- 10 real-DLL cases passed with Epic 2.35 and GOG 2.25 on synthetic games: Epic legacy, v4 and v2 manifests and malformed data; GOG absolute and relative paths, Galaxy settings, and errors for multiple and URL-process actions. A GOG relative action launched a fixture window. No Epic or GOG client or purchased game was launched.
- DAVE THE DIVER launched through installed Steam 2.44 (SDK 6.18) with Playnite closed, reached its menu, and returned to Details on close with the launcher in the foreground. The worker exited, and the Steam plugin settings file was unchanged.
- The Legend of Zelda: Tears of the Kingdom launched through Eden, reached its menu and returned to Details.
- Xbox, Ubisoft and Battle.net were checked against plugin source and DLLs only; this PC has no games for them.

In the Steam run, dispatch to Started took about 2.5 seconds and dispatch to a ready game window about 4.4 seconds, mostly Steam and game startup. The worker used about 93 MiB working set during play and does not run while browsing.

The source audit found a Battle.net classic-controller bug in the upstream plugin, which the host does not patch.

## 2026-11-20: Trailer quality

Build `4227f15` raised trailers from 1080p/30 to source frame rate up to 60 fps and up to 4K, sized by physical display pixels. A 1 MiB output pipe removed the 4K frame-transfer bottleneck.

- 38 unit tests passed.
- Release decoder runs passed at 4K/60, 4K/30, 1440p/60 and 1080p/60, each measured for five seconds after warmup, with shutdown under two seconds.
- Ten replacement trailers decoded samples at start, middle and end. One frame each was inspected.

Ten trailers in the test library were replaced with official publisher uploads (six 4K VP9, four 1080p), and the Octopath Traveler II music file was trimmed of its quiet introduction. Originals were backed up and hash-checked. No Playnite database or launch settings changed.

## 2026-11-10: Retained-window lifecycle

Builds `83de079` (full matrix) and `9144557` (focus-order fix). This pass checked the retained window, Playnite-style process tracking and Steam directory tracking.

| Area | Result |
| --- | --- |
| Steam directory tracking | Two Playnite reference traces replayed to one start and one stop |
| Fixtures | Main 10 cases, timeout and late-ready cases, no-activate, minimized return, manual opt-out and pre-dispatch cancel passed |
| Held input on return | Passed |
| HELLDIVERS 2, DAVE THE DIVER, Zelda | Launch, input, normal exit and return to Details, then B to Home |
| NBA 2K27 | Launch and return passed; menu needed account agreements, which were not accepted |
| Forza Horizon 6 | Launch and return passed; game showed error FHE01, also under Playnite |
| Startup, duplicate start, F11 | A startup focus regression found on `83de079` was fixed in `9144557` and passed its recheck |
| Unit tests | 35 passed |

At the time, release was held for a full rerun of the matrix and screen-recording checks. Later builds continued from `9144557`.

## 2026-11-09: Real games and audio

The first retained-window build was played with real games, using virtual Xbox or DualShock 4 input.

| Game | Result |
| --- | --- |
| NBA 2K27 | Offline exhibition, menu and player movement; return to Details, then Home |
| DAVE THE DIVER | Existing save, movement, return |
| Octopath Traveler II | Existing save, movement, return |
| Untitled Goose Game | Existing save, movement and honk, return |
| Forza Horizon 6 | Free drive with Xbox input, return |
| Cyberpunk 2077 | Existing save, movement with Xbox input, return |
| Tears of the Kingdom (Eden) | Automatic focus, existing save, movement, return |
| Mario Kart 8 Deluxe | Grand Prix race, return |
| Super Smash Bros. Ultimate | Local match, return |
| Pokémon Legends: Arceus | Existing save, movement, return |
| Super Mario 3D World | Super Bell Hill, movement and jump, return |
| HELLDIVERS 2 | Title screens with DS4 and return, after a fix for early session end; stopped at the first-run privacy choice |
| Hades II | One run worked; later input failures also happened with the launcher closed; cause not found |
| God of War Ragnarök | Game failed with a D3D device error, also with the launcher closed |

Music moved from Windows MCI to FFmpeg with an SDL queue. At gain 0.5, the NBA 2K27 launcher track measured -14.5 LUFS against -15.0 and -14.4 LUFS for the game's own menu and intro, from 12-second speaker-loopback samples. Other game samples ranged from -12.1 to -30.3 LUFS.

## Earlier builds

Before 2026-11-09, games ran with the launcher in a separate host process that closed the UI during play and recreated it on return. The retained window replaced that design, so the status screen stays behind the game and nothing covers the screen in black. Measurements from those builds, including in-game memory, do not apply to the current design.
