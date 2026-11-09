//! One owner for file-backed decoded pixels. Video frames bypass this cache.
//! Visible resources are pinned by the last rendered frame, including outgoing
//! artwork during a crossfade. The 64 MiB budget is soft only when pins exceed it.
use gpui::{
    App, AppContext, Asset, Entity, ImageAssetLoader, ImageCache, ImageCacheError, RenderImage,
    Resource, WeakEntity, Window,
};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    time::Instant,
};

const BUDGET: usize = 64 * 1024 * 1024;
const DECODERS: usize = 2;
type Loaded = Result<Arc<RenderImage>, ImageCacheError>;
struct Entry {
    id: u64,
    used: u64,
    result: Option<Loaded>,
    bytes: usize,
    obsolete: bool,
}
struct Completion {
    resource: Resource,
    id: u64,
    result: Loaded,
}
struct Flight {
    estimated_bytes: Arc<AtomicUsize>,
}

#[derive(serde::Serialize)]
pub struct Stats {
    budget_bytes: usize,
    decoded_bytes: usize,
    pinned_bytes: usize,
    pinned_overage_bytes: usize,
    entries: usize,
    in_flight: usize,
    in_flight_estimated_bytes: usize,
    in_flight_unknown: usize,
    queued: usize,
    evictions: u64,
    stale_completions: u64,
    completed: u64,
}

pub struct ArtworkCache {
    owner: Option<WeakEntity<Self>>,
    entries: HashMap<Resource, Entry>,
    queue: VecDeque<(Resource, u64)>,
    flights: HashMap<u64, Flight>,
    tx: mpsc::Sender<Completion>,
    rx: mpsc::Receiver<Completion>,
    epoch: u64,
    serial: u64,
    budget: usize,
    evictions: u64,
    stale: u64,
    completed: u64,
    pub last_report: Instant,
    pub ready_at: Option<Instant>,
}
impl ArtworkCache {
    fn empty(budget: usize) -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            owner: None,
            entries: HashMap::new(),
            queue: VecDeque::new(),
            flights: HashMap::new(),
            tx,
            rx,
            epoch: 0,
            serial: 0,
            budget,
            evictions: 0,
            stale: 0,
            completed: 0,
            last_report: Instant::now(),
            ready_at: None,
        }
    }
    pub fn new(cx: &mut App) -> Entity<Self> {
        let entity = cx.new(|cx| {
            let mut cache = Self::empty(BUDGET);
            cache.owner = Some(cx.entity().downgrade());
            cache
        });
        cx.observe_release(&entity, |cache, cx| {
            for (_, entry) in cache.entries.drain() {
                if let Some(Ok(image)) = entry.result {
                    cx.drop_image(image, None);
                }
            }
        })
        .detach();
        entity
    }
    pub fn begin_frame(&mut self) {
        self.epoch += 1;
    }

    // Keep outgoing artwork until the renderer stops requesting it. Paths that
    // disappeared during live refresh are then retired even below the budget.
    pub fn retire_paths(&mut self, paths: &HashSet<Resource>) {
        for (path, entry) in &mut self.entries {
            if paths.contains(path) {
                entry.obsolete = true;
            }
        }
    }
    /// Keep only the current game's still artwork while its process runs.
    pub fn retain_only(&mut self, paths: &HashSet<Resource>, window: &mut Window, cx: &mut App) {
        for image in self.take_except(paths) {
            cx.drop_image(image, Some(window));
        }
    }
    fn take_except(&mut self, paths: &HashSet<Resource>) -> Vec<Arc<RenderImage>> {
        let mut images = Vec::new();
        self.entries.retain(|path, entry| {
            if paths.contains(path) {
                return true;
            }
            if let Some(Ok(image)) = entry.result.take() {
                images.push(image);
                self.evictions += 1;
            }
            false
        });
        self.queue.retain(|(path, _)| paths.contains(path));
        images
    }
    fn request(&mut self, resource: &Resource) -> Option<Loaded> {
        if let Some(entry) = self.entries.get_mut(resource) {
            entry.used = self.epoch;
            if entry.result.is_none()
                && !self.flights.contains_key(&entry.id)
                && !self.queue.iter().any(|(_, id)| *id == entry.id)
            {
                self.queue.push_back((resource.clone(), entry.id));
            }
            return entry.result.clone();
        }
        self.serial += 1;
        let id = self.serial;
        self.entries.insert(
            resource.clone(),
            Entry {
                id,
                used: self.epoch,
                result: None,
                bytes: 0,
                obsolete: false,
            },
        );
        self.queue.push_back((resource.clone(), id));
        None
    }
    fn accept(&mut self, completion: Completion) -> bool {
        let Some(entry) = self
            .entries
            .get_mut(&completion.resource)
            .filter(|entry| entry.id == completion.id && entry.used == self.epoch)
        else {
            self.stale += 1;
            if self
                .entries
                .get(&completion.resource)
                .is_some_and(|e| e.id == completion.id)
            {
                self.entries.remove(&completion.resource);
            }
            return false;
        };
        entry.bytes = completion
            .result
            .as_ref()
            .map(|image| {
                (0..image.frame_count())
                    .map(|i| image.as_bytes(i).map_or(0, |b| b.len()))
                    .sum()
            })
            .unwrap_or(0);
        entry.result = Some(completion.result);
        self.completed += 1;
        true
    }
    fn trim(&mut self) -> Vec<Arc<RenderImage>> {
        let mut bytes: usize = self.entries.values().map(|e| e.bytes).sum();
        let mut candidates: Vec<_> = self
            .entries
            .iter()
            .filter(|(_, e)| e.used != self.epoch)
            .map(|(r, e)| {
                (
                    r.clone(),
                    e.used,
                    e.obsolete,
                    e.result.is_none() || e.bytes == 0,
                )
            })
            .collect();
        candidates.sort_by_key(|(_, used, obsolete, empty)| (!(*obsolete || *empty), *used));
        let mut images = Vec::new();
        for (resource, _, obsolete, empty) in candidates {
            if bytes <= self.budget && !obsolete && !empty {
                continue;
            }
            if let Some(entry) = self.entries.remove(&resource) {
                bytes -= entry.bytes;
                if let Some(Ok(image)) = entry.result {
                    images.push(image);
                    self.evictions += 1;
                }
            }
        }
        self.queue
            .retain(|(r, id)| self.entries.get(r).is_some_and(|e| e.id == *id));
        images
    }
    fn start_queued(&mut self, window: &mut Window, cx: &mut App) {
        while self.flights.len() < DECODERS {
            let Some((resource, id)) = self.queue.pop_front() else {
                break;
            };
            if !self
                .entries
                .get(&resource)
                .is_some_and(|e| e.id == id && e.used == self.epoch)
            {
                continue;
            }
            // Call the loader directly: App::fetch_asset would retain another
            // completed task and defeat eviction. No full-library prefetch.
            let future = ImageAssetLoader::load(resource.clone(), cx);
            let tx = self.tx.clone();
            let estimated_bytes = Arc::new(AtomicUsize::new(0));
            let estimate = estimated_bytes.clone();
            let worker = cx.background_executor().spawn(async move {
                // Header IO happens on the worker, never in render/layout.
                // This estimate covers static pixel payload only, not decoder
                // scratch, compressed input or additional animated frames.
                if let Resource::Path(path) = &resource {
                    if let Ok(reader) = image::ImageReader::open(path) {
                        if let Ok(reader) = reader.with_guessed_format() {
                            if let Ok((w, h)) = reader.into_dimensions() {
                                estimate.store(
                                    (w as usize).saturating_mul(h as usize).saturating_mul(4),
                                    Ordering::Relaxed,
                                );
                            }
                        }
                    }
                }
                let result = future.await;
                Completion {
                    resource,
                    id,
                    result,
                }
            });
            self.flights.insert(id, Flight { estimated_bytes });
            let owner = self.owner.clone();
            // Wake on completion rather than spending a timer interval between
            // every two decodes. Weak ownership permits window/cache teardown.
            window
                .spawn(cx, async move |cx| {
                    let completion = worker.await;
                    if tx.send(completion).is_err() {
                        return;
                    }
                    let _ = cx.update(move |window, cx| {
                        if let Some(owner) = owner {
                            let _ = owner.update(cx, |cache, cx| {
                                if cache.pump(window, cx) {
                                    window.refresh();
                                }
                            });
                        }
                    });
                })
                .detach();
        }
    }
    pub fn pump(&mut self, window: &mut Window, cx: &mut App) -> bool {
        let mut changed = false;
        while let Ok(completion) = self.rx.try_recv() {
            self.flights.remove(&completion.id);
            changed |= self.accept(completion);
        }
        for image in self.trim() {
            cx.drop_image(image, Some(window));
        }
        self.start_queued(window, cx);
        if changed && self.flights.is_empty() && self.queue.is_empty() {
            self.ready_at = Some(Instant::now());
        }
        changed
    }

    pub fn stats(&self) -> Stats {
        let decoded_bytes = self.entries.values().map(|e| e.bytes).sum();
        let pinned_bytes: usize = self
            .entries
            .values()
            .filter(|e| e.used == self.epoch)
            .map(|e| e.bytes)
            .sum();
        Stats {
            budget_bytes: self.budget,
            decoded_bytes,
            pinned_bytes,
            pinned_overage_bytes: pinned_bytes.saturating_sub(self.budget),
            entries: self.entries.len(),
            in_flight: self.flights.len(),
            queued: self.queue.len(),
            evictions: self.evictions,
            in_flight_estimated_bytes: self
                .flights
                .values()
                .map(|f| f.estimated_bytes.load(Ordering::Relaxed))
                .sum(),
            in_flight_unknown: self
                .flights
                .values()
                .filter(|f| f.estimated_bytes.load(Ordering::Relaxed) == 0)
                .count(),
            stale_completions: self.stale,
            completed: self.completed,
        }
    }
}
impl ImageCache for ArtworkCache {
    fn load(&mut self, resource: &Resource, window: &mut Window, cx: &mut App) -> Option<Loaded> {
        let result = self.request(resource);
        self.start_queued(window, cx);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn resource(name: &str) -> Resource {
        Resource::Path(std::path::Path::new(name).into())
    }
    fn complete(cache: &mut ArtworkCache, resource: &Resource) -> Arc<RenderImage> {
        let image = Arc::new(RenderImage::new([image::Frame::new(
            image::RgbaImage::new(4, 4),
        )]));
        let id = cache.entries[resource].id;
        assert!(cache.accept(Completion {
            resource: resource.clone(),
            id,
            result: Ok(image.clone())
        }));
        image
    }
    #[test]
    fn budget_keeps_visible_and_evicts_oldest_pixels() {
        let mut cache = ArtworkCache::empty(128);
        let paths = [resource("old"), resource("recent"), resource("visible")];
        for path in &paths {
            cache.begin_frame();
            cache.request(path);
            complete(&mut cache, path);
        }
        let removed = cache.trim();
        assert_eq!(removed.len(), 1);
        assert!(!cache.entries.contains_key(&paths[0]));
        assert_eq!(cache.stats().decoded_bytes, 128);
        assert_eq!(cache.stats().pinned_bytes, 64);
    }
    #[test]
    fn crossfade_pins_can_exceed_budget_without_losing_images() {
        let mut cache = ArtworkCache::empty(64);
        let a = resource("outgoing");
        let b = resource("current");
        cache.begin_frame();
        cache.request(&a);
        complete(&mut cache, &a);
        cache.request(&b);
        complete(&mut cache, &b);
        assert!(cache.trim().is_empty());
        assert_eq!(cache.stats().pinned_overage_bytes, 64);
        cache.begin_frame();
        cache.request(&b);
        assert_eq!(cache.trim().len(), 1);
    }
    #[test]
    fn stale_decode_cannot_replace_new_request_and_old_paths_retire() {
        let mut cache = ArtworkCache::empty(1024);
        let a = resource("replaced");
        cache.begin_frame();
        cache.request(&a);
        let old_id = cache.entries[&a].id;
        cache.begin_frame();
        cache.trim();
        cache.request(&a);
        let image = Arc::new(RenderImage::new([image::Frame::new(
            image::RgbaImage::new(4, 4),
        )]));
        assert!(!cache.accept(Completion {
            resource: a.clone(),
            id: old_id,
            result: Ok(image)
        }));
        assert!(cache.entries[&a].result.is_none());
        complete(&mut cache, &a);
        cache.retire_paths(&HashSet::from([a]));
        assert!(cache.trim().is_empty()); // Still visible.
        cache.begin_frame();
        assert_eq!(cache.trim().len(), 1);
        assert_eq!(cache.stats().decoded_bytes, 0);
    }
    #[test]
    fn game_handoff_releases_pinned_pixels_and_rejects_late_completions() {
        let mut cache = ArtworkCache::empty(1024);
        let a = resource("visible");
        let b = resource("loading");
        cache.begin_frame();
        cache.request(&a);
        let image = complete(&mut cache, &a);
        let weak = Arc::downgrade(&image);
        drop(image);
        cache.request(&b);
        let id = cache.entries[&b].id;
        drop(cache.take_except(&HashSet::new()));
        assert!(weak.upgrade().is_none());
        assert_eq!(cache.stats().decoded_bytes, 0);
        assert_eq!(cache.stats().pinned_bytes, 0);
        assert!(cache.queue.is_empty());
        let image = Arc::new(RenderImage::new([image::Frame::new(
            image::RgbaImage::new(4, 4),
        )]));
        assert!(!cache.accept(Completion {
            resource: b,
            id,
            result: Ok(image)
        }));
        assert!(cache.entries.is_empty());
        cache.request(&a);
        complete(&mut cache, &a); // Return can reload normally.
    }
    #[test]
    fn gameplay_retains_only_return_artwork_and_discards_unrelated_decodes() {
        let mut cache = ArtworkCache::empty(1024);
        let keep = HashSet::from([resource("background"), resource("logo"), resource("cover")]);
        cache.begin_frame();
        for path in &keep {
            cache.request(path);
            complete(&mut cache, path);
        }
        let old = resource("old-background");
        cache.request(&old);
        let old_image = complete(&mut cache, &old);
        let weak = Arc::downgrade(&old_image);
        drop(old_image);
        let loading = resource("unrelated-loading-cover");
        cache.request(&loading);
        let id = cache.entries[&loading].id;
        drop(cache.take_except(&keep));
        assert!(weak.upgrade().is_none());
        assert_eq!(cache.stats().decoded_bytes, 3 * 64);
        assert_eq!(cache.entries.len(), 3);
        assert!(keep.iter().all(|p| cache.request(p).is_some()));
        assert!(!cache.accept(Completion {
            resource: loading,
            id,
            result: Ok(Arc::new(RenderImage::new([image::Frame::new(
                image::RgbaImage::new(4, 4)
            )]))),
        }));
        assert_eq!(cache.entries.len(), 3);
    }
    #[test]
    fn a_visible_request_can_requeue_after_an_obsolete_queue_item_was_skipped() {
        let mut cache = ArtworkCache::empty(1024);
        let a = resource("visible-again");
        cache.begin_frame();
        cache.request(&a);
        cache.queue.clear();
        cache.begin_frame();
        cache.request(&a);
        assert_eq!(cache.queue.len(), 1);
    }
}
