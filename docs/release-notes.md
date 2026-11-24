The first release of Playnite Fullscreen GPUI: a fast, console-style fullscreen launcher for your Playnite library, written in Rust with GPUI.

## Highlights

- Opens in about 0.3 seconds in local benchmarks.
- Covers, backgrounds, logos, descriptions, music, trailers and platform art from your Playnite library, with live library updates.
- Xbox controller, PlayStation controller and keyboard input together, with matching button prompts.
- Trailers at up to 4K and 60 fps.
- A status screen stays behind the running game. When the game closes you return to its Details page.
- Steam, Epic, GOG, Xbox, Ubisoft and Battle.net launch through your installed Playnite integrations. Emulator and executable actions are supported too.
- Works with portable Playnite, or with Playnite's program, data and library in separate folders.
- Works standalone or with Apollo/Moonlight.

## Install

1. Download `playnite-fullscreen-gpui-v0.1.0-windows-x64.zip`.
2. Extract the whole folder.
3. Set your Playnite folder in `playnite-fullscreen-gpui.json`.
4. Run `playnite-fullscreen-gpui.exe`.

Requires Windows x64, a DirectX 11 GPU and Playnite 10. FFmpeg is included. The launcher uses the Playnite and storefront DLLs from your own Playnite installation. Playnite can stay closed. The executable is not code-signed.

See the [README](https://github.com/Skarian/playnite-fullscreen-gpui#setup) for configuration and controls.

## Downloads

- `playnite-fullscreen-gpui-v0.1.0-windows-x64.zip`: the launcher.
- `playnite-fullscreen-gpui-v0.1.0-media-sources.tar.gz`: FFmpeg and dav1d sources and build files for the included media tools.
- `SHA256SUMS.txt`: checksums for both.
