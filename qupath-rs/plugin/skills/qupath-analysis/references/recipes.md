# Recipes and tuning

## Cell detection parameters (`detect_cells`)

| Parameter | Default | Change it when… |
| --- | --- | --- |
| `requested_pixel_size` | 0.5 µm | Lower it (0.25) for small, crowded nuclei; raise it (1.0) for speed on large areas. |
| `background_radius` | 8 µm | Uneven background. Set 0 to turn it off when nuclei are large or touch one another. |
| `sigma` | 1.5 µm | Nuclei split into pieces: raise to 2–2.5. Touching nuclei merge: lower to 1. |
| `min_area` / `max_area` | 10 / 400 µm² | Debris counted as cells: raise `min_area`. Large tumour nuclei missed: raise `max_area`. |
| `threshold` | 0.1 | Faint nuclei missed: lower to 0.05. Background detected as nuclei: raise to 0.15–0.3. |
| `cell_expansion` | 5 µm | Cytoplasm-wide measurements need a bigger expansion; use 0 for nuclei only. |
| `detection_image` | `hematoxylin` | `od_sum` for odd stains. Fluorescence: `channel:N`, 0-based nuclear channel such as DAPI. |

## Positive cells (IHC)

- Single threshold (Positive/Negative): `positive: true, positive_thresholds: [0.2]` on `Nucleus: DAB OD mean`. This is QuPath's default, and right for Ki-67, ER and PR.
- H-score style 1+/2+/3+: `positive_thresholds: [0.2, 0.4, 0.6]`.
- Membrane or cytoplasm markers (HER2, PD-L1): `positive_measurement: "Cell: DAB OD mean"` or `"Cytoplasm: DAB OD mean"`.
- To choose a threshold, call `measurements {kind: "cell", summary: true, columns: ["Nucleus: DAB OD mean"]}`, then `view_image` on cells either side of the candidate value. `region_stats` on a clearly positive and a clearly negative area also gives the DAB OD range.

## Tissue detection (`threshold_regions`)

- Brightfield: `channel: "average"`, `above: false`, `threshold` about 5–10 below the glass brightness from `region_stats` (glass is usually 235–250). Use `downsample: 16–32`.
- Faint tissue: `channel: "od_sum"`, `above: true`, `threshold: 0.1–0.2`.
- Fluorescence: `channel: "channel:0"`, `above: true`.
- To drop dust: raise `min_object_area` (µm²). To fill holes: raise `min_hole_area`. Use `split: true` for one annotation per tissue piece, such as TMA cores or several sections.

## Fluorescence

Call `set_image_type {type: "FLUORESCENCE"}`, then `detect_cells {detection_image: "channel:0", threshold: <intensity>}`. The threshold is in raw intensity units, so read `region_stats` first: a value between background and nuclei, often 2–3× the background mean, works well.

## Common problems

- **"set a brightfield image type"**: call `set_image_type` with H&E or H-DAB.
- **"no area annotation to work in"**: add one with `add_annotation` (`full_image`) or `threshold_regions`.
- **Very slow detection or huge output**: you probably passed a full-image annotation on a big slide. Detect tissue first, or test on a rectangle.
- **The `view_image` overlay leaves out detections**: more than 30,000 are in view. Zoom into a smaller region.
- **The user wants results in QuPath**: the project folder opens directly in QuPath 0.7 (objects are in `data/<id>/objects.geojson`; use File › Import objects in QuPath). `export_script` gives the matching Groovy.
