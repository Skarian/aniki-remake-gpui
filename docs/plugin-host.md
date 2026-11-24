# Plugin host

Storefront games launch through the Playnite library plugin you already have installed. The launcher doesn't reimplement any store. It runs the store's own Playnite DLL.

## How a launch works

1. The library maps the game's `PluginId` to its integration folder and reads the module name from `extension.yaml`. Plugins in the Playnite data folder take precedence over copies in the program folder, as in Playnite.
2. The launcher starts a second copy of its own executable with `--plugin-worker`. That process hosts .NET Framework 4, loads `importer/PlayniteFullscreen.PluginHost.dll`, Playnite SDK 6 and the one library plugin. The GPUI process never loads the CLR, and Playnite itself does not run.
3. The worker asks the plugin for its play controller:
   - **Automatic actions** (File or URL) are returned to Rust as a launch plan. The worker exits, and Rust starts the game and tracks it by process tree or install directory. Relative File paths resolve against the action's working directory.
   - **Custom controllers** run their real `Play` method inside the worker. Their Started and Stopped callbacks define the session. Rust uses the reported PID and install folder only to find the game window. The worker exits after Stopped.
4. When the game ends, the launcher returns to Details.

Supported plugins: Steam, Epic, GOG, Xbox, Ubisoft and Battle.net (the Playnite built-in integrations).

## What the host provides

The host implements the part of the Playnite SDK these plugins use to launch games: paths, application info, language, basic resources and JSON serialization, including the `TryFrom*` methods. It checks the plugin identity and SDK major version, then calls the SDK's internal initialization and Started/Stopped hooks. A future SDK change may need a host update.

Each worker gets a temporary copy of the plugin's `config.json`, so settings migrations never touch your Playnite data. Any other SDK call returns a clear error to the UI.

Not hosted: store login and import, metadata download, plugin UI, scripts, action choosers, other extensions' hooks, database writes and playtime. GOG games with several Play actions, and URL actions that need process tracking, report an error.

## Pending launches

Pressing Back before the game starts stops the worker. It does not close the store client, so a launch the client already received can still start later. If the worker crashes, the UI reports lost tracking and releases the launch cover. The 90-second cover limit applies to plugin launches too; if a store prompt appears, press Back to reveal it.

## Paths

`playnite_root` supplies the SDK assemblies and `{PlayniteDir}`. `playnite_data_root` supplies plugin settings and extra media. `playnite_library_path` selects the library database. See the [README](../README.md#configuration).

## Tests

```powershell
cargo build --example launch_probe --locked
python tests/plugin_worker.py --exe <exe> --probe target/debug/examples/launch_probe.exe
python tests/storefront_actions.py --exe <exe> --plugins <plugin-folders> --probe target/debug/examples/launch_probe.exe
```

`launch_probe` compiles the production launch, plugin and library modules. `plugin_worker.py` uses a fixture plugin (`tests/PluginFixture.cs`) to test the real CLR and SDK boundary: Started and Stopped timing, automatic process and directory actions, errors, unsupported calls, worker crashes, parent disconnect and cancellation. `storefront_actions.py` runs the real Epic and GOG DLLs against synthetic games. Two cases open short-lived fixture windows.

Results from real games are in the [test history](test-history.md#2026-11-21-plugin-host).

## Older cached plans

Before the plugin host, Steam games used a native Rust launch of `steam://rungameid`. Library entries cached by that build keep the native path until the next library refresh replaces them.
