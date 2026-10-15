//! Finding and loading the OpenSlide library at runtime, for vendor formats (NDPI, MRXS, SCN, VMS, BIF, JPEG 2000 SVS...).
//!
//! The library is looked up in `QUPATH_RS_OPENSLIDE`, the usual install
//! locations, and finally inside an installed QuPath, whose
//! `openslide-*.jar` carries a self-contained `libopenslide`.

use std::ffi::{c_char, c_void};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use anyhow::{Context, Result, anyhow, bail};
use libloading::Library;

/// An OpenSlide handle.
pub(super) type Osr = *mut c_void;

/// The functions this reader calls.
pub(super) struct Api {
    _lib: Library,
    pub(super) open: unsafe extern "C" fn(*const c_char) -> Osr,
    pub(super) close: unsafe extern "C" fn(Osr),
    pub(super) get_error: unsafe extern "C" fn(Osr) -> *const c_char,
    pub(super) level_count: unsafe extern "C" fn(Osr) -> i32,
    pub(super) level_dimensions: unsafe extern "C" fn(Osr, i32, *mut i64, *mut i64),
    pub(super) level_downsample: unsafe extern "C" fn(Osr, i32) -> f64,
    pub(super) read_region: unsafe extern "C" fn(Osr, *mut u32, i64, i64, i32, i64, i64),
    pub(super) property_names: unsafe extern "C" fn(Osr) -> *const *const c_char,
    pub(super) property_value: unsafe extern "C" fn(Osr, *const c_char) -> *const c_char,
    pub(super) associated_names: unsafe extern "C" fn(Osr) -> *const *const c_char,
    pub(super) associated_dimensions: unsafe extern "C" fn(Osr, *const c_char, *mut i64, *mut i64),
    pub(super) read_associated: unsafe extern "C" fn(Osr, *const c_char, *mut u32),
}

pub(super) static API: OnceLock<std::result::Result<Arc<Api>, String>> = OnceLock::new();

fn candidates() -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::env::var_os("QUPATH_RS_OPENSLIDE").map(PathBuf::from).into_iter().collect();
    for p in [
        "/opt/homebrew/lib/libopenslide.1.dylib",
        "/usr/local/lib/libopenslide.1.dylib",
        "/usr/lib/x86_64-linux-gnu/libopenslide.so.1",
        "/usr/lib/libopenslide.so.1",
        "libopenslide-1.dll",
    ] {
        v.push(PathBuf::from(p));
    }
    if let Some(p) = extract_from_qupath() {
        v.push(p);
    }
    v
}

/// Copies `libopenslide` out of an installed QuPath's jar into the cache, once.
fn extract_from_qupath() -> Option<PathBuf> {
    let cache = directories::ProjectDirs::from("org", "qupath-rs", "qupath-rs")?.cache_dir().join("openslide");
    let target = cache.join(if cfg!(target_os = "macos") { "libopenslide.dylib" } else { "libopenslide.so" });
    if target.exists() {
        return Some(target);
    }
    let apps = std::fs::read_dir("/Applications").ok()?;
    for app in apps.flatten().map(|e| e.path()).filter(|p| p.to_string_lossy().contains("QuPath")) {
        let dir = app.join("Contents/app");
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for jar in
            entries.flatten().map(|e| e.path()).filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().starts_with("openslide-")))
        {
            if unzip_dylib(&jar, &target).is_ok() {
                log::info!("Using OpenSlide from {}", jar.display());
                return Some(target);
            }
        }
    }
    None
}

fn unzip_dylib(jar: &Path, target: &Path) -> Result<()> {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(jar)?)?;
    for i in 0..zip.len() {
        let mut f = zip.by_index(i)?;
        let name = f.name()?.into_owned();
        if name.ends_with("libopenslide.dylib") || name.ends_with("libopenslide.so") {
            std::fs::create_dir_all(target.parent().context("no parent")?)?;
            let mut bytes = Vec::new();
            std::io::Read::read_to_end(&mut f, &mut bytes)?;
            fastframe_file::write(target, bytes)?;
            return Ok(());
        }
    }
    bail!("no library in {}", jar.display())
}

pub(super) fn load() -> std::result::Result<Arc<Api>, String> {
    let mut errors = Vec::new();
    for path in candidates() {
        // SAFETY: loading a C library that only runs its own initialisers.
        match unsafe { Library::new(&path) } {
            Ok(lib) => match bind(lib) {
                Ok(api) => return Ok(Arc::new(api)),
                Err(e) => errors.push(format!("{}: {e}", path.display())),
            },
            Err(e) => errors.push(format!("{}: {e}", path.display())),
        }
    }
    Err(format!("OpenSlide not found ({})", errors.join("; ")))
}

fn bind(lib: Library) -> Result<Api> {
    // SAFETY: the signatures match openslide.h (OpenSlide 3.4 and 4.x).
    unsafe {
        macro_rules! sym {
            ($n:literal) => {
                *lib.get($n).map_err(|e| anyhow!("{e}"))?
            };
        }
        Ok(Api {
            open: sym!(b"openslide_open\0"),
            close: sym!(b"openslide_close\0"),
            get_error: sym!(b"openslide_get_error\0"),
            level_count: sym!(b"openslide_get_level_count\0"),
            level_dimensions: sym!(b"openslide_get_level_dimensions\0"),
            level_downsample: sym!(b"openslide_get_level_downsample\0"),
            read_region: sym!(b"openslide_read_region\0"),
            property_names: sym!(b"openslide_get_property_names\0"),
            property_value: sym!(b"openslide_get_property_value\0"),
            associated_names: sym!(b"openslide_get_associated_image_names\0"),
            associated_dimensions: sym!(b"openslide_get_associated_image_dimensions\0"),
            read_associated: sym!(b"openslide_read_associated_image\0"),
            _lib: lib,
        })
    }
}

/// Whether OpenSlide could be loaded (and why not). Blocks while it loads.
pub fn availability() -> std::result::Result<(), String> {
    API.get_or_init(load).as_ref().map(|_| ()).map_err(Clone::clone)
}

/// The load result if loading finished; never blocks (for the interface thread).
pub fn status() -> Option<std::result::Result<(), String>> {
    API.get().map(|r| r.as_ref().map(|_| ()).map_err(Clone::clone))
}

/// Finds and loads OpenSlide on a thread of its own (extracting it can write 13 MB).
pub fn preload() {
    let _ = std::thread::Builder::new().name("openslide-loader".into()).spawn(|| {
        let _ = availability();
    });
}
