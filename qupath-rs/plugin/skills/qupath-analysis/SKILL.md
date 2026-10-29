---
name: qupath-analysis
description: Analyze whole-slide and microscopy images (SVS, OME-TIFF, NDPI, MRXS, TIFF, PNG) with qupath-rs, QuPath's core rebuilt in Rust, through the `qupath` MCP tools. Use it for digital pathology and bioimage analysis tasks such as detecting tissue, cell detection, IHC positive cell scoring (DAB, Ki-67, ER/PR), H&E nucleus counting, annotating regions, measuring areas, and exporting objects, measurements or scripts to QuPath. Also use it when the user mentions QuPath, .qpproj projects, GeoJSON annotations or slide images.
license: GPL-3.0-or-later
---

# Whole-slide analysis with qupath-rs

The `qupath` MCP server (from `qupath-rs mcp`) keeps one image open at a time, optionally inside a QuPath project. If its tools are missing, read [references/setup.md](references/setup.md).

## Ground rules

- **Coordinates** are full-resolution pixels, origin top-left. Distances and areas in tool parameters are µm/µm² when the image has a pixel size (`image_info` → `pixel_size_um`).
- **Objects** are referred to by `id` (a UUID). Get ids from `list_objects`, or from the result of the tool that created the object.
- **Work in a project** (`create_project` + `add_images`, or `open_project`). Every edit to a project image is saved straight away, and the user can open the same project in QuPath 0.7 or the qupath-rs GUI. For an image outside a project, edits stay in memory until `export_geojson`.
- **Look before and after.** Call `view_image` (the whole slide by default) before you choose regions, and again on a small region (about 500–1000 px) after detection to check the result. If a detection is wrong, adjust its parameters and rerun it: `detect_cells` replaces the detections inside the parent annotation.
- Tool errors explain what to do next (for example, "no image is open: call open_image"). Follow them rather than retrying unchanged.

## Standard workflow

1. **Open.** Use `create_project {dir}`, then `add_images {paths}`, then `open_project_image {id}`. For a quick look at a single file, `open_image {path}` is enough.
2. **Check the image.** Call `image_info`. Fix the image type when it is wrong: `set_image_type`, using `BRIGHTFIELD_H_DAB` for IHC, `BRIGHTFIELD_H_E` for H&E, `FLUORESCENCE` for fluorescence. If `pixel_size_um` is null, ask the user for the pixel size and call `set_pixel_size`, or work in pixels.
3. **Define where to analyze.**
   - Tissue: `region_stats {downsample: 32}` shows how bright the background is. Then `threshold_regions {channel: "average", threshold: <between tissue and glass, ~210–230>, above: false}` gives a `Region*` annotation.
   - A region the user describes: `add_annotation` with a rectangle, ellipse or polygon. Use `{type: "full_image"}` for the whole slide.
4. **Analyze.**
   - IHC: `detect_cells {parent_ids, positive: true}`.
   - H&E or haematoxylin only: `detect_cells {parent_ids}`.
   - Grids for sampling: `create_tiles`.
   - On large tissue areas, test the parameters on a small rectangle first. Detection runs at roughly 5–20k cells/s, so a whole slide can take a minute.
5. **Verify.** Call `view_image` on a few regions with `overlay: true`. Positive cells are drawn red, Negative blue, 1+/2+/3+ yellow, orange and red. Look for missed nuclei, merged or split cells, and false positives in stroma or background.
6. **Report.**
   - Per region: `measurements {kind: "annotation", columns: ["Name", "Num Detections", "Num Positive", "Positive %", "Area µm^2"]}`.
   - Per-cell distributions: `measurements {kind: "cell", summary: true}`.
   - Files: `export_measurements {path}` (CSV/TSV), `export_geojson {path}`, and `export_script {path}`, which writes a Groovy script that reruns the analysis in QuPath.

Parameter tuning (nucleus size, thresholds, fluorescence, H&E), common problems and fixes are in [references/recipes.md](references/recipes.md).

## Reporting results

Give numbers with their denominators and parameters, for example: "1,426 cells in ROI 1 (0.25 mm²); 443 positive (31.1%) at Nucleus: DAB OD mean ≥ 0.2." Say what you checked visually. Note that qupath-rs ports QuPath's algorithms: results match QuPath within about 0.5% but are not identical, and they are not a clinical diagnosis.
