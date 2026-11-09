use gpui::RenderImage;
use std::os::windows::process::CommandExt;
use std::{
    io::Read,
    path::Path,
    process::{Child, Command, Stdio},
    sync::{
        Arc, Mutex,
        mpsc::{self, Sender},
    },
    thread,
};
pub struct Video {
    child: Child,
    pub frame: Arc<Mutex<Option<Arc<RenderImage>>>>,
    reader: Option<thread::JoinHandle<()>>,
}
impl Video {
    pub fn start(
        ffmpeg: &str,
        path: &str,
        viewport_width: u32,
        viewport_height: u32,
        threads: u32,
    ) -> Option<Self> {
        Self::open(ffmpeg, path, viewport_width, viewport_height, true, threads)
    }
    pub fn watch(ffmpeg: &str, path: &str, width: u32, height: u32, threads: u32) -> Option<Self> {
        Self::open(ffmpeg, path, width, height, false, threads)
    }
    fn open(
        ffmpeg: &str,
        path: &str,
        viewport_width: u32,
        viewport_height: u32,
        looping: bool,
        threads: u32,
    ) -> Option<Self> {
        if ffmpeg.is_empty() || !Path::new(path).is_file() {
            return None;
        }
        // Preserve 1080p trailer detail; cap decode/upload work on larger displays.
        let scale = (viewport_width as f32 / 1920.)
            .min(viewport_height as f32 / 1080.)
            .clamp(0.5, 1.);
        let width = ((1920. * scale) as u32 / 2) * 2;
        let height = ((1080. * scale) as u32 / 2) * 2;
        let filter = if looping {
            format!(
                "scale={width}:{height}:force_original_aspect_ratio=increase:flags=lanczos,crop={width}:{height},fps=30"
            )
        } else {
            format!(
                "scale={width}:{height}:force_original_aspect_ratio=decrease:flags=lanczos,pad={width}:{height}:(ow-iw)/2:(oh-ih)/2,fps=30"
            )
        };
        let threads = threads.min(32).to_string();
        let mut child = Command::new(ffmpeg)
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-re",
                "-stream_loop",
                if looping { "-1" } else { "0" },
                "-threads",
                &threads,
                "-i",
                path,
                "-filter_threads",
                &threads,
                "-threads",
                &threads,
                "-an",
                "-vf",
                &filter,
                "-pix_fmt",
                "bgra",
                "-f",
                "rawvideo",
                "pipe:1",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .creation_flags(0x08000000)
            .spawn()
            .ok()?;
        let mut stream = child.stdout.take()?;
        let frame = Arc::new(Mutex::new(None));
        let target = frame.clone();
        let reader = thread::spawn(move || {
            loop {
                let mut bytes = vec![0u8; (width * height * 4) as usize];
                if stream.read_exact(&mut bytes).is_err() {
                    break;
                }
                let image = image::RgbaImage::from_raw(width, height, bytes).unwrap();
                let rendered = Arc::new(RenderImage::new([image::Frame::new(image)]));
                if let Ok(mut slot) = target.lock() {
                    *slot = Some(rendered)
                } else {
                    break;
                }
            }
        });
        Some(Self {
            child,
            frame,
            reader: Some(reader),
        })
    }
    pub fn finished(&mut self) -> bool {
        self.child.try_wait().ok().flatten().is_some()
    }
    pub fn take_frame(&self) -> Option<Arc<RenderImage>> {
        self.frame.lock().ok()?.take()
    }
}
impl Drop for Video {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}
pub struct Music {
    tx: Sender<(String, u32)>,
    pub status: mpsc::Receiver<String>,
}
impl Music {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel::<(String, u32)>();
        let (status_tx, status) = mpsc::channel();
        thread::spawn(move || {
            fn send(command: &str) -> u32 {
                let wide: Vec<u16> = command.encode_utf16().chain(Some(0)).collect();
                unsafe {
                    windows::Win32::Media::Multimedia::mciSendStringW(
                        windows::core::PCWSTR(wide.as_ptr()),
                        None,
                        None,
                    )
                }
            }
            while let Ok((mut path, mut volume)) = rx.recv() {
                while let Ok((p, v)) = rx.try_recv() {
                    path = p;
                    volume = v;
                }
                send("close lumen_music");
                if !path.is_empty() && !path.contains('"') {
                    let opened = send(&format!("open \"{path}\" type mpegvideo alias lumen_music"));
                    let volume_result = send(&format!("setaudio lumen_music volume to {volume}"));
                    let played = send("play lumen_music repeat");
                    let _ = status_tx.send(format!(
                        "open={opened}; volume={volume_result}; play={played}"
                    ));
                }
            }
            send("close lumen_music");
        });
        Self { tx, status }
    }
    pub fn play(&self, path: &str, volume: f32) {
        let _ = self
            .tx
            .send((path.into(), (volume.clamp(0., 1.) * 1000.) as u32));
    }
    pub fn stop(&self) {
        self.play("", 0.);
    }
}

// Audio for the explicit trailer view only. The existing SDL runtime owns a
// bounded PCM queue; no player window, audio transcode cache, or startup work.
pub struct TrailerAudio {
    child: Child,
    stop: Arc<std::sync::atomic::AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
    pub status: mpsc::Receiver<&'static str>,
}
#[repr(C)]
struct AudioSpec {
    freq: i32,
    format: u16,
    channels: u8,
    silence: u8,
    samples: u16,
    padding: u16,
    size: u32,
    callback: Option<unsafe extern "C" fn(*mut std::ffi::c_void, *mut u8, i32)>,
    userdata: *mut std::ffi::c_void,
}
impl TrailerAudio {
    pub fn start(sdl: &Path, ffmpeg: &str, path: &str) -> Option<Self> {
        let mut child = Command::new(ffmpeg)
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-i",
                path,
                "-vn",
                "-ac",
                "2",
                "-ar",
                "48000",
                "-f",
                "s16le",
                "pipe:1",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .creation_flags(0x08000000)
            .spawn()
            .ok()?;
        let mut stream = child.stdout.take()?;
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stopped = stop.clone();
        let (status_tx, status) = mpsc::channel();
        let sdl = sdl.to_path_buf();
        let worker = thread::spawn(move || {
            use std::sync::atomic::Ordering;
            // SDL2 ABI: https://wiki.libsdl.org/SDL2/SDL_AudioSpec
            unsafe {
                let Ok(lib) = libloading::Library::new(sdl) else {
                    return;
                };
                macro_rules! symbol {
                    ($name:literal, $ty:ty) => {
                        match lib.get::<$ty>(concat!($name, "\0").as_bytes()) {
                            Ok(f) => *f,
                            Err(_) => return,
                        }
                    };
                }
                let init = symbol!("SDL_InitSubSystem", unsafe extern "C" fn(u32) -> i32);
                let quit = symbol!("SDL_QuitSubSystem", unsafe extern "C" fn(u32));
                let open = symbol!(
                    "SDL_OpenAudioDevice",
                    unsafe extern "C" fn(
                        *const i8,
                        i32,
                        *const AudioSpec,
                        *mut AudioSpec,
                        i32,
                    ) -> u32
                );
                let close = symbol!("SDL_CloseAudioDevice", unsafe extern "C" fn(u32));
                let queue = symbol!(
                    "SDL_QueueAudio",
                    unsafe extern "C" fn(u32, *const u8, u32) -> i32
                );
                let queued = symbol!("SDL_GetQueuedAudioSize", unsafe extern "C" fn(u32) -> u32);
                let pause = symbol!("SDL_PauseAudioDevice", unsafe extern "C" fn(u32, i32));
                if init(0x10) != 0 {
                    return;
                }
                let spec = AudioSpec {
                    freq: 48000,
                    format: 0x8010,
                    channels: 2,
                    silence: 0,
                    samples: 1024,
                    padding: 0,
                    size: 0,
                    callback: None,
                    userdata: std::ptr::null_mut(),
                };
                let device = open(std::ptr::null(), 0, &spec, std::ptr::null_mut(), 0);
                if device == 0 {
                    quit(0x10);
                    return;
                }
                pause(device, 0);
                let mut bytes = [0u8; 4096];
                let mut remainder = 0;
                let mut reported = false;
                while !stopped.load(Ordering::Relaxed) {
                    while queued(device) > 8192 && !stopped.load(Ordering::Relaxed) {
                        thread::sleep(std::time::Duration::from_millis(4));
                    }
                    if stopped.load(Ordering::Relaxed) {
                        break;
                    }
                    match stream.read(&mut bytes[remainder..]) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            let total = remainder + n;
                            let aligned = total / 4 * 4;
                            if aligned > 0 && queue(device, bytes.as_ptr(), aligned as u32) != 0 {
                                break;
                            }
                            remainder = total - aligned;
                            bytes.copy_within(aligned..total, 0);
                            if aligned > 0 && !reported {
                                let _ = status_tx.send("playing; 48000 Hz stereo PCM queued");
                                reported = true;
                            }
                        }
                    }
                }
                while queued(device) > 0 && !stopped.load(Ordering::Relaxed) {
                    thread::sleep(std::time::Duration::from_millis(4));
                }
                close(device);
                quit(0x10);
            }
        });
        Some(Self {
            child,
            stop,
            worker: Some(worker),
            status,
        })
    }
}
impl Drop for TrailerAudio {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
        let _ = self.child.kill();
        let _ = self.child.wait();
        // Closing the pipe unblocks the reader; do not leave a playing audio device.
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
