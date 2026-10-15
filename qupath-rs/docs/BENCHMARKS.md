# qupath-rs vs. QuPath 0.7.0

Same machine, same image, same parameters. Medians of three runs.

- **Machine:** Apple M1 Pro (8 cores), 16 GB, macOS 27.0.1
- **Image:** `make-test-slide`, 24000×18000 px pyramidal OME-TIFF (JPEG tiles, 0.25 µm/px, H-DAB)
- **QuPath 0.7.0:** run headless via `QuPath script -i IMAGE scripts/bench/qupath-bench.groovy`. Timings are taken inside the JVM, so JVM startup is excluded from each task.
- **qupath-rs:** run with `cargo run --release --example bench -- IMAGE OUT.geojson`

| Task | QuPath 0.7.0 | qupath-rs | Faster |
| --- | ---: | ---: | ---: |
| Open the image (read metadata) | 75 ms | 3 ms | ~24× |
| Read 200 full-resolution 512² tiles (cold, sequential) | 1,729 ms | 576 ms | 3.0× |
| Positive cell detection, 2000² px (1.4k cells), first run | 2,237 ms | 254 ms | 8.8× |
| … same, second run (JIT warm) | 869 ms | 228 ms | 3.8× |
| Positive cell detection, 6000² px (8k cells), first run | 6,701 ms | 561 ms | 11.9× |
| … same, second run | 5,914 ms | 520 ms | 11.4× |
| Export 8k cells as GeoJSON | 1,139 ms | 159 ms | 7.2× |
| Import them again | 1,178 ms | 227 ms | 5.2× |
| Whole run, including process startup | 24.0 s | 2.8 s | 8.7× |
| Peak resident memory | 1.49 GB | 0.93 GB | 1.6× less |
| Application size | 261 MB | 15 MB | 17× smaller |

**Results agree.** QuPath found 1,433 cells (444 positive) and 8,027 cells (2,343 positive). qupath-rs found 1,426 (443) and 8,000 (2,340), all within 0.5%.

## Caveats

- **Measurement count:** QuPath computes 41 measurements per cell and qupath-rs computes 31, so QuPath does somewhat more work per cell.
- **Memory:** the JVM's resident memory includes heap it reserved but may not need. qupath-rs's peak comes mostly from holding the GeoJSON document in memory during import and export.
- **Not compared:** viewer rendering speed and GUI startup, because these need QuPath's GUI driven interactively. qupath-rs opens its window and draws the slide about 1.1 s after launch.
- **Cold vs. warm:** first runs include the JVM's JIT warm-up. In an interactive session QuPath gets faster on repeated commands, while qupath-rs is equally fast from the first call.
