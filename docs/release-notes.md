First public version of Aniki Remake GPUI: a minimal fullscreen Windows launcher for an existing Playnite library.

## Download and setup

Download **aniki-remake-gpui-v0.1.0-windows-x64.zip**, extract the whole folder, and edit `aniki-remake-gpui.json` beside the executable with your Playnite paths. Keep `assets`, `importer`, and `tools` beside it. Playnite can stay closed while you use the launcher.

SDL and the native C runtime are statically linked. FFmpeg/ffprobe are included. **Playnite DLLs, storefront plugins, games and media are not bundled**: use your configured Playnite installation. Requires Windows x64 and a DirectX 11-capable GPU. The executable is unsigned.

## Included

- Keyboard and Xbox/PlayStation navigation and prompts.
- Playnite covers, backgrounds, logos, descriptions, platform collections, music and trailers.
- Separate Details and trailer pages, with a retained game-status screen during play.
- Game launch tracking and automatic return to Details.
- Portable and separate Playnite program/data/library folder layouts.
- Standalone use or Apollo/Moonlight streaming.

## Scope

Steam has real-game launch/return coverage. GOG has an actual-plugin action check. Epic, Xbox, Ubisoft and Battle.net adapters are experimental. Login/import dialogs, library editing, scripts, advanced multi-action selection and playtime writeback are not included. Physical DualSense USB/Bluetooth and all-game compatibility are not certified.

See the [README](https://github.com/Skarian/aniki-remake-gpui#setup) for setup and controls and [release build details](https://github.com/Skarian/aniki-remake-gpui/blob/main/docs/releases.md) for checks and dependencies.

The media-source archive contains matching FFmpeg/dav1d sources and build materials. The Windows ZIP includes the applicable notices. `SHA256SUMS.txt` covers both downloads.
