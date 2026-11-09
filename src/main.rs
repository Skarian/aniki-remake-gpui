#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod artwork;
mod controller;
mod focus;
mod launch;
mod library;
mod media;
mod motion;
mod screens;
mod session;
use controller::{Controller, Input, Kind};
use gpui::{prelude::*, *};
use library::{Config, Game, Library, Update};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc, OnceLock,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    time::{Duration, Instant},
};
static PROCESS_OFFSET: OnceLock<f64> = OnceLock::new();
fn process_age_ms() -> f64 {
    use windows::Win32::{
        Foundation::FILETIME,
        System::Threading::{GetCurrentProcess, GetProcessTimes},
    };
    let (mut created, mut exited, mut kernel, mut user) = (
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
    );
    if unsafe {
        GetProcessTimes(
            GetCurrentProcess(),
            &mut created,
            &mut exited,
            &mut kernel,
            &mut user,
        )
    }
    .is_err()
    {
        return 0.;
    }
    let creation = ((created.dwHighDateTime as u64) << 32) | created.dwLowDateTime as u64;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos()
        / 100
        + 116444736000000000;
    (now.saturating_sub(creation as u128)) as f64 / 10000.
}
fn log(base: &Path, event: &str, detail: &str, start: Instant) {
    use std::io::Write;
    if let Ok(mut f) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(base.join("artifacts/session.jsonl"))
    {
        // Both roles append here. Format first so each complete record is one
        // append write rather than interleaved formatting fragments.
        let mut record = serde_json::to_vec(&serde_json::json!({"pid":std::process::id(),"event":event,"ms":start.elapsed().as_secs_f64()*1000.,"process_ms":start.elapsed().as_secs_f64()*1000.+PROCESS_OFFSET.get().copied().unwrap_or(0.),"detail":detail})).unwrap();
        record.push(b'\n');
        let _ = f.write_all(&record);
    }
}
#[derive(Clone, PartialEq)]
enum Phase {
    Library,
    Launching,
    Running,
    Failed(String),
}
struct Launcher {
    base: PathBuf,
    config: Config,
    library: Library,
    artwork: Entity<artwork::ArtworkCache>,
    trace_memory: bool,
    selected: usize,
    platform: usize,
    platforms: Vec<String>,
    focus: FocusHandle,
    rx: Receiver<Update>,
    refresh_paused: Arc<AtomicBool>,
    controller: Controller,
    music: media::Music,
    video: Option<media::Video>,
    video_frame: Option<Arc<RenderImage>>,
    retired: Vec<Arc<RenderImage>>,
    changed_at: Instant,
    music_started: bool,
    video_started: bool,
    phase: Phase,
    launch_rx: Option<Receiver<launch::LaunchEvent>>,
    game_minimized: bool,
    session: Option<session::Client>,
    session_ready: bool,
    session_revealed: bool,
    session_visible: bool,
    session_fade: motion::Tween,
    menu: bool,
    menu_index: usize,
    details: bool,
    details_action: usize,
    watching_video: bool,
    trailer_audio: Option<media::TrailerAudio>,
    details_scroll: ScrollHandle,
    status: String,
    start: Instant,
    first_render: bool,
    fixture: bool,
    benchmark: bool,
    preview: bool,
    preview_input: String,
    foreground_checked: bool,
    waveform_frame: u64,
    animation_frame: u64,
    keyboard: bool,
    rail: motion::Rail,
    platform_motion: motion::Tween,
    background_motion: motion::Tween,
    background_pending: bool,
    previous_background: String,
    previous_frame: Option<Arc<RenderImage>>,
    video_motion: motion::Tween,
    trace_motion: bool,
    page_motion: motion::Tween,
    last_page: &'static str,
    motion_was_active: bool,
    menu_motion: motion::Tween,
}
impl Launcher {
    fn new(
        base: PathBuf,
        config: Config,
        library: Library,
        rx: Receiver<Update>,
        window: &mut Window,
        cx: &mut Context<Self>,
        start: Instant,
        fixture: bool,
        refresh_paused: Arc<AtomicBool>,
    ) -> Self {
        let focus = cx.focus_handle();
        window.focus(&focus);
        let platforms = Self::platforms(&library);
        let saved = fs::read_to_string(base.join("cache/selection.txt")).unwrap_or_default();
        let selected = library
            .games
            .iter()
            .position(|g| g.id == saved)
            .unwrap_or(0);
        cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(16))
                    .await;
                if this
                    .update_in(cx, |this, window, cx| this.tick(window, cx))
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        let controller = Controller::start(base.join("SDL2.dll"));
        let args: Vec<String> = std::env::args().collect();
        let preview = args
            .iter()
            .position(|a| a == "--preview-screen")
            .and_then(|i| args.get(i + 1))
            .cloned()
            .unwrap_or_default();
        let mut launcher = Self {
            base,
            config,
            library,
            artwork: artwork::ArtworkCache::new(cx),
            trace_memory: args.iter().any(|a| a == "--trace-memory"),
            selected,
            platform: 0,
            platforms,
            focus,
            rx,
            refresh_paused,
            controller,
            music: media::Music::new(),
            video: None,
            video_frame: None,
            retired: Vec::new(),
            changed_at: Instant::now(),
            music_started: false,
            video_started: false,
            phase: Phase::Library,
            launch_rx: None,
            game_minimized: false,
            session: args
                .iter()
                .any(|a| a == "--session-ui")
                .then(session::Client::new),
            session_ready: false,
            session_revealed: false,
            session_visible: false,
            session_fade: motion::Tween::new(
                if args.iter().any(|a| a == "--session-ui") {
                    0.
                } else {
                    1.
                },
                start,
            ),
            menu: false,
            menu_index: 0,
            details: false,
            details_action: 0,
            watching_video: false,
            trailer_audio: None,
            details_scroll: ScrollHandle::new(),
            status: "Reading your Playnite library…".into(),
            start,
            first_render: false,
            fixture,
            benchmark: std::env::args().any(|a| a == "--benchmark"),
            preview: !preview.is_empty(),
            preview_input: args
                .iter()
                .position(|a| a == "--preview-input")
                .and_then(|i| args.get(i + 1))
                .cloned()
                .unwrap_or_default(),
            foreground_checked: false,
            waveform_frame: 0,
            animation_frame: 0,
            keyboard: true,
            rail: motion::Rail::new(start),
            platform_motion: motion::Tween::new(0., start),
            background_motion: motion::Tween::new(1., start),
            background_pending: false,
            previous_background: String::new(),
            previous_frame: None,
            video_motion: motion::Tween::new(1., start),
            trace_motion: args.iter().any(|a| a == "--trace-motion"),
            page_motion: motion::Tween::new(1., start),
            last_page: "",
            motion_was_active: false,
            menu_motion: motion::Tween::new(0., start),
        };
        if let Some(platform) = args
            .iter()
            .position(|a| a == "--resume-platform")
            .and_then(|i| args.get(i + 1))
        {
            if let Some(index) = launcher.platforms.iter().position(|p| p == platform) {
                launcher.platform = index;
                launcher.platform_motion = motion::Tween::new(index as f32, start);
            }
        }
        if launcher.preview {
            launcher.config.music = false;
            launcher.config.video = false;
            if let Some(index) = args
                .iter()
                .position(|a| a == "--preview-game-index")
                .and_then(|i| args.get(i + 1))
                .and_then(|value| value.parse::<usize>().ok())
            {
                launcher.selected = index.min(launcher.library.games.len().saturating_sub(1));
            }
        }
        match preview.as_str() {
            "details" => launcher.details=true,
            "menu" => launcher.menu=true,
            "launching" => launcher.phase=Phase::Launching,
            "running" => launcher.phase=Phase::Running,
            "error" => launcher.phase=Phase::Failed("The game executable could not be found. Check its installation in Playnite, then try again.".into()),
            "empty" => { launcher.platform=launcher.platforms.len()-1; launcher.library.games.iter_mut().for_each(|g|g.favorite=false); },
            "loading" => launcher.library.games.clear(),
            "empty-library" => { launcher.library.games.clear(); launcher.status = "No installed games were found in Playnite.".into(); },
            _ => (),
        }
        launcher
    }
    fn platforms(lib: &Library) -> Vec<String> {
        let mut p = vec!["All games".into()];
        for g in &lib.games {
            if !g.platform.is_empty() && !p.contains(&g.platform) {
                p.push(g.platform.clone());
            }
        }
        p.push("Favorites".into());
        p
    }
    fn visible(&self) -> Vec<usize> {
        self.library
            .games
            .iter()
            .enumerate()
            .filter(|(_, g)| {
                self.platform == 0
                    || (self.platforms[self.platform] == "Favorites" && g.favorite)
                    || g.platform == self.platforms[self.platform]
            })
            .map(|(i, _)| i)
            .collect()
    }
    fn game(&self) -> Option<&Game> {
        self.library.games.get(self.selected)
    }
    fn reset_media(&mut self) {
        self.trailer_audio = None;
        self.video = None;
        if let Some(frame) = self.video_frame.take() {
            self.retired.push(frame);
        }
        self.music.stop();
        self.changed_at = Instant::now();
        self.music_started = false;
        self.video_started = false;
    }
    fn select(&mut self, index: usize, cx: &mut Context<Self>) {
        if index >= self.library.games.len() || index == self.selected {
            return;
        }
        let now = Instant::now();
        // Keep a frozen outgoing image/video until the next artwork is decoded.
        if self.background_motion.value(now) >= 0.5 || self.previous_background.is_empty() {
            self.previous_background = self
                .game()
                .map(|g| g.background.clone())
                .unwrap_or_default();
            if let Some(frame) = self.previous_frame.take() {
                self.retired.push(frame);
            }
            self.previous_frame = self.video_frame.take();
        }
        self.background_motion = motion::Tween::new(0., now);
        self.background_pending = true;
        self.selected = index;
        self.reset_media();
        self.details = false;
        let _ = fs::write(
            self.base.join("cache/selection.txt"),
            &self.library.games[index].id,
        );
        log(
            &self.base,
            "selection",
            &self.library.games[index].name,
            self.start,
        );
        cx.notify();
    }
    fn set_platform(&mut self, index: usize, cx: &mut Context<Self>) {
        self.platform = index.min(self.platforms.len() - 1);
        self.platform_motion
            .to(self.platform as f32, 200, Instant::now());
        self.rail.reset();
        let visible = self.visible();
        if !visible.contains(&self.selected) {
            if let Some(first) = visible.first() {
                self.select(*first, cx);
            }
        }
        if visible.is_empty() {
            self.reset_media();
        }
        cx.notify();
    }
    fn launch(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.preview {
            self.details = false;
            self.phase = Phase::Launching;
            cx.notify();
            return;
        }
        if self.launch_rx.is_some() || self.visible().is_empty() {
            return;
        }
        let Some(game) = self.game().cloned() else {
            return;
        };
        if !game.launch.error.is_empty() {
            self.phase = Phase::Failed(game.launch.error);
            cx.notify();
            return;
        }
        self.reset_media();
        self.details = false;
        self.phase = Phase::Launching;
        let (tx, rx) = mpsc::channel();
        self.launch_rx = Some(rx);
        self.refresh_paused.store(true, Ordering::Relaxed);
        focus::release(window);
        log(
            &self.base,
            "focus_released",
            &focus::state(window),
            self.start,
        );
        log(&self.base, "launch_requested", &game.name, self.start);
        if self.session.is_some() {
            let message = session::UiMessage::Launch {
                plan: game.launch,
                name: game.name,
                platform: self.platforms[self.platform].clone(),
                hwnd: focus::hwnd(window).map_or(0, |h| h.0 as usize as u64),
            };
            if let Err(error) = session::Client::send(&message) {
                let _ = tx.send(launch::LaunchEvent::Failed(format!(
                    "Session host disconnected: {error}"
                )));
            }
        } else {
            launch::start(game.launch, tx);
        }
        cx.notify();
    }
    fn input(&mut self, input: Input, window: &mut Window, cx: &mut Context<Self>) {
        if self.session.is_some() && !self.session_visible {
            return;
        }
        log(&self.base, "input", &format!("{input:?}"), self.start);
        // Only the tracked process-exit event resumes the library during a game.
        if self.phase == Phase::Running && self.launch_rx.is_some() {
            return;
        }
        if self.watching_video {
            if input == Input::Back || input == Input::Video {
                self.watching_video = false;
                self.reset_media();
                cx.notify();
            }
            return;
        }
        if self.is_loading() && self.phase == Phase::Library {
            return;
        }
        if self.menu {
            match input {
                Input::Back | Input::Menu => self.menu = false,
                Input::Up => self.menu_index = (self.menu_index + 3) % 4,
                Input::Down => self.menu_index = (self.menu_index + 1) % 4,
                Input::Accept => match self.menu_index {
                    0 => self.menu = false,
                    1 => {
                        self.toggle_music();
                    }
                    2 => {
                        self.toggle_video();
                    }
                    _ => {
                        self.music.stop();
                        self.video = None;
                        cx.quit();
                    }
                },
                _ => {}
            }
            cx.notify();
            return;
        }
        if self.phase != Phase::Library {
            if input == Input::Back
                || matches!(self.phase, Phase::Failed(_)) && input == Input::Accept
            {
                self.phase = Phase::Library;
                self.reset_media();
                cx.notify();
            }
            return;
        }
        if self.details {
            match input {
                Input::Back => {
                    self.details = false;
                    self.details_scroll.set_offset(point(px(0.), px(0.)));
                }
                Input::Accept if self.details_action == 1 => self.watch_video(window, cx),
                Input::Accept => self.launch(window, cx),
                Input::Video => self.watch_video(window, cx),
                Input::Left | Input::Right => {
                    if self.game().is_some_and(|g| !g.video.is_empty()) {
                        self.details_action = 1 - self.details_action;
                    }
                }
                Input::Up | Input::Down => {
                    let mut offset = self.details_scroll.offset();
                    offset.y += px(if input == Input::Up { 64. } else { -64. });
                    self.details_scroll.set_offset(offset);
                }
                _ => {}
            }
            cx.notify();
            return;
        }
        match input {
            Input::Left | Input::Right => {
                let visible = self.visible();
                if !visible.is_empty() {
                    let current = visible
                        .iter()
                        .position(|i| *i == self.selected)
                        .unwrap_or(0);
                    let index = if input == Input::Right {
                        (current + 1) % visible.len()
                    } else {
                        (current + visible.len() - 1) % visible.len()
                    };
                    self.select(visible[index], cx);
                }
            }
            Input::PreviousPlatform => self.set_platform(
                (self.platform + self.platforms.len() - 1) % self.platforms.len(),
                cx,
            ),
            Input::NextPlatform => {
                self.set_platform((self.platform + 1) % self.platforms.len(), cx)
            }
            Input::Accept if self.visible().is_empty() => {
                if self.library.games.is_empty() {
                    self.menu = true;
                } else {
                    self.set_platform(0, cx);
                }
            }
            Input::Accept => self.open_details(cx),
            Input::Back => {
                if self.details {
                    self.details = false
                } else {
                    self.menu = true;
                    self.menu_index = 0;
                }
            }
            Input::Menu => {
                self.menu = true;
                self.menu_index = 0;
            }
            Input::Music => {
                self.toggle_music();
            }
            Input::Video => {
                self.toggle_video();
            }
            Input::Up | Input::Down => {
                if !self.visible().is_empty() {
                    self.open_details(cx);
                }
            }
        }
        let _ = window;
        cx.notify();
    }
    fn open_details(&mut self, cx: &mut Context<Self>) {
        if self.visible().is_empty() {
            return;
        }
        self.details = true;
        self.details_action = 0;
        self.details_scroll.set_offset(point(px(0.), px(0.)));
        cx.notify();
    }
    fn watch_video(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(game) = self.game().cloned().filter(|g| !g.video.is_empty()) else {
            return;
        };
        self.reset_media();
        let viewport = window.viewport_size();
        let width: f32 = viewport.width.into();
        let height: f32 = viewport.height.into();
        self.video = media::Video::watch(
            &self.config.ffmpeg,
            &game.video,
            width as u32,
            height as u32,
            self.config.video_threads,
        );
        if self.video.is_some() {
            self.watching_video = true;
            self.trailer_audio = media::TrailerAudio::start(
                &self.base.join("SDL2.dll"),
                &self.config.ffmpeg,
                &game.video,
            );
            log(&self.base, "trailer_opened", &game.name, self.start);
        }
        cx.notify();
    }
    fn toggle_music(&mut self) {
        self.config.music = !self.config.music;
        self.music.stop();
        self.music_started = false;
        self.save_config();
    }
    fn toggle_video(&mut self) {
        self.config.video = !self.config.video;
        if self.config.video {
            self.video_started = false;
        } else {
            self.video = None;
            self.video_motion.to(0., 180, Instant::now());
        }
        self.save_config();
    }
    fn save_config(&self) {
        if self.preview {
            return;
        }
        if let Ok(bytes) = serde_json::to_vec_pretty(&self.config) {
            let _ = library::atomic_write(&self.base.join("aniki-remake-gpui.json"), &bytes);
        }
    }
    fn tick(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let now = Instant::now();
        let messages: Vec<_> = self
            .session
            .as_ref()
            .map(|s| s.events.try_iter().collect())
            .unwrap_or_default();
        let mut remote_events = Vec::new();
        for message in messages {
            match message {
                session::HostMessage::Shutdown => {
                    self.reset_media();
                    log(&self.base, "ui_suspended", "", self.start);
                    cx.quit();
                    return;
                }
                session::HostMessage::Reveal => {
                    self.session_revealed = true;
                    self.session_fade.to(1., 360, now);
                    if self.trace_motion {
                        // The focus call can block. Record the actual tween
                        // origin, not the later foreground-confirmation time.
                        log(&self.base, "session_fade_started",
                            &format!("{:.6}", now.duration_since(self.start).as_secs_f64()*1000.), self.start);
                    }
                    focus::restore(window);
                    log(
                        &self.base,
                        "session_reveal",
                        &focus::state(window),
                        self.start,
                    );
                    cx.notify();
                }
                session::HostMessage::Activate => {
                    if self.session_visible && self.launch_rx.is_none() {
                        focus::restore(window);
                        log(&self.base, "session_activated", &focus::state(window), self.start);
                    }
                }
                session::HostMessage::Game(event) => remote_events.push(event),
            }
        }
        let (artwork_changed, stats, ready_at) = self.artwork.update(cx, |cache, cx| {
            let changed = cache.pump(window, cx);
            let stats =
                if self.trace_memory && cache.last_report.elapsed() >= Duration::from_secs(1) {
                    cache.last_report = now;
                    Some(serde_json::to_string(&cache.stats()).unwrap())
                } else {
                    None
                };
            (changed, stats, cache.ready_at.take())
        });
        if artwork_changed {
            cx.notify();
        }
        if self.session.is_some()
            && !self.session_ready
            && self.first_render
            && !self.background_pending
            && self.artwork.read(cx).settled()
        {
            self.session_ready = true;
            // Leave the native black cover in place until the completed scene
            // has passed through a frame. The UI itself is also opaque black.
            window.on_next_frame(|window, _| {
                let _ = session::Client::send(&session::UiMessage::Ready {
                    hwnd: focus::hwnd(window).map_or(0, |h| h.0 as usize as u64),
                });
            });
            cx.notify();
        }
        if self.session_revealed && !self.session_visible && !self.session_fade.active(now) {
            self.session_visible = true;
            let _ = session::Client::send(&session::UiMessage::Visible);
            log(&self.base, "session_fade_complete", "", self.start);
        }
        if let Some(stats) = stats {
            log(&self.base, "artwork_memory", &stats, self.start);
        }
        if self.trace_memory {
            if let Some(ready_at) = ready_at {
                log(
                    &self.base,
                    "artwork_ready",
                    &format!(
                        "decoded_at_ms={:.3}",
                        ready_at.duration_since(self.start).as_secs_f64() * 1000.
                    ),
                    self.start,
                );
            }
        }
        let motion_active = self.session_fade.active(now)
            || self.rail.active(now)
            || self.platform_motion.active(now)
            || self.background_motion.active(now)
            || self.video_motion.active(now)
            || self.page_motion.active(now)
            || self.menu_motion.active(now);
        if window.is_window_active() && (motion_active || self.motion_was_active) {
            cx.notify();
        }
        self.motion_was_active = motion_active;
        if self.background_pending && self.changed_at.elapsed() >= Duration::from_millis(120) {
            cx.notify();
        }
        if !self.watching_video
            && !self.config.video
            && self.video_motion.target == 0.
            && !self.video_motion.active(now)
        {
            if let Some(frame) = self.video_frame.take() {
                self.retired.push(frame);
            }
        }
        if !self.background_pending
            && !self.background_motion.active(now)
            && self.background_motion.target == 1.
        {
            self.previous_background.clear();
            if let Some(frame) = self.previous_frame.take() {
                self.retired.push(frame);
            }
        }
        if !self.foreground_checked && self.start.elapsed() > Duration::from_millis(500) {
            self.foreground_checked = true;
            if self.launch_rx.is_none() && (self.session.is_none() || self.session_revealed) {
                focus::restore(window);
            }
            log(
                &self.base,
                "startup_focus",
                &focus::state(window),
                self.start,
            );
        }
        let animation = self.start.elapsed().as_millis() as u64 / 33;
        if animation != self.animation_frame && self.is_loading() && window.is_window_active() {
            self.animation_frame = animation;
            cx.notify();
        }
        let frame = self.start.elapsed().as_millis() as u64 / 140;
        if frame != self.waveform_frame
            && self.config.music
            && self.music_started
            && window.is_window_active()
        {
            self.waveform_frame = frame;
            cx.notify();
        }
        while let Ok(status) = self.music.status.try_recv() {
            log(&self.base, "music_status", &status, self.start);
        }
        // GPUI can present a retained scene even when its view is clean.
        // Rebuild that scene whenever a previously displayed texture retires.
        if !self.retired.is_empty() {
            cx.notify();
        }
        for frame in self.retired.drain(..) {
            cx.drop_image(frame, Some(window));
        }
        if (self.fixture || self.benchmark) && self.start.elapsed() > Duration::from_secs(5) {
            cx.quit();
            return;
        }
        while let Ok(update) = self.rx.try_recv() {
            match update {
                Update::Library(library) => {
                    let paths = |lib: &Library| -> std::collections::HashSet<Resource> {
                        lib.games
                            .iter()
                            .flat_map(|g| [&g.cover, &g.background, &g.logo])
                            .chain(lib.platform_art.values())
                            .filter(|p| !p.is_empty())
                            .map(|p| Resource::Path(Arc::from(Path::new(p))))
                            .collect()
                    };
                    let old = paths(&self.library);
                    let new = paths(&library);
                    let removed = old.difference(&new).cloned().collect();
                    self.artwork
                        .update(cx, |cache, _| cache.retire_paths(&removed));
                    let selected = self.game().map(|g| g.id.clone());
                    let prior = self
                        .game()
                        .map(|g| (g.video.clone(), g.music.clone(), g.background.clone()));
                    let platform = self.platforms[self.platform].clone();
                    self.library = library;
                    self.rail.reset();
                    self.platforms = Self::platforms(&self.library);
                    self.platform = self
                        .platforms
                        .iter()
                        .position(|p| *p == platform)
                        .unwrap_or(0);
                    self.platform_motion
                        .to(self.platform as f32, 200, Instant::now());
                    self.selected = selected
                        .and_then(|id| self.library.games.iter().position(|g| g.id == id))
                        .unwrap_or(0);
                    if prior
                        != self
                            .game()
                            .map(|g| (g.video.clone(), g.music.clone(), g.background.clone()))
                    {
                        self.reset_media();
                    }
                    self.status = if self.library.games.is_empty() {
                        "No installed games were found in Playnite.".into()
                    } else {
                        "Library up to date".into()
                    };
                    log(
                        &self.base,
                        "library_updated",
                        &self.library.games.len().to_string(),
                        self.start,
                    );
                    cx.notify();
                }
                Update::Status(status) => {
                    self.status = status;
                    cx.notify();
                }
            }
        }
        let mut finished = false;
        {
            let events: Vec<_> = self
                .launch_rx
                .iter()
                .flat_map(|rx| rx.try_iter())
                .chain(remote_events)
                .collect();
            for event in events {
                match event {
                    launch::LaunchEvent::Focused(_) => {}
                    launch::LaunchEvent::Started => {
                        self.phase = Phase::Running;
                        self.reset_media();
                        self.previous_background.clear();
                        self.background_pending = false;
                        self.background_motion = motion::Tween::new(1., now);
                        if let Some(frame) = self.previous_frame.take() {
                            self.retired.push(frame);
                        }
                        for frame in self.retired.drain(..) {
                            cx.drop_image(frame, Some(window));
                        }
                        self.artwork.update(cx, |cache, cx| cache.clear(window, cx));
                        log(&self.base, "game_started", "", self.start);
                        cx.notify();
                    }
                    launch::LaunchEvent::Ended => {
                        self.phase = Phase::Library;
                        finished = true;
                        log(&self.base, "game_ended", "", self.start);
                        focus::restore(window);
                        log(
                            &self.base,
                            "return_focus",
                            &focus::state(window),
                            self.start,
                        );
                        cx.notify();
                    }
                    launch::LaunchEvent::Failed(message) => {
                        self.phase = Phase::Failed(message.clone());
                        focus::restore(window);
                        finished = true;
                        log(&self.base, "launch_failed", &message, self.start);
                        cx.notify();
                    }
                }
            }
        }
        if finished {
            self.launch_rx = None;
            self.game_minimized = false;
            self.refresh_paused.store(false, Ordering::Relaxed);
            self.reset_media();
            window.focus(&self.focus);
        }
        // Process creation precedes the game's first window. Keep the loading
        // surface visible until focus transfers, then release renderer surfaces.
        if self.session.is_none() && self.phase == Phase::Running && self.launch_rx.is_some() {
            if window.is_window_active() {
                self.game_minimized = false;
            } else if !self.game_minimized {
                self.game_minimized = true;
                window.minimize_window();
                log(&self.base, "game_minimized", "", self.start);
            }
        }
        let was_connected = self.controller.connected;
        let was_kind = self.controller.kind;
        let inputs = self.controller.poll();
        if was_connected != self.controller.connected || was_kind != self.controller.kind {
            log(
                &self.base,
                "controller",
                &format!(
                    "connected={};kind={:?}",
                    self.controller.connected, self.controller.kind
                ),
                self.start,
            );
            cx.notify();
        }
        if window.is_window_active() {
            for input in inputs {
                self.keyboard = false;
                self.input(input, window, cx);
            }
        }
        if self.watching_video {
            if !window.is_window_active() {
                self.watching_video = false;
                self.reset_media();
                cx.notify();
                return;
            }
            if let Some(audio) = &self.trailer_audio {
                while let Ok(status) = audio.status.try_recv() {
                    log(&self.base, "trailer_audio", status, self.start);
                }
            }
            if let Some(video) = &self.video {
                if let Some(frame) = video.take_frame() {
                    if let Some(old) = self.video_frame.replace(frame) {
                        cx.drop_image(old, Some(window));
                    }
                    cx.notify();
                }
            }
            if self.video.as_mut().is_some_and(|v| v.finished()) {
                self.watching_video = false;
                self.reset_media();
                cx.notify();
            }
            return;
        }
        if self.phase == Phase::Library
            && !self.visible().is_empty()
            && self.launch_rx.is_none()
            && window.is_window_active()
            && (self.session.is_none() || self.session_visible)
        {
            if !self.music_started && self.changed_at.elapsed() > Duration::from_millis(450) {
                self.music_started = true;
                if self.config.music {
                    if let Some(game) = self.game() {
                        self.music.play(&game.music, self.config.music_volume);
                    }
                }
            }
            if !self.video_started
                && self.changed_at.elapsed() > Duration::from_millis(self.config.video_delay_ms)
            {
                self.video_started = true;
                if self.config.video {
                    if let Some(game) = self.game() {
                        let viewport = window.viewport_size();
                        let width: f32 = viewport.width.into();
                        let height: f32 = viewport.height.into();
                        self.video = media::Video::start(
                            &self.config.ffmpeg,
                            &game.video,
                            width as u32,
                            height as u32,
                            self.config.video_threads,
                        );
                    }
                }
            }
            if let Some(video) = &self.video {
                if let Some(frame) = video.take_frame() {
                    if self.video_frame.is_none() || self.video_motion.target == 0. {
                        self.video_motion = motion::Tween::new(0., Instant::now());
                        self.video_motion.to(1., 220, Instant::now());
                    }
                    if let Some(old) = self.video_frame.replace(frame) {
                        cx.drop_image(old, Some(window));
                    }
                    cx.notify();
                }
            }
        } else if self.music_started || self.video_started {
            self.reset_media();
        }
    }
    fn image(path: &str) -> ImageSource {
        Arc::<Path>::from(PathBuf::from(path)).into()
    }
    fn prompt<'a>(&self, keyboard: &'a str, xbox: &'a str, ps: &'a str) -> &'a str {
        if self.preview {
            match self.preview_input.as_str() {
                "keyboard" => return keyboard,
                "xbox" => return xbox,
                "playstation" => return ps,
                _ => (),
            }
        }
        if self.keyboard || !self.controller.connected {
            keyboard
        } else if self.controller.kind == Kind::PlayStation {
            ps
        } else {
            xbox
        }
    }
    fn button(&self, id: &'static str, label: String, primary: bool, s: f32) -> Stateful<Div> {
        div()
            .id(id)
            .flex()
            .items_center()
            .justify_center()
            .h(px(44. * s))
            .min_w(px(if primary { 130. } else { 112. } * s))
            .px(px(24. * s))
            .gap(px(10. * s))
            .rounded(px(4. * s))
            .border_1()
            .border_color(if primary {
                rgb(0xe0f0ff)
            } else {
                rgba(0xffffff38)
            })
            .hover(move |d| {
                d.bg(if primary {
                    rgb(0xffffff)
                } else {
                    rgb(0x2b3b4a)
                })
            })
            .active(move |d| {
                d.bg(if primary {
                    rgb(0xa8d8ff)
                } else {
                    rgb(0x40546a)
                })
            })
            .bg(if primary {
                rgb(0xe0f0ff)
            } else {
                rgba(0x101923dd)
            })
            .text_color(if primary {
                rgb(0x102233)
            } else {
                rgb(0xffffff)
            })
            .text_size(px(16. * s))
            .font_weight(FontWeight::SEMIBOLD)
            .cursor_pointer()
            .when(label == "Play", |d| {
                d.child(div().text_size(px(12. * s)).child("▶"))
            })
            .child(label)
    }
    fn backdrop(&self, game: &Game, w: f32, h: f32) -> Div {
        let now = Instant::now();
        let mix = self.background_motion.value(now);
        let mut base = div().absolute().size_full().overflow_hidden();
        if !self.previous_background.is_empty() {
            base = base.child(
                img(Self::image(&self.previous_background))
                    .absolute()
                    .size_full()
                    .object_fit(ObjectFit::Cover),
            );
        }
        if let Some(frame) = &self.previous_frame {
            base = base.child(
                img(frame.clone())
                    .absolute()
                    .size_full()
                    .object_fit(ObjectFit::Cover),
            );
        }
        let zoom = 0.;
        let mut current = div()
            .absolute()
            .left(px(-w * zoom * 0.5))
            .top(px(-h * zoom * 0.5))
            .w(px(w * (1. + zoom)))
            .h(px(h * (1. + zoom)))
            .opacity(mix);
        if !game.background.is_empty() {
            current = current.child(
                img(Self::image(&game.background))
                    .absolute()
                    .size_full()
                    .object_fit(ObjectFit::Cover),
            );
        }
        if let Some(frame) = &self.video_frame {
            current = current.child(
                img(frame.clone())
                    .absolute()
                    .size_full()
                    .object_fit(ObjectFit::Cover)
                    .opacity(self.video_motion.value(now)),
            );
        }
        base.child(current)
    }
    fn page_shell(&self, scene: Stateful<Div>, _s: f32) -> ImageCacheElement {
        let progress = self.page_motion.smooth_value(Instant::now());
        image_cache(self.artwork.clone()).size_full().child(
            div()
                .size_full()
                .relative()
                .overflow_hidden()
                .bg(rgb(0x080d14))
                .child(scene.opacity(progress))
                .when(
                    self.session.is_some() && self.session_fade.smooth_value(Instant::now()) < 1.,
                    |d| {
                        d.child(
                            div()
                                .absolute()
                                .inset_0()
                                .bg(rgb(0x000000))
                                .opacity(1. - self.session_fade.smooth_value(Instant::now())),
                        )
                    },
                ),
        )
    }
}
impl Render for Launcher {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.artwork.update(cx, |cache, _| cache.begin_frame());
        if !self.first_render {
            self.first_render = true;
            let base = self.base.clone();
            let start = self.start;
            let managed = self.session.is_some();
            window.on_next_frame(move |window, _| {
                if !managed {
                    focus::restore(window);
                }
                log(&base, "first_frame_callback", "", start);
            });
            log(&self.base, "first_render", "", self.start);
            log(&self.base, "gpu", &format!("{:?}", window.gpu_specs()), self.start);
        }
        let bounds = window.viewport_size();
        let w: f32 = bounds.width.into();
        let h: f32 = bounds.height.into();
        let s = (w / 1280.).min(h / 720.).clamp(0.5, 3.);
        let pad = 54. * s;
        let footer_bottom = 20. * s;
        let footer_height = 36. * s;
        let rail_footer_gap = 28. * s;
        let visible = self.visible();
        let game = if visible.is_empty() {
            Game::default()
        } else {
            self.game().cloned().unwrap_or_default()
        };
        let selected_position = visible
            .iter()
            .position(|i| *i == self.selected)
            .unwrap_or(0);
        let now = Instant::now();
        self.menu_motion
            .to(if self.menu { 1. } else { 0. }, 190, now);
        let page = match self.phase {
            Phase::Launching => "launching",
            Phase::Running => "running",
            Phase::Failed(_) => "error",
            Phase::Library
                if self.library.games.is_empty() && !self.status.starts_with("No installed") =>
            {
                "loading"
            }
            Phase::Library if visible.is_empty() => "empty",
            Phase::Library if self.watching_video => "video",
            Phase::Library if self.details => "details",
            Phase::Library => "home",
        };
        if self.last_page != page {
            let browsing = |p| matches!(p, "home" | "empty");
            if !self.last_page.is_empty() && !(browsing(page) && browsing(self.last_page)) {
                self.page_motion = motion::Tween::new(0., now);
                self.page_motion
                    .to(1., if page == "details" { 340 } else { 240 }, now);
            } else {
                self.page_motion = motion::Tween::new(1., now);
            }
            self.last_page = page;
        }
        let pitch = 137. * s;
        let slots = (((w - 2. * pad) + 14. * s) / pitch).floor().max(1.) as usize;
        self.rail.sync(visible.len(), selected_position, slots, now);
        if self.phase != Phase::Running
            && self.background_pending
            && self.changed_at.elapsed() >= Duration::from_millis(120)
        {
            let ready = if let ImageSource::Resource(resource) = Self::image(&game.background) {
                game.background.is_empty()
                    || self
                        .artwork
                        .update(cx, |cache, cx| cache.load(&resource, window, cx))
                        .is_some()
            } else {
                true
            };
            if ready {
                self.background_pending = false;
                self.background_motion.to(1., 160, now);
            }
        }
        if self.trace_motion {
            log(&self.base, "motion_frame", &serde_json::json!({
                "selected": selected_position, "slots":slots, "rail":self.rail.offset.value(now),
                "rail_target":self.rail.offset.target, "scale":self.rail.selection.get(selected_position).map(|t|t.value(now)),
                "platform":self.platform_motion.value(now), "platform_target":self.platform,
                "artwork":self.background_motion.value(now), "video":self.video_motion.value(now),
                "page":page, "page_progress":self.page_motion.smooth_value(now),
                "session_fade":self.session_fade.smooth_value(now),
                "sample_ms":now.duration_since(self.start).as_secs_f64()*1000.,
                "menu":self.menu, "details_action":self.details_action
            }).to_string(), self.start);
        }
        // Follow GPUI's display-frame scheduling for motion, rather than the
        // controller timer (Windows can round a 16 ms timer up to ~32 ms).
        if window.is_window_active()
            && (self.session_fade.active(now) || self.rail.active(now)
                || self.platform_motion.active(now)
                || self.background_motion.active(now)
                || self.video_motion.active(now)
                || self.page_motion.active(now)
                || self.menu_motion.active(now))
        {
            window.request_animation_frame();
        }
        let mut root = div()
            .id(
                if self.phase != Phase::Library || self.library.games.is_empty() {
                    "status-page"
                } else if self.details {
                    "details-page"
                } else {
                    "library-page"
                },
            )
            .track_focus(&self.focus)
            .size_full()
            .relative()
            .overflow_hidden()
            .bg(rgb(0x0b1015))
            .text_color(rgb(0xf4f5f5))
            .font_family("Segoe UI")
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                // A held confirm must not select a game and then launch it.
                if event.is_held
                    && !matches!(
                        event.keystroke.key.as_str(),
                        "left" | "right" | "up" | "down"
                    )
                {
                    return;
                }
                this.keyboard = true;
                let input = match event.keystroke.key.as_str() {
                    "left" => Some(Input::Left),
                    "right" => Some(Input::Right),
                    "up" => Some(Input::Up),
                    "down" => Some(Input::Down),
                    "enter" => Some(Input::Accept),
                    "escape" => Some(Input::Back),
                    "f1" => Some(Input::Menu),
                    "q" => Some(Input::PreviousPlatform),
                    "e" | "tab" => Some(Input::NextPlatform),
                    "m" => Some(Input::Music),
                    "v" => Some(Input::Video),
                    "f11" => {
                        window.toggle_fullscreen();
                        if this.launch_rx.is_none() {
                            if window.is_fullscreen() {
                                focus::restore(window);
                            } else {
                                focus::release(window);
                            }
                        }
                        None
                    }
                    _ => None,
                };
                if let Some(input) = input {
                    this.input(input, window, cx);
                }
            }));
        if self.phase != Phase::Library
            || (self.library.games.is_empty() && !self.status.starts_with("No installed"))
        {
            root = self.status_page(root, &game, s, cx);
            return self.page_shell(root, s);
        }
        if self.watching_video {
            root = self.video_page(root, s, cx);
            return self.page_shell(root, s);
        }
        if self.details {
            root = self.details_page(root, &game, w, h, s, cx);
            return self.page_shell(root, s);
        }
        root = root.child(self.backdrop(&game, w, h));
        root = root
            .child(div().absolute().size_full().bg(linear_gradient(
                90.,
                linear_color_stop(rgba(0x05080ddc), 0.),
                linear_color_stop(rgba(0x05080d00), 0.72),
            )))
            .child(
                div()
                    .absolute()
                    .bottom_0()
                    .w_full()
                    .h(px(270. * s))
                    .bg(linear_gradient(
                        0.,
                        linear_color_stop(rgba(0x080d14f8), 0.),
                        linear_color_stop(rgba(0x080d1400), 1.),
                    )),
            )
            .child(
                div()
                    .absolute()
                    .top_0()
                    .w_full()
                    .h(px(130. * s))
                    .bg(linear_gradient(
                        180.,
                        linear_color_stop(rgba(0x080d14bb), 0.),
                        linear_color_stop(rgba(0x080d1400), 1.),
                    )),
            );
        let (previous, next) = (self.prompt("Q", "LB", "L1"), self.prompt("E", "RB", "R1"));
        let mut tabs = div().flex().gap(px(18. * s)).items_center().child(
            self.keycap(previous, s)
                .id("previous-platform")
                .cursor_pointer()
                .on_click(cx.listener(|this, _, window, cx| {
                    this.input(Input::PreviousPlatform, window, cx)
                })),
        );
        let mut platform_tabs = div().relative().h(px(54. * s)).flex().gap(px(18. * s));
        for (i, p) in self.platforms.iter().enumerate() {
            let mut tab = div()
                .id(("platform", i))
                .cursor_pointer()
                .h(px(54. * s))
                .w(px(137. * s))
                .px(px(16. * s))
                .flex()
                .items_center()
                .justify_center()
                .opacity(if i == self.platform { 1. } else { 0.8 })
                .hover(|d| d.opacity(1.).bg(rgba(0xffffff0c)))
                .text_size(px(16. * s))
                .font_weight(FontWeight::SEMIBOLD);
            if let Some(path) = self.library.platform_art.get(p) {
                tab = tab.child(
                    img(Self::image(path))
                        .w(px(105. * s))
                        .h(px(34. * s))
                        .object_fit(ObjectFit::Contain),
                );
            } else {
                tab = tab.child(p.clone());
            }
            platform_tabs = platform_tabs
                .child(tab.on_click(cx.listener(move |this, _, _, cx| this.set_platform(i, cx))));
        }
        platform_tabs = platform_tabs.child(
            div()
                .absolute()
                .bottom_0()
                .left(px(self.platform_motion.value(now) * 155. * s))
                .w(px(137. * s))
                .h(px(2. * s))
                .rounded(px(s))
                .bg(rgb(0xa8d8ff)),
        );
        tabs = tabs.child(platform_tabs);
        tabs = tabs.child(
            self.keycap(next, s)
                .id("next-platform")
                .cursor_pointer()
                .on_click(
                    cx.listener(|this, _, window, cx| this.input(Input::NextPlatform, window, cx)),
                ),
        );
        root = root.child(
            div()
                .absolute()
                .left(px(pad))
                .right(px(pad))
                .top(px(24. * s))
                .flex()
                .justify_center()
                .child(tabs),
        );
        if visible.is_empty() {
            let favorites =
                !self.library.games.is_empty() && self.platforms[self.platform] == "Favorites";
            let no_library = self.library.games.is_empty();
            let heading = if no_library {
                "No games"
            } else if favorites {
                "No favorites"
            } else {
                "No games"
            };
            root = root.child(
                div()
                    .absolute()
                    .left(px(pad))
                    .right(px(pad))
                    .top(px(135. * s))
                    .bottom(px(100. * s))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .items_center()
                            .text_center()
                            .w(px(480. * s))
                            .child(
                                div()
                                    .text_size(px(29. * s))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(rgb(0xf1f6fb))
                                    .child(heading),
                            )
                            .child(
                                div().mt(px(30. * s)).child(
                                    self.button(
                                        "empty-action",
                                        if no_library {
                                            "Open menu"
                                        } else {
                                            "Browse games"
                                        }
                                        .into(),
                                        true,
                                        s,
                                    )
                                    .on_click(cx.listener(
                                        move |this, _, _, cx| {
                                            if no_library {
                                                this.menu = true;
                                                this.menu_index = 0;
                                                cx.notify();
                                            } else {
                                                this.set_platform(0, cx);
                                            }
                                        },
                                    )),
                                ),
                            ),
                    ),
            );
        } else {
            let mut hero = div()
                .absolute()
                .left(px(pad))
                .top(px(90. * s))
                .w(px(570. * s));
            if !game.logo.is_empty() {
                hero = hero.child(
                    div().relative().w(px(410. * s)).h(px(184. * s)).child(
                        img(Self::image(&game.logo))
                            .absolute()
                            .size_full()
                            .object_fit(ObjectFit::Contain),
                    ),
                );
            } else {
                hero = hero.child(
                    div()
                        .h(px(184. * s))
                        .pt(px(75. * s))
                        .overflow_hidden()
                        .text_size(px(42. * s))
                        .font_weight(FontWeight::BOLD)
                        .line_height(relative(1.05))
                        .child(game.name.clone()),
                );
            }
            let meta = [
                game.year.clone(),
                game.genres
                    .split('·')
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_string(),
                if game.playtime > 0 {
                    format!("{:.1} hrs played", game.playtime as f64 / 3600.)
                } else {
                    "Not played yet".into()
                },
            ]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("   ·   ");
            hero = hero
                .child(
                    div()
                        .mt(px(18. * s))
                        .h(px(18. * s))
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .text_size(px(12. * s))
                        .text_color(rgb(0xc0c7ce))
                        .child(meta),
                )
                .child(
                    div()
                        .mt(px(12. * s))
                        .w(px(480. * s))
                        .h(px(44. * s))
                        .overflow_hidden()
                        .text_size(px(13. * s))
                        .line_height(relative(1.6))
                        .text_color(rgb(0xb0bac5))
                        .child(if game.description.is_empty() {
                            game.name.clone()
                        } else {
                            game.description.clone()
                        }),
                );
            root = root.child(hero);
            let offset = self.rail.offset.value(now);
            let begin = (offset.floor().max(0.) as usize).saturating_sub(1);
            let end = ((offset.ceil() as usize) + slots + 1).min(visible.len());
            let mut rail = div()
                .absolute()
                .left(px(pad))
                .right(px(pad))
                .bottom(px(footer_bottom + footer_height + rail_footer_gap))
                .h(px(177. * s))
                .overflow_hidden();
            for (position, index) in visible.iter().enumerate().take(end).skip(begin) {
                let index = *index;
                let g = &self.library.games[index];
                let focus = self.rail.selection[position].value(now);
                let cw = 107. + focus * 16.;
                let ch = 145. + focus * 21.;
                let mut card = div()
                    .id(("game", index))
                    .absolute()
                    .left(px(
                        (position as f32 - offset) * pitch + (123. - cw) * 0.5 * s
                    ))
                    .bottom_0()
                    .w(px(cw * s))
                    .flex_shrink_0()
                    .cursor_pointer()
                    .rounded(px(7. * s))
                    .hover(|d| d.shadow_lg())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.select(index, cx);
                        this.open_details(cx);
                    }));
                let mut cover = div()
                    .relative()
                    .w_full()
                    .h(px(ch * s))
                    .rounded(px(7. * s))
                    .overflow_hidden()
                    .border(px(2.5 * s))
                    .border_color(rgba(0xffffff00 | (focus * 255.) as u32))
                    .bg(rgb(0x25313c));
                if !g.cover.is_empty() {
                    cover = cover.child(
                        img(Self::image(&g.cover))
                            .size_full()
                            .object_fit(ObjectFit::Cover),
                    );
                } else {
                    cover = cover.child(div().p_3().text_size(px(13. * s)).child(g.name.clone()));
                }
                card = card.child(cover);
                rail = rail.child(card);
            }
            root = root.child(rail);
        }
        let playing = self.config.music && !game.music.is_empty();
        let mut wave = div().flex().gap(px(3. * s)).h(px(22. * s)).items_center();
        for i in 0..7 {
            let height = if playing {
                5. + ((self.waveform_frame as f32 * 0.7 + i as f32 * 1.6).sin() + 1.) * 7.
            } else {
                3.
            };
            wave = wave.child(
                div()
                    .w(px(3. * s))
                    .h(px(height * s))
                    .rounded(px(2. * s))
                    .bg(if playing {
                        rgb(0xa8d8ff)
                    } else {
                        rgb(0x748291)
                    }),
            );
        }
        root = root.child(
            div()
                .absolute()
                .bottom(px(footer_bottom))
                .left(px(pad))
                .right(px(pad))
                .flex()
                .justify_between()
                .items_center()
                .child(
                    div()
                        .id("music-toggle")
                        .when(visible.is_empty(), |d| d.invisible())
                        .px(px(14. * s))
                        .cursor_pointer()
                        .h(px(footer_height))
                        .rounded(px(3. * s))
                        .hover(|d| d.bg(rgba(0xffffff14)))
                        .active(|d| d.bg(rgba(0xffffff25)))
                        .flex()
                        .items_center()
                        .gap(px(12. * s))
                        .child(wave)
                        .child(
                            div()
                                .text_size(px(13. * s))
                                .text_color(rgb(0xe9f3fc))
                                .child("Music"),
                        )
                        .child(
                            div()
                                .opacity(0.65)
                                .child(self.keycap(self.prompt("M", "X", "□"), s * 0.7)),
                        )
                        .on_click(
                            cx.listener(|this, _, window, cx| this.input(Input::Music, window, cx)),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(30. * s))
                        .when(!visible.is_empty(), |d| {
                            d.child(
                                self.hint(self.prompt("Enter", "A", "\u{00d7}"), "Select", s)
                                    .cursor_pointer()
                                    .on_click(cx.listener(|this, _, _, cx| this.open_details(cx))),
                            )
                        })
                        .child(
                            div()
                                .id("menu")
                                .cursor_pointer()
                                .child(self.hint(self.prompt("Esc", "Start", "Options"), "Menu", s))
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.input(Input::Menu, window, cx)
                                })),
                        ),
                ),
        );
        self.page_shell(self.menu_layer(root, s, cx), s)
    }
}
fn base_dir() -> PathBuf {
    let exe = std::env::current_exe().unwrap();
    let mut path = exe.parent().unwrap().to_path_buf();
    for _ in 0..4 {
        if path.join("aniki-remake-gpui.json").exists() {
            return path;
        }
        if !path.pop() {
            break;
        }
    }
    std::env::current_dir().unwrap()
}
fn main() {
    let start = Instant::now();
    let _ = PROCESS_OFFSET.set(process_age_ms());
    let base = base_dir();
    let _ = fs::create_dir_all(base.join("cache"));
    let _ = fs::create_dir_all(base.join("artifacts"));
    let args: Vec<String> = std::env::args().collect();
    let managed = args.iter().any(|a| a == "--session-ui");
    let direct = args.iter().any(|a| {
        matches!(
            a.as_str(),
            "--preview-screen" | "--benchmark" | "--fixture-game" | "--import" | "--check-snapshot"
        )
    });
    if !managed && !direct {
        if let Err(error) = session::run(&base, &args[1..]) {
            log(&base, "host_failed", &error.to_string(), start);
            use windows::{Win32::UI::WindowsAndMessaging::*, core::HSTRING};
            unsafe {
                MessageBoxW(
                    None,
                    &HSTRING::from(error.to_string()),
                    &HSTRING::from("aniki-remake-gpui"),
                    MB_OK | MB_ICONERROR,
                );
            }
        }
        return;
    }
    let config: Config = fs::read(base.join("aniki-remake-gpui.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    let args: Vec<String> = std::env::args().collect();
    let fixture = args.iter().any(|a| a == "--fixture-game");
    let force_windowed = args.iter().any(|a| a == "--windowed") || fixture;
    let library: Library = fs::read(base.join("cache/library.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    if args.iter().any(|a| a == "--import") {
        let source = args
            .iter()
            .position(|a| a == "--raw")
            .and_then(|i| args.get(i + 1))
            .map(PathBuf::from)
            .unwrap_or(base.join("cache/raw.json"));
        let raw: serde_json::Value =
            serde_json::from_slice(&fs::read(source).expect("Run importer first")).unwrap();
        let mut library = library::normalize(&raw);
        library::prepare_assets(&mut library, &base.join("cache/artwork"));
        library::atomic_write(
            &base.join("cache/library.json"),
            &serde_json::to_vec(&library).unwrap(),
        )
        .unwrap();
        for g in &library.games {
            println!("{}: {} {}", g.name, g.launch.kind, g.launch.error)
        }
        return;
    }
    if let Some(i) = args.iter().position(|a| a == "--check-snapshot") {
        let raw: serde_json::Value =
            serde_json::from_slice(&fs::read(&args[i + 1]).unwrap()).unwrap();
        println!(
            "{}",
            serde_json::to_string(&library::normalize(&raw)).unwrap()
        );
        return;
    }
    let (tx, rx) = mpsc::channel();
    let refresh_paused = Arc::new(AtomicBool::new(false));
    if !fixture && !args.iter().any(|a| a == "--preview-screen") {
        library::start_refresh(base.clone(), config.clone(), tx, refresh_paused.clone());
    }
    log(
        &base,
        "snapshot_loaded",
        &library.games.len().to_string(),
        start,
    );
    Application::new().run(move |cx| {
        let bounds = Bounds::centered(None, size(px(1280.), px(720.)), cx);
        let fullscreen = config.fullscreen && !force_windowed;
        cx.open_window(
            WindowOptions {
                focus: !managed,
                window_bounds: Some(if fullscreen {
                    WindowBounds::Fullscreen(bounds)
                } else {
                    WindowBounds::Windowed(bounds)
                }),
                titlebar: Some(TitlebarOptions {
                    title: Some(if fixture {
                        "aniki-remake-gpui test game".into()
                    } else {
                        "aniki-remake-gpui".into()
                    }),
                    ..Default::default()
                }),
                window_min_size: Some(size(px(800.), px(500.))),
                ..Default::default()
            },
            move |window, cx| {
                cx.new(|cx| {
                    Launcher::new(
                        base,
                        config,
                        library,
                        rx,
                        window,
                        cx,
                        start,
                        fixture,
                        refresh_paused,
                    )
                })
            },
        )
        .expect("Create aniki-remake-gpui window");
        cx.on_window_closed(|cx| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        if !managed {
            cx.activate(true);
        }
    });
}
