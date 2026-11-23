# Installed Playnite plugin host

The launcher now calls the installed storefront integration instead of adding a
Rust implementation for each store. It starts another instance of the same exe
with `--plugin-worker`. That process hosts .NET Framework 4 and loads
`importer/Aniki.PluginHost.dll`, Playnite SDK 6 and one library plugin. The GPUI
process never loads the CLR. Playnite's application and database services are
not started.

## Boundaries

- Discovery uses the game's PluginId and the installed integration's
  `extension.yaml` Module. The initial provider list is Steam, Epic, GOG, Xbox,
  Ubisoft and Battle.net. No store DLLs are bundled or downloaded at launch.
- The adapter supplies paths, application information, language, basic resources
  and JSON serialization. It validates the plugin identity and SDK major version.
  It uses the SDK's internal initialization and Started/Stopped hooks, so future
  SDK changes can require an adapter update. Unsupported API calls fail explicitly.
- Each worker reads a private copy of that plugin's `config.json`. Constructor
  migrations and setting writes affect only the temporary copy. Other plugin
  data, account authentication, import and metadata services are not hosted.
- Automatic File/URL actions become native launch plans. Process-tree and
  directory tracking reuse the Rust loop; other tracking modes are rejected.
  Relative File paths resolve against the action's working directory.
  The worker exits before the automatic action is dispatched.
- Custom controllers execute their actual `Play` method. Their callbacks are the
  sole source of lifetime events. Rust uses the reported PID and installation
  directory only to find a ready window; it does not independently end the session.
  A stale cached directory does not cancel a custom controller's tracking.
- Back before Started cancels the worker, never the game or store client. A launch
  already sent to a client can still start later, untracked. A worker crash reports
  lost tracking and releases the cover without claiming the game exited. The UI's
  existing post-Started Back and return behavior remain unchanged.
- The 90-second cover limit also applies while waiting for a plugin to start.
  A store prompt can require Back. There is no new watchdog, focus system, window
  or game-killing policy.

This is a bounded compatibility layer, not a complete Playnite host. Custom
plugin UI, scripts, action choosers, lifecycle hooks from unrelated extensions,
database edits and playtime writeback are outside its scope. Explicit emulator
and executable actions retain the existing path. Legacy cached Steam plans keep
their native fallback until refreshed. This version supports separate
Playnite program/data/library folders through three JSON paths: `playnite_root`,
`playnite_data_root`, and optional `playnite_library_path`. Empty optional paths
preserve the portable defaults. Program assemblies and `{PlayniteDir}` come
from the program root; plugin settings and extra media come from the data root.
User-installed plugins take precedence over program-folder copies. Relative
artwork paths use the selected library's `files` folder. The live exporter also
records its actual configuration and database paths.

## Build and tests

Run `importer/build.ps1 -PlayniteRoot <root>` and the normal Rust build. Package
`importer/Aniki.PluginHost.dll` beside the existing importer files. There is one
product exe, with a managed adapter DLL, rather than a second helper executable.

Playnite SDK, LiteDB, Newtonsoft.Json and storefront DLLs are user-provided and
are not packaged. The importer resolves its assemblies from the supplied program
root at runtime, so it needs no copied Playnite DLLs beside it. `-OutputDirectory`
on the importer build script can stage our own helper files separately.

```powershell
.\build.ps1 -Test
cargo build --example launch_probe --locked
python tests/plugin_worker.py --exe <packaged-exe> --probe target/debug/examples/launch_probe.exe
python tests/storefront_actions.py --exe <packaged-exe> --plugins <downloaded-plugin-folders> --probe target/debug/examples/launch_probe.exe
```

The probe compiles the production launch, plugin and library modules. The fixture
DLL tests the real CLR/SDK event boundary; it does not emulate a real store.
The two automatic-action cases open short-lived fixture windows. Keep the
desktop free while running them. All fixture data stays under `artifacts`.

## Evidence from 2026-11-21

- 39 Rust tests passed; the pre-existing real-video test remains opt-in.
- 16 worker/launch-loop cases passed: asynchronous Started/Stopped, immediate
  Stopped, automatic process and directory actions, dispatch exception,
  unsupported API, multiple actions, worker crash, parent disconnect and pending
  cancellation. Rust crash handling produced Failed, not Ended.
- Installed Steam 2.44 with SDK 6.18 launched DAVE THE DIVER from the fullscreen
  candidate. The game reached its menu. Closing it returned to Details in the
  same launcher, with foreground true and topmost false. The worker exited.
  Playnite was closed throughout. Original Steam settings retained SHA-256
  `27B19E05C2EE944D074ACCDBB90AF7ADB265362C80EC0DD5043B940E0770981F`.
- Zelda launched through Eden, reached its menu, and returned to Details with
  the same foreground/topmost result. No emulator controls were edited.
- The official GOG 2.25 DLL resolved a synthetic installed-game manifest into an
  automatic File/Process plan using the production managed adapter. This is not
  a real GOG purchase/account/gameplay test.
- Xbox, Ubisoft and Battle.net have no real-game launch evidence on this PC.
  Source and downloaded-DLL checks do not certify those launches.

The first directory fixture placed the probe executable beneath the fake install
directory. The tracker correctly kept that directory active. Moving the runner
outside the fake game directory fixed the test; product tracking was unchanged.

Steam dispatch-to-Started was about 2.50 seconds and dispatch-to-ready-window about
4.35 seconds in the observed run. These include Steam and game startup, not just
adapter overhead. The worker working set was about 93 MiB during that game and
private commit about 78 MiB. It is absent while browsing and after return.
Working sets include shared pages and must not be treated as unique physical RAM.
This is a single runtime sample, not a benchmark guarantee.

The UI, focus, controller and media implementations were not changed. The tests
do not establish every storefront's behavior, physical controller compatibility,
or frame-by-frame absence of desktop exposure. The candidate is ready for normal
user testing; these are representative checks, not an all-games certification.

## Follow-up fixes and checks, 2026-11-21

Three repairs follow the plugin source audit:

- Supply the SDK's JSON string, file and stream Try methods, including out values
  and errors. These methods follow Playnite's success, failure and stream ownership
  rules. Epic uses all three forms across its manifest formats.
- Resolve an automatic File action's relative executable against its working
  directory. GOG emits this form for nested game directories.
- Treat an unavailable custom-controller install directory as a missing window
  hint. Keep its Started/Stopped callbacks authoritative. Automatic Directory
  actions still reject unavailable directories because they need them for tracking.

Validation on the revised candidate:

- 39 Rust tests passed, with one existing optional video test ignored.
- 20 worker and launch-loop cases passed. These include the serializer contracts,
  missing/file-valued custom directories, strict automatic directory validation,
  pending cancellation, parent disconnect and worker crashes.
- 10 actual-DLL cases passed with Epic 2.35 and GOG 2.25. Synthetic data covered
  Epic legacy, v4 and v2 manifests and malformed data. GOG covered absolute and
  relative paths, Galaxy settings and explicit errors for multiple/URL-process
  actions. The GOG relative action launched a short-lived fixture through Rust.
  No Epic/GOG store client or purchased game was launched.
- DAVE THE DIVER reached its menu through installed Steam 2.44 with Playnite closed.
  Closing the game returned to Details in the same launcher. The log recorded
  `foreground=true;topmost=false`, and the worker exited. Steam settings retained
  the hash recorded above. No UI, focus or emulator-control code changed.

Local machine results are in `artifacts/plugin-worker-tests/results.json`,
`artifacts/storefront-action-tests/results.json` and
`artifacts/plugin-host-fixes/steam-session.json`. DLL and executable hashes are
included in the test reports. The package source list remains in the root
checkout's `artifacts/plugin-host-research/packages.json`.

Known limits remain explicit: GOG multi-action selection and URL actions with
process tracking are unsupported. The audited Battle.net classic-controller
null-event bug belongs to the upstream plugin and is not patched by this host.
These tests do not establish all-storefront gameplay support.

## Size

Runtime source is 5722 physical lines across `src/*.rs` and
`importer/*.cs`, versus 5250 at the checkpoint: net +472. The three follow-up
repairs add 36 lines over the first plugin-host build. This excludes
Rust test blocks and the test-only probe/fixture files; it includes comments and
blank lines. The production executable is about 14.8 MiB and the adapter DLL
21 KiB. Existing media, SDK and plugin files are not bundled again.
