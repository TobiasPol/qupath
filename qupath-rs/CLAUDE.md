# qupath-rs

## Overview
Lives in `qupath-rs/` of a personal fork of qupath/qupath; run all commands from this directory. Never open pull requests against qupath/qupath: its AI_POLICY.md does not accept AI-generated code.

A Rust/egui reimplementation of QuPath's core: a whole-slide image viewer with
annotation tools, object hierarchy, cell detection, measurements and QuPath
project/GeoJSON compatibility. fastframe supplies fonts, text rendering, icons,
palettes, logging and atomic saves.

## Tech Stack
- Language: Rust (edition 2024, 1.98+)
- GUI: egui/eframe 0.36.1, patched to crmne's egui/winit forks as fastframe requires (see `Cargo.toml` `[patch.crates-io]`)
- Platform layer: fastframe v0.6.0 (fonts, text, icons, theme, log, file)
- Geometry: geo (boolean ops, buffer, earcut), rstar (spatial index)
- Images: own TIFF reader/writer, jpeg-decoder, image, imageproc; OpenSlide via libloading
- Database: none (projects are JSON files in QuPath's format)

## Commands
- `cargo run --release -- [image|project.qpproj|objects.geojson]`: start the app
- `cargo build --release`: build
- `cargo test --release`: run the tests
- `cargo clippy --all-targets -- -D warnings`: lint
- `cargo fmt`: format (`rustfmt.toml`: max width 140)
- `scripts/check-sizes.sh`: no file over 300 lines, no function over 50 lines
- `cargo run --release --bin make-test-slide -- out.ome.tif`: synthetic H-DAB slide
- `scripts/bundle-macos.sh`: `dist/QuPath-rs.app`

## Architecture
- `src/objects/`: ROIs, classes (global colours like QuPath's `PathClass`), objects, hierarchy (slotmap arena, containment insert/resolve), selection, R-tree index, measurements, geometry ops
- `src/image/`: `ImageServer` trait, region reading, TIFF reader/writer (`tiff/`), OpenSlide FFI (`openslide/`), in-memory images, stains (colour deconvolution) and display mapping
- `src/io/`: QuPath GeoJSON, `.qpproj` projects, per-image data (`objects.geojson` + `qupath-rs.json`), preferences
- `src/analysis/`: cell detection (`cells/`), morphology, contours, tiles, thresholder
- `src/viewer/`: transform, tile manager (worker threads + LRU + textures), rendering, overlays, decorations, tools (`tools/`)
- `src/app/`: app state, command table (menus, shortcuts and command list all derive from it), dispatch, files, objects, dialogs, export, tasks, automation
- `src/ui/`: style (fastframe theme/fonts), icons, analysis pane tabs, tool windows, dialogs
- `docs/ARCHITECTURE.md`: details and the QuPath parity table

Conventions:
- Long work runs in `Tasks` (background threads with progress); the UI thread never blocks on disk. Use `AppState::writer` (fastframe `Writer`) for saves from the UI thread; `fastframe_file::write` panics there in debug builds.
- Every hierarchy change takes an undo checkpoint and records a workflow step with the equivalent QuPath script line.
- Keep measurement names identical to QuPath's (`Nucleus: Area µm^2`, `Num Positive`, …).
- Verify UI changes with `QUPATH_RS_AUTOMATION` screenshots (see `src/app/automation.rs`).

## Environment Variables
- `RUST_LOG`: log filter
- `QUPATH_RS_OPENSLIDE`: path to libopenslide
- `QUPATH_RS_AUTOMATION`: scripted UI steps for screenshots
