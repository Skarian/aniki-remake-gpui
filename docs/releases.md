# Releases

Releases are built by `.github/workflows/build.yml` on a Windows 2025 runner. The workflow is adapted from [Skarian/desktop-starter](https://github.com/Skarian/desktop-starter) at `504ed64`.

| Trigger | Result |
| --- | --- |
| Pull request or push to `main` | Build, Rust tests, split-library import test, plugin host contract test |
| Manual run | All of the above, plus FFmpeg and a complete package (not published) |
| Tag `v*` matching the Cargo version | Full package and a **draft** GitHub release using [release-notes.md](release-notes.md) |

Download and check the draft's ZIP before publishing it.

## What goes into a release

- **Launcher.** Rust toolchain pinned in `rust-toolchain.toml`, dependencies locked in `Cargo.lock`. SDL 2.32.10 and the Microsoft C runtime are linked statically.
- **Helpers.** `PlayniteFullscreen.Import.exe` and `PlayniteFullscreen.PluginHost.dll`, compiled against Playnite 10.62. `scripts/prepare-playnite.ps1` downloads that Playnite version, checking its hash, for compilation and tests only. No Playnite DLL is packaged.
- **Media tools.** `scripts/build-media.sh` cross-compiles FFmpeg and ffprobe on Ubuntu 24.04 from FFmpeg `5a54fcf`, with dav1d 1.5.1 (`42b2b24`) for AV1. Source archive hashes are checked before building. The build is LGPL-2.1-or-later plus dav1d's BSD license, with no GPL or nonfree parts and no network protocols. FFmpeg's built-in decoders cover H.264, HEVC, VP9, AAC, MP3 and more.
- **Package.** `package.ps1 -Portable` copies an allowlist: the launcher, helpers, prompts, Apollo cover, docs, a clean config, FFmpeg and license notices. `scripts/check-package.ps1` checks the DLL allowlist, file hashes and config, and runs ffprobe, video and audio decodes.

Release assets:

- `playnite-fullscreen-gpui-vX.Y.Z-windows-x64.zip`
- `playnite-fullscreen-gpui-vX.Y.Z-media-sources.tar.gz`: FFmpeg and dav1d source archives, configure log, generated config, cross file, build script and compiler package versions
- `SHA256SUMS.txt`

The build inputs are pinned. The compiler images are maintained by the runner, so later builds may not be bit-identical. Builds are not code-signed.

## v0.1.0

v0.1.0 was first published on 2026-11-23 under the earlier name *aniki-remake-gpui*, from CI run [45290035474](https://github.com/Skarian/playnite-fullscreen-gpui/actions/runs/45290035474). For that build, the downloaded ZIP passed the hash and allowlist checks, the import test, 11 plugin worker cases, and H.264, AV1 and media-reader checks. The installed app at `82feb27` passed a desktop session with keyboard, virtual Xbox and PS4 controllers, trailers, menu and music, and a DAVE THE DIVER launch and return to Details in the same process.

The renamed release uses playnite-fullscreen-gpui for the repository, executable, configuration, ZIP folder and downloads. Build and package verification are recorded in [GitHub Actions](https://github.com/Skarian/playnite-fullscreen-gpui/actions/workflows/build.yml).
