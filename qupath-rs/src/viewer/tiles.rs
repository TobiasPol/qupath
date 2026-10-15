//! Tiles: read on worker threads, kept in an LRU cache, turned into textures on demand.

use std::collections::{HashMap, HashSet};
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};

use crossbeam_channel::{Receiver, Sender, unbounded};
use egui::{ColorImage, TextureHandle, TextureOptions};

use crate::image::display::ImageDisplay;
use crate::image::{ImageServer, Tile};

/// Identifies a tile of one image.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TileKey {
    /// Changes when another image opens.
    pub image: u64,
    /// Pyramid level.
    pub level: usize,
    /// Column.
    pub tx: u32,
    /// Row.
    pub ty: u32,
}

struct Request {
    key: TileKey,
    server: Arc<dyn ImageServer>,
}

type Wanted = Arc<Mutex<HashSet<TileKey>>>;

/// Reads and caches tiles, and keeps one texture per visible tile.
pub struct TileManager {
    raw: lru::LruCache<TileKey, Arc<Tile>>,
    textures: HashMap<TileKey, (u64, TextureHandle, u64)>,
    pending: HashSet<TileKey>,
    failed: HashSet<TileKey>,
    wanted_now: HashSet<TileKey>,
    wanted: Wanted,
    requests: Sender<Request>,
    results: Receiver<(TileKey, anyhow::Result<Tile>)>,
    frame: u64,
    uploads_left: usize,
    budget_bytes: usize,
    used_bytes: usize,
}

impl TileManager {
    /// Starts the worker threads.
    pub fn new(ctx: &egui::Context, cache_mb: usize) -> Self {
        let (requests, req_rx) = unbounded::<Request>();
        let (res_tx, results) = unbounded();
        let wanted: Wanted = Arc::default();
        let workers = std::thread::available_parallelism().map_or(4, |n| n.get()).clamp(2, 8);
        for i in 0..workers {
            let (rx, tx, wanted, ctx) = (req_rx.clone(), res_tx.clone(), wanted.clone(), ctx.clone());
            let spawn = std::thread::Builder::new().name(format!("tile-reader-{i}")).spawn(move || worker(&rx, &tx, &wanted, &ctx));
            if let Err(e) = spawn {
                log::error!("cannot start tile reader: {e}");
            }
        }
        TileManager {
            raw: lru::LruCache::unbounded(),
            textures: HashMap::new(),
            pending: HashSet::new(),
            failed: HashSet::new(),
            wanted_now: HashSet::new(),
            wanted,
            requests,
            results,
            frame: 0,
            uploads_left: 0,
            budget_bytes: cache_mb.max(64) * 1024 * 1024,
            used_bytes: 0,
        }
    }

    /// Call once per frame before drawing.
    pub fn begin_frame(&mut self) {
        self.frame += 1;
        self.uploads_left = 24;
        self.wanted_now.clear();
        while let Ok((key, res)) = self.results.try_recv() {
            self.pending.remove(&key);
            match res {
                Ok(tile) => self.insert(key, tile),
                Err(e) => {
                    log::warn!("tile {key:?}: {e:#}");
                    self.failed.insert(key);
                }
            }
        }
    }

    fn insert(&mut self, key: TileKey, tile: Tile) {
        self.used_bytes += tile.byte_size();
        self.raw.put(key, Arc::new(tile));
        while self.used_bytes > self.budget_bytes {
            let Some((k, t)) = self.raw.pop_lru() else { break };
            self.used_bytes = self.used_bytes.saturating_sub(t.byte_size());
            self.textures.remove(&k);
        }
    }

    /// Asks for a tile to be read if it is not cached.
    pub fn request(&mut self, key: TileKey, server: &Arc<dyn ImageServer>) {
        self.wanted_now.insert(key);
        if self.raw.contains(&key) || self.pending.contains(&key) || self.failed.contains(&key) {
            return;
        }
        self.pending.insert(key);
        // Publish at once: a worker may take the request before `end_frame` runs.
        if let Ok(mut w) = self.wanted.lock() {
            w.insert(key);
        }
        let _ = self.requests.send(Request { key, server: server.clone() });
    }

    /// Call once per frame after drawing: workers skip tiles no longer wanted.
    pub fn end_frame(&mut self) {
        if let Ok(mut w) = self.wanted.lock() {
            w.clone_from(&self.wanted_now);
        }
        // Requests for tiles that scrolled away were dropped by the workers.
        let wanted = &self.wanted_now;
        self.pending.retain(|k| wanted.contains(k));
        let frame = self.frame;
        self.textures.retain(|_, (_, _, last)| frame - *last < 120);
    }

    /// The pixels of a cached tile.
    pub fn raw(&mut self, key: &TileKey) -> Option<Arc<Tile>> {
        self.raw.get(key).cloned()
    }

    /// A texture for a cached tile, refreshed when the display changed (a few per frame).
    pub fn texture(&mut self, ctx: &egui::Context, key: TileKey, display: &ImageDisplay) -> Option<egui::TextureId> {
        let frame = self.frame;
        if let Some((version, handle, last)) = self.textures.get_mut(&key) {
            *last = frame;
            if *version == display.version || self.uploads_left == 0 {
                return Some(handle.id());
            }
        }
        if self.uploads_left == 0 {
            return None;
        }
        let tile = self.raw.peek(&key)?.clone();
        self.uploads_left -= 1;
        let img = ColorImage::from_rgba_unmultiplied([tile.width as usize, tile.height as usize], &display.to_rgba(&tile));
        let opts = TextureOptions::LINEAR;
        match self.textures.get_mut(&key) {
            Some((version, handle, _)) => {
                handle.set(img, opts);
                *version = display.version;
                Some(handle.id())
            }
            None => {
                let handle = ctx.load_texture(format!("tile-{key:?}"), img, opts);
                let id = handle.id();
                self.textures.insert(key, (display.version, handle, frame));
                Some(id)
            }
        }
    }

    /// Whether tiles are still being read.
    pub fn busy(&self) -> bool {
        !self.pending.is_empty()
    }

    /// Forgets everything (another image, or a cache size change).
    pub fn clear(&mut self, cache_mb: usize) {
        self.raw.clear();
        self.textures.clear();
        self.pending.clear();
        self.failed.clear();
        self.used_bytes = 0;
        self.budget_bytes = cache_mb.max(64) * 1024 * 1024;
    }
}

fn worker(rx: &Receiver<Request>, tx: &Sender<(TileKey, anyhow::Result<Tile>)>, wanted: &Wanted, ctx: &egui::Context) {
    while let Ok(req) = rx.recv() {
        let still_wanted = wanted.lock().map_or(true, |w| w.is_empty() || w.contains(&req.key));
        if !still_wanted {
            continue;
        }
        let md = req.server.metadata();
        let (tw, th) = (md.tile_width, md.tile_height);
        let res = req.server.read_level_region(req.key.level, req.key.tx * tw, req.key.ty * th, tw, th);
        if tx.send((req.key, res)).is_err() {
            return;
        }
        ctx.request_repaint();
    }
}

/// An LRU sized by count, for small caches elsewhere in the viewer.
pub fn small_cache<K: std::hash::Hash + Eq, V>(n: usize) -> lru::LruCache<K, V> {
    lru::LruCache::new(NonZeroUsize::new(n.max(1)).unwrap_or(NonZeroUsize::MIN))
}
