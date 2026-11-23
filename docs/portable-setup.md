# Portable setup verification — 2026-11-23

The setup changes keep manual JSON configuration and defer GitHub build automation.
SDL 2.32.10 (sdl2-sys 0.39.0) and the native Microsoft C runtime are statically linked.
The portable packager supplies FFmpeg/ffprobe privately; an empty `ffmpeg` config
uses the packaged tools. It does not save a machine-specific resolved path.
Playnite assemblies and storefront DLLs remain user-provided. Only our importer
and adapter are shipped. No user's cache, settings, library or game media is packaged.

`playnite_root` selects program files; `playnite_data_root` selects settings,
extensions and extra media; `playnite_library_path` optionally selects a relocated
library. Empty optional values preserve the existing portable layout. Live exports
record the actual API paths. Rebuild/reinstall the optional exporter when upgrading.
Refresh fingerprints include the configured roots so an old cached plan is rebuilt.

Source reference: https://github.com/JosefNemec/Playnite at
`e2e6efcda311fea38fcd92f61c8ca90811d97f28`, read as-is from
`C:/Users/Neil/.skills/reference-module/JosefNemec/Playnite`.
`Settings/PlaynitePaths.cs`, `API/PlaynitePathsAPI.cs` and
`Plugins/ExtensionFactory.cs` establish program/config separation, `{PlayniteDir}`
semantics and user-extension precedence. Plugin settings still use isolated copies.

Verified locally:

- Release build and 41 application unit tests passed. Two normally ignored SDL
  tests also passed: virtual controller input through the production worker, and
  FFmpeg-decoded PCM through SDL's silent dummy audio driver. The optional real-video
  test was not run in this pass.
- 24 launch-probe unit tests passed (these overlap application modules).
- 11 managed worker checks passed, including separate user-data settings and
  protection of the original settings file.
- `tests/library_layout.py` generated a fixture database in a custom directory,
  imported it with no adjacent Playnite DLLs, and checked normalization through
  the packaged launcher. The database hash was unchanged. The existing portable
  library also imported through the legacy two-argument invocation.
- PE dependency inspection found neither SDL2.dll nor VC runtime imports in the
  final native executable. The package's only DLL is Aniki.PluginHost.dll.
- Packaged FFmpeg and ffprobe both execute. Audio checks use the packaged FFmpeg.

No real game, physical-controller session, display setting or installed launcher
was changed by these checks. This is not a fresh-Windows-machine certification.
The local media tools came from the already tested FFmpeg build
`N-127043-g5a54fcf75e-20260930`; their license and build configuration accompany
the package. Public release automation, including corresponding third-party
source distribution, remains deferred.
