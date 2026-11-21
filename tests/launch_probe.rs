//! Test executable using the production worker, import model and launch loop.
#![allow(dead_code)]
#[path = "../src/launch.rs"]
mod launch;
#[path = "../src/library.rs"]
mod library;
#[path = "../src/plugin.rs"]
mod plugin;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::Duration,
};
fn main() {
    let args: Vec<_> = std::env::args().collect();
    if args.iter().any(|a| a == "--plugin-worker") {
        std::process::exit(plugin::worker_main());
    }
    let plan = serde_json::from_slice(&std::fs::read(&args[1]).unwrap()).unwrap();
    let cancel = Arc::new(AtomicBool::new(false));
    let (tx, rx) = mpsc::channel();
    launch::start(plan, tx, cancel.clone());
    if let Some(delay) = args.get(2) {
        let delay = delay.parse::<u64>().unwrap();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(delay));
            cancel.store(true, Ordering::Relaxed);
        });
    }
    loop {
        match rx.recv_timeout(Duration::from_secs(15)) {
            Ok(event) => println!("{}", serde_json::to_string(&event).unwrap()),
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                panic!("Launch probe exceeded 15 seconds without an event")
            }
        }
    }
}
