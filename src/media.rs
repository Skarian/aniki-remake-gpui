use gpui::RenderImage;
use std::os::windows::io::FromRawHandle;
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
    child: Arc<Mutex<Option<Child>>>,
    stop: Arc<std::sync::atomic::AtomicBool>,
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
        let ffmpeg = ffmpeg.to_owned();
        let path = path.to_owned();
        let child = Arc::new(Mutex::new(None));
        let worker_child = child.clone();
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let worker_stop = stop.clone();
        let frame = Arc::new(Mutex::new(None));
        let target = frame.clone();
        let threads = threads.min(32).to_string();
        let reader = thread::spawn(move || {
            // Probe and decode off the UI thread. Both processes remain cancellable.
            let probe = Path::new(&ffmpeg).with_file_name("ffprobe.exe");
            let mut metadata = String::new();
            if let Some(mut pipe) = video_pipe(
                Command::new(probe).args([
                    "-v",
                    "error",
                    "-select_streams",
                    "v:0",
                    "-show_entries",
                    "stream=width,height",
                    "-of",
                    "csv=p=0:s=x",
                    &path,
                ]),
                &worker_child,
                &worker_stop,
            ) {
                let _ = pipe.read_to_string(&mut metadata);
            }
            let source = metadata
                .trim()
                .split_once('x')
                .and_then(|(w, h)| Some((w.parse::<u32>().ok()?, h.parse::<u32>().ok()?)))
                .unwrap_or((1920, 1080));
            let (width, height) = video_size(viewport_width, viewport_height, source.0, source.1);
            let sizing = if looping {
                format!("increase:flags=lanczos,crop={width}:{height}")
            } else {
                format!("decrease:flags=lanczos,pad={width}:{height}:(ow-iw)/2:(oh-ih)/2")
            };
            let filter = format!(
                "scale={width}:{height}:force_original_aspect_ratio={sizing},fps='min(source_fps,60)'"
            );
            let Some(mut stream) = video_pipe(
                Command::new(&ffmpeg).args([
                    "-hide_banner",
                    "-loglevel",
                    "error",
                    "-re",
                    "-stream_loop",
                    if looping { "-1" } else { "0" },
                    "-threads",
                    &threads,
                    "-i",
                    &path,
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
                ]),
                &worker_child,
                &worker_stop,
            ) else {
                return;
            };
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
            stop,
            frame,
            reader: Some(reader),
        })
    }
    pub fn finished(&mut self) -> bool {
        self.reader
            .as_ref()
            .is_none_or(|reader| reader.is_finished())
    }
    pub fn take_frame(&self) -> Option<Arc<RenderImage>> {
        self.frame.lock().ok()?.take()
    }
}
impl Drop for Video {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
        if let Some(mut child) = self.child.lock().unwrap().take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

fn video_pipe(
    command: &mut Command,
    slot: &Mutex<Option<Child>>,
    stop: &std::sync::atomic::AtomicBool,
) -> Option<std::fs::File> {
    let mut slot = slot.lock().unwrap();
    if stop.load(std::sync::atomic::Ordering::Relaxed) {
        return None;
    }
    if let Some(mut previous) = slot.take() {
        let _ = previous.wait();
    }
    // Windows' small default anonymous pipe throttles 4K BGRA transfers.
    // A bounded 1 MiB pipe avoids thousands of tiny producer/consumer wakeups.
    let (read, write) = unsafe {
        use windows::Win32::{Foundation::HANDLE, System::Pipes::CreatePipe};
        let (mut read, mut write) = (HANDLE::default(), HANDLE::default());
        CreatePipe(&mut read, &mut write, None, 1024 * 1024).ok()?;
        (
            std::fs::File::from_raw_handle(read.0),
            std::fs::File::from_raw_handle(write.0),
        )
    };
    let spawned = command
        .stdin(Stdio::null())
        .stdout(Stdio::from(write))
        .stderr(Stdio::null())
        .creation_flags(0x08000000)
        .spawn();
    // Command retains supplied handles after spawn; release the parent's writer
    // so the reader sees EOF when the child exits (including cancellation).
    command.stdout(Stdio::null());
    let child = spawned.ok()?;
    *slot = Some(child);
    Some(read)
}

// Bound uploads by the actual display, source detail and 4K. GPU scaling handles
// any remaining enlargement, rather than upscaling a 1080p file in the decoder.
fn video_size(
    view_width: u32,
    view_height: u32,
    source_width: u32,
    source_height: u32,
) -> (u32, u32) {
    let scale = (view_width as f32 / 3840.)
        .min(view_height as f32 / 2160.)
        .min(source_width as f32 / 3840.)
        .min(source_height as f32 / 2160.)
        .min(1.);
    (
        ((3840. * scale) as u32 / 2 * 2).max(2),
        ((2160. * scale) as u32 / 2 * 2).max(2),
    )
}

#[cfg(test)]
mod video_tests {
    use super::*;

    #[test]
    fn video_resolution_preserves_source_detail_with_bounded_uploads() {
        assert_eq!(video_size(3840, 2160, 3840, 2160), (3840, 2160));
        assert_eq!(video_size(3840, 2160, 1920, 1080), (1920, 1080));
        assert_eq!(video_size(2560, 1440, 3840, 2160), (2560, 1440));
        assert_eq!(video_size(7680, 4320, 7680, 4320), (3840, 2160));
        assert_eq!(video_size(3440, 1440, 3840, 2160), (2560, 1440));
        assert_eq!(video_size(0, 0, 0, 0), (2, 2));
    }

    #[test]
    fn cancelled_video_worker_cannot_spawn_another_decoder() {
        let slot = Mutex::new(None);
        let stop = std::sync::atomic::AtomicBool::new(true);
        assert!(video_pipe(&mut Command::new("must-not-be-started.exe"), &slot, &stop).is_none());
        assert!(slot.lock().unwrap().is_none());
    }

    #[test]
    #[ignore = "requires ANIKI_TEST_VIDEO and ANIKI_TEST_FFMPEG; real decoder, no UI"]
    fn real_video_preserves_resolution_pacing_and_stops() {
        use std::time::{Duration, Instant};
        let path = std::env::var("ANIKI_TEST_VIDEO").unwrap();
        let ffmpeg = std::env::var("ANIKI_TEST_FFMPEG").unwrap();
        let width = std::env::var("ANIKI_TEST_WIDTH")
            .unwrap()
            .parse::<u32>()
            .unwrap();
        let height = std::env::var("ANIKI_TEST_HEIGHT")
            .unwrap()
            .parse::<u32>()
            .unwrap();
        let fps = std::env::var("ANIKI_TEST_FPS")
            .unwrap()
            .parse::<f64>()
            .unwrap();
        let mut video = Video::start(&ffmpeg, &path, width, height, 2).unwrap();
        let started = Instant::now();
        let mut measured = None;
        let mut frames = 0;
        loop {
            assert!(
                started.elapsed() < Duration::from_secs(20),
                "decoder stalled"
            );
            assert!(!video.finished(), "decoder exited early");
            if let Some(frame) = video.take_frame() {
                let size = frame.size(0);
                assert_eq!((size.width.0 as u32, size.height.0 as u32), (width, height));
                // Exclude FFmpeg's initial real-time catch-up burst.
                if started.elapsed() < Duration::from_secs(2) {
                    continue;
                }
                if let Some(since) = measured {
                    frames += 1;
                    let elapsed: Duration = Instant::now() - since;
                    if elapsed >= Duration::from_secs(5) {
                        let rate = frames as f64 / elapsed.as_secs_f64();
                        println!(
                            "{width}x{height}: {rate:.1} frames/s (source {fps}); measurement began {:?}",
                            since - started
                        );
                        assert!(
                            rate >= fps * 0.9 && rate <= fps * 1.15,
                            "decoder pacing: {rate}"
                        );
                        break;
                    }
                } else {
                    measured = Some(Instant::now());
                }
            }
            thread::sleep(Duration::from_millis(1));
        }
        let ended = Instant::now();
        drop(video);
        assert!(
            ended.elapsed() < Duration::from_secs(2),
            "decoder shutdown stalled"
        );
    }
}
pub struct Music {
    tx: Sender<(String, u32)>,
    pub status: mpsc::Receiver<String>,
}
impl Music {
    pub fn new(sdl: std::path::PathBuf, ffmpeg: String) -> Self {
        let (tx, rx) = mpsc::channel::<(String, u32)>();
        let (status_tx, status) = mpsc::channel();
        thread::spawn(move || {
            let mut audio: Option<TrailerAudio> = None;
            loop {
                match rx.recv_timeout(std::time::Duration::from_millis(25)) {
                    Ok((mut path, mut volume)) => {
                        while let Ok((p, v)) = rx.try_recv() {
                            path = p;
                            volume = v;
                        }
                        // Close the old SDL queue before opening another track.
                        drop(audio.take());
                        if !path.is_empty() {
                            audio = TrailerAudio::open(
                                &sdl,
                                &ffmpeg,
                                &path,
                                true,
                                volume as f32 / 1000.,
                            );
                            if audio.is_none() {
                                let _ = status_tx.send("Music decoder could not start".into());
                            }
                        }
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }
                if let Some(audio) = &audio {
                    while let Ok(message) = audio.status.try_recv() {
                        let _ = status_tx.send(message.into());
                    }
                }
            }
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

// Audio for music and the explicit trailer view. The existing SDL runtime owns a
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
        Self::open(sdl, ffmpeg, path, false, 1.)
    }
    fn open(sdl: &Path, ffmpeg: &str, path: &str, looping: bool, volume: f32) -> Option<Self> {
        if !Path::new(path).is_file() {
            return None;
        }
        let gain = format!("volume={}", volume.clamp(0., 1.));
        let mut child = Command::new(ffmpeg)
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-stream_loop",
                if looping { "-1" } else { "0" },
                "-threads",
                "1",
                "-i",
                path,
                "-vn",
                "-filter_threads",
                "1",
                "-af",
                &gain,
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
                    let _ = status_tx.send("SDL audio initialization failed");
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
                    let _ = status_tx.send("SDL audio output could not open");
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
                if !reported {
                    let _ = status_tx.send("Audio decoder produced no samples");
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
