# Releases

The Windows workflow is adapted from [Skarian/desktop-starter](https://github.com/Skarian/desktop-starter) at `504ed64d9c42637df2b8ebcc4d9d651a757a8584`. Its pinned checkout/artifact actions and Windows 2025 runner are retained. This app ships a Windows x64 portable ZIP instead of an installer or macOS package.

- Pull requests and pushes to `main` compile the launcher and run Rust, importer, and managed plugin contract tests.
- Manual dispatch also builds FFmpeg/ffprobe and produces a complete release artifact, without publishing.
- A `v*` tag must match the Cargo version (for example `v0.1.0`). It builds the full package and creates a **draft** GitHub release. Review/download the ZIP before publishing the draft.

## Build inputs

Rust is pinned in `rust-toolchain.toml`; Cargo dependencies are locked. SDL and the native Microsoft C runtime are linked statically. `scripts/prepare-playnite.ps1` downloads checksum-pinned Playnite 10.62 solely for compiler references and fixture tests. Its DLLs never enter the package. Users supply Playnite at runtime.

`scripts/build-media.sh` cross-compiles FFmpeg from revision `5a54fcf75e0245111075b1c31593ba1919c25306` (the same FFmpeg source revision as the previously tested local build), with dav1d 1.5.1 at `42b2b24fb8819f1ed3643aa9cf2a62f03868e3aa` for AV1 decoding. Both source archive hashes are checked before building. Built-in H.264, HEVC, VP9, AAC, MP3 and other decoders remain available; optional external encoders and network protocols are not enabled. Media is read from local Playnite files. This build is LGPL-2.1-or-later plus dav1d's BSD license; it does not enable GPL/nonfree components.

Run the media script on Ubuntu 24.04 with the packages listed at its top. Its output includes the original source archives, configure log, generated config, cross file, build script, and installed compiler-package versions. Those materials are uploaded alongside the Windows ZIP as `*-media-sources.tar.gz`. The package contains notices and source revision details. The compiler image itself is runner-maintained, so this is a pinned-input build, not a claim of bit-identical reproduction across future runner images.

`package.ps1 -Portable` selects only our executables/adapter, prompt assets, cover, docs, clean example config and media tools. `scripts/check-package.ps1` verifies the DLL allowlist, file hashes, config and real probe/video/audio commands on Windows. No game files, library cache or personal settings are shipped.

## v0.1 validation

The installed app at `82feb27` passed a real desktop session: keyboard navigation, virtual Xbox/PS4 controls and prompts, trailer playback, menu/music controls, and DAVE THE DIVER launch/foreground/close/automatic return to Details. It stayed in the same launcher process. Audio evidence was decoding and PCM queueing, not listening. Physical controller, display-transition and all-storefront certification are not claimed.

The release additionally requires the hosted Windows build, managed helper tests, and packaged media checks to pass. Checksums are published in `SHA256SUMS.txt`. Builds are currently unsigned.
