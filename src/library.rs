use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::os::windows::process::CommandExt;
use std::{
    collections::HashMap,
    fs,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::Sender,
    },
    time::Duration,
};

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
#[serde(default)]
pub struct LaunchPlan {
    pub kind: String,
    pub executable: String,
    pub arguments: String,
    pub working_dir: String,
    pub install_dir: String,
    pub steam_id: String,
    pub error: String,
}
#[derive(Clone, Serialize, Deserialize, Debug, Default)]
#[serde(default)]
pub struct Game {
    pub id: String,
    pub name: String,
    pub platform: String,
    pub genres: String,
    pub developer: String,
    pub description: String,
    pub year: String,
    pub playtime: u64,
    pub last_activity: String,
    pub cover: String,
    pub background: String,
    pub logo: String,
    pub video: String,
    pub music: String,
    pub favorite: bool,
    pub launch: LaunchPlan,
}
#[derive(Clone, Serialize, Deserialize, Debug, Default)]
pub struct Library {
    pub games: Vec<Game>,
    pub exported_utc: String,
    #[serde(default)]
    pub platform_art: HashMap<String, String>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub playnite_root: String,
    pub fullscreen: bool,
    pub music: bool,
    pub video: bool,
    pub music_volume: f32,
    pub video_delay_ms: u64,
    pub ffmpeg: String,
    /// Decoder, filter and output worker count. Zero restores FFmpeg auto sizing.
    pub video_threads: u32,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            playnite_root: r"D:\Playnite".into(),
            fullscreen: true,
            music: true,
            video: true,
            music_volume: 0.25,
            video_delay_ms: 1500,
            ffmpeg: String::new(),
            video_threads: 2,
        }
    }
}
pub fn text(v: &Value) -> String {
    v.as_str()
        .or_else(|| v.get("$guid").and_then(Value::as_str))
        .or_else(|| v.get("$date").and_then(Value::as_str))
        .unwrap_or_default()
        .to_owned()
}
pub fn id(v: &Value) -> String {
    let x = if v.get("_id").is_some() {
        &v["_id"]
    } else {
        &v["Id"]
    };
    text(x)
}
pub fn list(v: &Value) -> &[Value] {
    v.as_array().map(Vec::as_slice).unwrap_or(&[])
}
pub fn flag(v: &Value) -> bool {
    v.as_bool().unwrap_or(false)
}
pub fn expand(
    input: &str,
    game: &Value,
    root: &str,
    emulator: &str,
    rom_override: Option<&str>,
    platform: &str,
) -> String {
    // Derived from Playnite GameExtensions.StringExpand/ExpandGame, MIT; see THIRD_PARTY_NOTICES.
    let basic = |s: &str| {
        s.replace("{PlayniteDir}", root)
            .replace("{EmulatorDir}", emulator)
    };
    let install = basic(&text(&game["InstallDirectory"]));
    let rom = basic(rom_override.unwrap_or_else(|| game["Roms"][0]["Path"].as_str().unwrap_or("")))
        .replace("{InstallDir}", &install);
    let file = rom.rsplit(['\\', '/']).next().unwrap_or("");
    let stem = file.rsplit_once('.').map(|(a, _)| a).unwrap_or(file);
    let dir = rom.rsplit_once(['\\', '/']).map(|(a, _)| a).unwrap_or("");
    let mut out = input.to_owned();
    if !install.is_empty() {
        out = out.replace("{InstallDir}", &install).replace(
            "{InstallDirName}",
            install
                .trim_end_matches(['\\', '/'])
                .rsplit(['\\', '/'])
                .next()
                .unwrap_or(""),
        );
    }
    if !rom.is_empty() {
        out = out
            .replace("{ImagePath}", &rom)
            .replace("{ImageNameNoExt}", stem)
            .replace("{ImageName}", file)
            .replace("{ImageDir}", dir);
    }
    for (key, value) in [
        ("PlayniteDir", root.to_owned()),
        ("Name", text(&game["Name"])),
        ("PluginId", text(&game["PluginId"])),
        ("GameId", text(&game["GameId"])),
        ("DatabaseId", id(game)),
        ("Version", text(&game["Version"])),
        ("EmulatorDir", emulator.to_owned()),
    ] {
        out = out.replace(&format!("{{{key}}}"), &value);
    }
    if !platform.is_empty() {
        out = out.replace("{Platform}", platform);
    }
    out
}
fn has_scripts(v: &Value) -> bool {
    [
        "PreScript",
        "PostScript",
        "StartupScript",
        "ExitScript",
        "GameStartedScript",
        "GameStartingScript",
        "GameStoppedScript",
    ]
    .iter()
    .any(|k| !text(&v[*k]).trim().is_empty())
}
fn emulator_executable(dir: &Path, pattern: &str, depth: u8) -> Option<String> {
    let re = regex::RegexBuilder::new(pattern)
        .case_insensitive(true)
        .build()
        .ok()?;
    let mut entries: Vec<_> = fs::read_dir(dir).ok()?.filter_map(|e| e.ok()).collect();
    entries.sort_by_key(|e| e.file_name());
    for e in &entries {
        if e.path().is_file() && re.is_match(&e.file_name().to_string_lossy()) {
            return Some(e.path().to_string_lossy().into());
        }
    }
    if depth > 0 {
        for e in entries {
            if e.path().is_dir() {
                if let Some(p) = emulator_executable(&e.path(), pattern, depth - 1) {
                    return Some(p);
                }
            }
        }
    }
    None
}
pub fn launch_plan(
    g: &Value,
    emulators: &[Value],
    root: &str,
    platform: &str,
) -> Result<LaunchPlan> {
    let ex = |s: &str| expand(s, g, root, "", None, platform);
    let install = ex(&text(&g["InstallDirectory"]));
    if has_scripts(g) {
        bail!(
            "This game has Playnite scripts. Script execution is not yet supported by aniki-remake-gpui."
        )
    }
    let actions: Vec<_> = list(&g["GameActions"])
        .iter()
        .filter(|a| flag(&a["IsPlayAction"]))
        .collect();
    if actions.len() > 1 {
        bail!("This game has multiple play actions; select a single action in Playnite for now.")
    }
    if let Some(a) = actions.first() {
        let mode = text(&a["TrackingMode"]);
        if !mode.is_empty() && mode != "Default" && mode != "Process" {
            bail!("Tracking mode {mode} requires a dedicated adapter.")
        }
        match text(&a["Type"]).as_str() {
            "File" => {
                let path = ex(&text(&a["Path"]));
                if !path.to_lowercase().ends_with(".exe") {
                    bail!("Only executable file actions are supported.")
                }
                let working = ex(&text(&a["WorkingDir"]));
                return Ok(LaunchPlan {
                    kind: "process".into(),
                    working_dir: if working.is_empty() {
                        Path::new(&path)
                            .parent()
                            .unwrap_or(Path::new("."))
                            .to_string_lossy()
                            .into()
                    } else {
                        working
                    },
                    executable: path,
                    arguments: ex(&text(&a["Arguments"])),
                    install_dir: install,
                    ..Default::default()
                });
            }
            "Emulator" => {
                if list(&g["Roms"]).len() != 1 {
                    bail!("Choose a single ROM for this game's play action.")
                }
                let em = emulators
                    .iter()
                    .find(|e| id(e) == text(&a["EmulatorId"]))
                    .context("Emulator configuration is missing")?;
                let emdir = ex(&text(&em["InstallDir"]));
                let expand_emu = |s: &str| expand(s, g, root, &emdir, None, platform);
                let rom = expand_emu(&text(&g["Roms"][0]["Path"]));
                if !Path::new(&rom).is_file() {
                    bail!("ROM not found: {rom}")
                }
                let pid = text(&a["EmulatorProfileId"]);
                let (path, mut args, work) = if let Some(p) =
                    list(&em["CustomProfiles"]).iter().find(|p| id(p) == pid)
                {
                    if has_scripts(p) {
                        bail!("Emulator profile uses scripts; unsupported.")
                    }
                    let tracking = text(&p["TrackingMode"]);
                    if !tracking.is_empty() && tracking != "Default" && tracking != "Process" {
                        bail!("Custom emulator tracking mode {tracking} needs a dedicated adapter.")
                    }
                    (
                        expand_emu(&text(&p["Executable"])),
                        text(&p["Arguments"]),
                        expand_emu(&text(&p["WorkingDirectory"])),
                    )
                } else {
                    let p = list(&em["BuiltinProfiles"])
                        .iter()
                        .find(|p| id(p) == pid)
                        .context("Built-in emulator profile is missing")?;
                    if has_scripts(p) {
                        bail!("Emulator profile uses scripts; unsupported.")
                    }
                    let config_id = text(&em["BuiltInConfigId"]);
                    let name = text(&p["BuiltInProfileName"]);
                    let base = Path::new(root).join("Emulation/Emulators");
                    let mut definition = None;
                    for entry in fs::read_dir(base)?.filter_map(|x| x.ok()) {
                        if let Ok(yaml) = fs::read_to_string(entry.path().join("emulator.yaml")) {
                            if let Ok(v) = serde_yaml::from_str::<Value>(&yaml) {
                                if text(&v["Id"]) == config_id {
                                    definition = Some(v);
                                    break;
                                }
                            }
                        }
                    }
                    let definition = definition.context("Emulator definition not found")?;
                    let profile = list(&definition["Profiles"])
                        .iter()
                        .find(|p| text(&p["Name"]) == name)
                        .context("Emulator profile definition not found")?;
                    if flag(&profile["ScriptStartup"]) {
                        bail!("Emulator needs a startup script; unsupported.")
                    }
                    let path = emulator_executable(
                        Path::new(&emdir),
                        &text(&profile["StartupExecutable"]),
                        3,
                    )
                    .context("Emulator executable not found")?;
                    let work = Path::new(&path).parent().unwrap().to_string_lossy().into();
                    let args = if flag(&p["OverrideDefaultArgs"]) {
                        text(&p["CustomArguments"])
                    } else {
                        text(&profile["StartupArguments"])
                    };
                    (path, args, work)
                };
                if flag(&a["OverrideDefaultArgs"]) {
                    args = text(&a["Arguments"])
                } else if !text(&a["AdditionalArguments"]).is_empty() {
                    args.push(' ');
                    args.push_str(&text(&a["AdditionalArguments"]));
                }
                return Ok(LaunchPlan {
                    kind: "process".into(),
                    executable: path,
                    arguments: expand_emu(&args),
                    working_dir: work,
                    install_dir: emdir,
                    ..Default::default()
                });
            }
            other => bail!("Unsupported play action: {other}"),
        }
    }
    if text(&g["PluginId"]) == "cb91dfc9-b977-43bf-8e70-55f46e410fab" {
        let steam_id = text(&g["GameId"]);
        if steam_id.is_empty() || !steam_id.bytes().all(|b| b.is_ascii_digit()) {
            bail!("Invalid Steam app ID")
        }
        return Ok(LaunchPlan {
            kind: "steam".into(),
            steam_id,
            install_dir: install,
            ..Default::default()
        });
    }
    bail!("No supported launch action. This library integration needs an adapter.")
}
fn asset_path(root: &Path, value: &Value) -> String {
    let p = text(value);
    if p.is_empty() {
        return p;
    }
    if Path::new(&p).is_absolute() {
        p
    } else {
        root.join("library/files").join(p).to_string_lossy().into()
    }
}
fn first_media(dir: &Path, exts: &[&str]) -> String {
    let mut paths: Vec<_> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .is_some_and(|e| exts.contains(&e.to_string_lossy().to_lowercase().as_str()))
        })
        .collect();
    paths.sort();
    paths
        .first()
        .map(|p| p.to_string_lossy().into())
        .unwrap_or_default()
}
fn present(path: PathBuf) -> String {
    if path.is_file() {
        path.to_string_lossy().into()
    } else {
        String::new()
    }
}
pub fn normalize(raw: &Value) -> Library {
    let root = text(&raw["root"]);
    let rootpath = Path::new(&root);
    let c = &raw["collections"];
    let lookup = |table: &str| -> HashMap<String, String> {
        list(&c[table])
            .iter()
            .map(|p| (id(p), text(&p["Name"])))
            .collect()
    };
    let platforms = lookup("platforms");
    let genres = lookup("genres");
    let companies = lookup("companies");
    let names = |value: &Value, map: &HashMap<String, String>| {
        list(value)
            .iter()
            .filter_map(|p| map.get(&text(p)).cloned())
            .collect::<Vec<_>>()
            .join(" · ")
    };
    let strip = regex::Regex::new("<[^>]*>").unwrap();
    let mut games = Vec::new();
    for g in list(&c["games"]) {
        if !flag(&g["IsInstalled"]) || flag(&g["Hidden"]) {
            continue;
        }
        let gid = id(g);
        let platform = names(&g["PlatformIds"], &platforms);
        let extra = rootpath.join("ExtraMetadata/games").join(&gid);
        let release = if g["ReleaseDate"].is_object() {
            g["ReleaseDate"]["Year"]
                .as_i64()
                .map(|n| n.to_string())
                .unwrap_or_else(|| {
                    text(&g["ReleaseDate"]["ReleaseDate"])
                        .chars()
                        .take(4)
                        .collect()
                })
        } else {
            text(&g["ReleaseDate"]).chars().take(4).collect()
        };
        let description = strip
            .replace_all(&text(&g["Description"]), " ")
            .replace("&nbsp;", " ")
            .replace("&amp;", "&");
        games.push(Game {
            id: gid.clone(),
            name: text(&g["Name"]),
            platform: platform.clone(),
            genres: names(&g["GenreIds"], &genres),
            developer: names(&g["DeveloperIds"], &companies),
            description: description.split_whitespace().collect::<Vec<_>>().join(" "),
            year: release,
            playtime: g["Playtime"]
                .as_u64()
                .or_else(|| text(&g["Playtime"]["$numberLong"]).parse().ok())
                .unwrap_or(0),
            last_activity: text(&g["LastActivity"]),
            cover: asset_path(rootpath, &g["CoverImage"]),
            background: asset_path(rootpath, &g["BackgroundImage"]),
            logo: present(extra.join("Logo.png")),
            video: present(extra.join("VideoTrailer.mp4")),
            music: first_media(
                &rootpath.join("ExtraMetadata/UniPlaySong/Games").join(&gid),
                &["mp3", "wav", "wma"],
            ),
            favorite: flag(&g["Favorite"]),
            launch: launch_plan(g, list(&c["emulators"]), &root, &platform).unwrap_or_else(|e| {
                LaunchPlan {
                    error: e.to_string(),
                    ..Default::default()
                }
            }),
        });
    }
    games.sort_by(|a, b| {
        b.last_activity
            .cmp(&a.last_activity)
            .then(a.name.cmp(&b.name))
    });
    Library {
        games,
        exported_utc: text(&raw["exported_utc"]),
        platform_art: platform_art(rootpath, list(&c["platforms"])),
    }
}
// Same precedence as Aniki's FilterPresetSelector: user folder, theme, then
// Playnite's platform icon. Resolve in the refresh worker, never in render.
fn platform_art(root: &Path, platforms: &[Value]) -> HashMap<String, String> {
    let read_json = |path: PathBuf| -> Value {
        fs::read(path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    };
    let helper =
        read_json(root.join("ExtensionsData/96a983a3-3f13-4dce-a474-4052b718bb52/config.json"));
    let custom = text(&helper["CustomFilterIconsFolder"]);
    let settings = read_json(root.join("fullscreenConfig.json"));
    let theme = root
        .join("Themes/Fullscreen")
        .join(text(&settings["Theme"]))
        .join("Icons/Filter");
    let mut result = HashMap::new();
    let entries = platforms
        .iter()
        .map(|p| (text(&p["Name"]), asset_path(root, &p["Icon"])))
        .chain([
            ("All games".into(), String::new()),
            ("Favorites".into(), String::new()),
        ]);
    for (name, fallback) in entries {
        let filename = if name == "All games" {
            "All Games"
        } else {
            &name
        };
        let user = if custom.is_empty() {
            String::new()
        } else {
            present(Path::new(&custom).join(format!("{filename}.png")))
        };
        let themed = present(theme.join(format!("{filename}.png")));
        let path = if !user.is_empty() {
            user
        } else if !themed.is_empty() {
            themed
        } else {
            fallback
        };
        if !path.is_empty() {
            result.insert(name, path);
        }
    }
    result
}
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    use windows::{
        Win32::Storage::FileSystem::{
            MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
        },
        core::PCWSTR,
    };
    let temp = path.with_extension("tmp");
    fs::write(&temp, bytes)?;
    let wide = |p: &Path| {
        p.as_os_str()
            .to_string_lossy()
            .encode_utf16()
            .chain(Some(0))
            .collect::<Vec<_>>()
    };
    let a = wide(&temp);
    let b = wide(path);
    unsafe {
        MoveFileExW(
            PCWSTR(a.as_ptr()),
            PCWSTR(b.as_ptr()),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )?
    }
    Ok(())
}
fn stamp(path: &Path) -> String {
    fs::metadata(path)
        .map(|m| format!("{}:{:?}", m.len(), m.modified()))
        .unwrap_or_default()
}
pub fn fingerprint(root: &Path) -> String {
    let mut v = Vec::new();
    if let Ok(entries) = fs::read_dir(root.join("library")) {
        for e in entries.flatten() {
            if e.path().extension().is_some_and(|x| x == "db") {
                v.push(format!("{}:{}", e.path().display(), stamp(&e.path())));
            }
        }
    }
    v.sort();
    v.join("|")
}
fn media_fingerprint(root: &Path) -> String {
    // Metadata queries only: never hash or copy movie/music contents during refresh.
    fn visit(dir: &Path, depth: u8, out: &mut Vec<String>) {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() && depth > 0 {
                    visit(&p, depth - 1, out);
                } else if p.is_file() {
                    out.push(format!("{}:{}", p.display(), stamp(&p)));
                }
            }
        }
    }
    let mut entries = Vec::new();
    for dir in [
        "ExtraMetadata/games",
        "ExtraMetadata/UniPlaySong/Games",
        "library/files",
    ] {
        visit(&root.join(dir), 1, &mut entries);
    }
    entries.sort();
    entries.join("|")
}
// Fit the visible mark, rather than transparent padding, into the fixed UI slot.
// This happens once in the background artwork cache; Playnite originals stay intact.
fn trim_logo(image: image::DynamicImage) -> image::DynamicImage {
    let rgba = image.to_rgba8();
    let (mut left, mut top) = rgba.dimensions();
    let (mut right, mut bottom) = (0, 0);
    for (x, y, pixel) in rgba.enumerate_pixels() {
        if pixel[3] > 0 {
            left = left.min(x);
            top = top.min(y);
            right = right.max(x + 1);
            bottom = bottom.max(y + 1);
        }
    }
    if right <= left || bottom <= top {
        return image;
    }
    image.crop_imm(left, top, right - left, bottom - top)
}

fn thumbnail(src: &str, cache: &Path, width: u32) -> String {
    if src.is_empty() || !Path::new(src).is_file() {
        return src.into();
    }
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    src.hash(&mut hash);
    stamp(Path::new(src)).hash(&mut hash);
    width.hash(&mut hash);
    "artwork-lanczos-quality92-v2".hash(&mut hash);
    if width == 900 {
        "logo-visible-bounds-v1".hash(&mut hash);
    }
    let target = cache.join(format!(
        "{:x}.{}",
        hash.finish(),
        if width == 900 { "png" } else { "jpg" }
    ));
    if !target.exists() {
        if let Ok(image) = image::open(src) {
            let image = if width == 900 {
                trim_logo(image)
            } else {
                image
            };
            let image = if image.width() > width || image.height() > width {
                image.resize(width, width, image::imageops::FilterType::Lanczos3)
            } else {
                image
            };
            let saved = if width == 900 {
                image.save(&target)
            } else {
                fs::File::create(&target)
                    .map_err(image::ImageError::IoError)
                    .and_then(|file| {
                        image::codecs::jpeg::JpegEncoder::new_with_quality(file, 92)
                            .encode_image(&image.to_rgb8())
                    })
            };
            if saved.is_err() {
                return src.into();
            }
        } else {
            return src.into();
        }
    }
    target.to_string_lossy().into()
}
pub fn prepare_assets(lib: &mut Library, cache: &Path) {
    let _ = fs::create_dir_all(cache);
    for game in &mut lib.games {
        game.cover = thumbnail(&game.cover, cache, 400);
        game.background = thumbnail(&game.background, cache, 2560);
        game.logo = thumbnail(&game.logo, cache, 900);
    }
}
#[derive(Clone)]
pub enum Update {
    Library(Library),
    Status(String),
}
pub fn start_refresh(base: PathBuf, config: Config, tx: Sender<Update>, paused: Arc<AtomicBool>) {
    std::thread::spawn(move || {
        let raw = base.join("cache/raw.json");
        let live = base.join("cache/live.json");
        let mut last_db = fs::read_to_string(base.join("cache/database.stamp")).unwrap_or_default();
        let mut last_live = String::new();
        let mut last_raw = String::new();
        let mut last_media = String::new();
        let mut preferred_source = raw.clone();
        let mut retry = 0;
        loop {
            if paused.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_secs(2));
                continue;
            }
            let db = fingerprint(Path::new(&config.playnite_root));
            let live_stamp = stamp(&live);
            let mut source = preferred_source.clone();
            let mut changed = false;
            if !live_stamp.is_empty() && live_stamp != last_live {
                source = live.clone();
                preferred_source = live.clone();
                last_live = live_stamp;
                changed = true;
            } else if db != last_db || retry > 0 {
                if retry > 0 {
                    retry -= 1;
                } else {
                    let result = Command::new(base.join("importer/Aniki.Import.exe"))
                        .arg(&config.playnite_root)
                        .arg(&raw)
                        .creation_flags(0x08000000)
                        .output();
                    match result {
                        Ok(o) if o.status.success() => {
                            source = raw.clone();
                            preferred_source = raw.clone();
                            let _ = fs::write(base.join("cache/database.stamp"), &db);
                            last_db = db;
                            changed = true;
                        }
                        _ => {
                            retry = 4;
                            let _ = tx.send(Update::Status(
                                "Using saved library · waiting for Playnite export".into(),
                            ));
                        }
                    }
                }
            }
            let rs = stamp(&raw);
            if rs != last_raw {
                last_raw = rs;
                changed = true;
            }
            let media = media_fingerprint(Path::new(&config.playnite_root));
            if media != last_media {
                last_media = media;
                changed = true;
            }
            if changed {
                let result = (|| -> Result<Library> {
                    let data = fs::read(&source)?;
                    let raw: Value = serde_json::from_slice(&data)?;
                    if raw["schema"] != 1 {
                        bail!("Unsupported snapshot version")
                    };
                    let mut lib = normalize(&raw);
                    prepare_assets(&mut lib, &base.join("cache/artwork"));
                    atomic_write(&base.join("cache/library.json"), &serde_json::to_vec(&lib)?)?;
                    Ok(lib)
                })();
                match result {
                    Ok(lib) => {
                        let _ = tx.send(Update::Library(lib));
                    }
                    Err(e) => {
                        let _ = tx.send(Update::Status(format!("Refresh paused: {e}")));
                    }
                }
            }
            std::thread::sleep(Duration::from_secs(2));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    // Translated from Playnite source/Tests/Playnite.Tests/Models/GameTests.cs (MIT).
    #[test]
    fn playnite_expand_variables() {
        let g = json!({"Name":"test game","InstallDirectory":"c:\\test\\test2\\","Roms":[{"Path":"c:\\test\\test2\\test.iso"}],"Id":"db-id","PluginId":"plugin-id","GameId":"game_id","Version":"1.0"});
        for (input, expected) in [
            ("", ""),
            ("teststring", "teststring"),
            ("{InstallDir}teststring", "c:\\test\\test2\\teststring"),
            ("{ImageNameNoExt}", "test"),
            ("{ImageName}", "test.iso"),
            ("{PlayniteDir}", "D:\\Playnite"),
            ("{Name}", "test game"),
            ("{InstallDirName}", "test2"),
            ("{Platform}", "PC"),
            ("{PluginId}", "plugin-id"),
            ("{GameId}", "game_id"),
            ("{DatabaseId}", "db-id"),
            ("{Version}", "1.0"),
            ("{ImagePath}", "c:\\test\\test2\\test.iso"),
        ] {
            assert_eq!(
                expand(input, &g, r"D:\Playnite", "", None, "PC"),
                expected,
                "{}",
                input
            )
        }
    }
    #[test]
    fn playnite_nested_variables() {
        let g = json!({"InstallDirectory":"{PlayniteDir}\\test\\test2","Roms":[{"Path":"{InstallDir}\\test.iso"}]});
        assert_eq!(
            expand("{ImagePath}", &g, r"D:\Playnite", "", None, ""),
            r"D:\Playnite\test\test2\test.iso"
        );
    }
    #[test]
    fn playnite_empty_game() {
        assert_eq!(expand("", &json!({}), "", "", None, ""), "");
    }
    #[test]
    fn explicit_rom_overrides_first() {
        let g = json!({"Roms":[{"Path":"old.iso"}]});
        assert_eq!(
            expand("{ImageName}", &g, "", "", Some("other.iso"), ""),
            "other.iso"
        );
    }
    #[test]
    fn rejects_unknown_integration() {
        assert!(launch_plan(&json!({"PluginId":"unknown"}), &[], "", "").is_err());
    }
    #[test]
    fn rejects_scripts_instead_of_silent_skip() {
        assert!(launch_plan(&json!({"GameStartingScript":"do stuff"}), &[], "", "").is_err());
    }
    #[test]
    fn validates_steam_id() {
        assert!(
            launch_plan(
                &json!({"PluginId":"cb91dfc9-b977-43bf-8e70-55f46e410fab","GameId":"1 & bad"}),
                &[],
                "",
                ""
            )
            .is_err()
        );
    }
    #[test]
    fn custom_emulator_arguments_follow_playnite_precedence() {
        // GenericGameController.StartEmulator: action override wins; otherwise append additional args.
        let dir = std::env::temp_dir().join(format!("lumen-args-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let rom = dir.join("game with spaces.nsp");
        fs::write(&rom, b"fixture").unwrap();
        let mut g = json!({"Roms":[{"Path":rom}],"GameActions":[{"Type":"Emulator","IsPlayAction":true,"EmulatorId":"emu","EmulatorProfileId":"profile","AdditionalArguments":"--fullscreen"}]});
        let em = json!({"Id":"emu","InstallDir":dir,"CustomProfiles":[{"Id":"profile","Executable":"{EmulatorDir}\\emu.exe","Arguments":"-g \"{ImagePath}\"","WorkingDirectory":"{EmulatorDir}"}]});
        let plan = launch_plan(&g, &[em.clone()], "", "").unwrap();
        assert_eq!(
            plan.arguments,
            format!("-g \"{}\" --fullscreen", rom.display())
        );
        g["GameActions"][0]["OverrideDefaultArgs"] = json!(true);
        g["GameActions"][0]["Arguments"] = json!("--custom \"{ImagePath}\"");
        let plan = launch_plan(&g, &[em], "", "").unwrap();
        assert_eq!(plan.arguments, format!("--custom \"{}\"", rom.display()));
        fs::remove_file(rom).unwrap();
        fs::remove_dir(dir).unwrap();
    }
    #[test]
    fn sdk_export_and_database_ids_both_resolve() {
        assert_eq!(
            id(&json!({"Id":"game-id"})),
            id(&json!({"_id":{"$guid":"game-id"}}))
        );
    }
    #[test]
    fn file_action_keeps_windows_argument_quoting() {
        let g = json!({"InstallDirectory":"D:\\Game Folder","GameActions":[{"Type":"File","IsPlayAction":true,"Path":"{InstallDir}\\game.exe","Arguments":"--name \"two words\" --value &literal"}]});
        let p = launch_plan(&g, &[], "", "").unwrap();
        assert_eq!(p.executable, r"D:\Game Folder\game.exe");
        assert_eq!(p.arguments, "--name \"two words\" --value &literal");
        assert_eq!(p.working_dir, r"D:\Game Folder");
    }
}
