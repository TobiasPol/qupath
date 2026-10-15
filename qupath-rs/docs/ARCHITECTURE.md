# Architecture and QuPath parity

qupath-rs is based on QuPath 0.7.0's source (tag `v0.7.0`, github.com/qupath/qupath)
and on the shipped `QuPath-0.7.0-arm64.app`. File formats, labels, shortcuts,
default classes, stain vectors and measurement names were taken from that
source. Compatibility is verified against the installed QuPath (see *Verification*).

## How the pieces map to QuPath

| qupath-rs | QuPath (Java) |
| --- | --- |
| `objects::roi::Roi` | `qupath.lib.roi.*` (`RectangleROI`, `EllipseROI`, `LineROI`, `PolygonROI`, `PolylineROI`, `PointsROI`, `GeometryROI`) |
| `objects::classes::PathClass` (global colour registry, `Tumor: Positive` derivation, colour rules) | `PathClass`, `PathClass.StandardPathClasses` |
| `objects::object::PathObject`, `ObjectKind` | `PathAnnotationObject`, `PathDetectionObject`, `PathCellObject`, `PathTileObject`, `TMACoreObject`, `PathRootObject` |
| `objects::hierarchy::Hierarchy` (insert by containment, `resolve`) | `PathObjectHierarchy` (`insertPathObject`, `resolveHierarchy`) |
| `objects::measurements` (`Area µm^2`, `Num Detections`, `Positive %`) | `ObservableMeasurementTableData` |
| `objects::ops` | `RoiTools`, `Commands.mergeSelectedAnnotations`, `makeInverseAnnotation`, `expandAnnotations` |
| `io::geojson` | `QuPathTypeAdapters`, `GsonTools.PathClassTypeAdapter`, `ROITypeAdapters` |
| `io::project` | `DefaultProject`, `DefaultProjectImageEntry`, `ImageServers.DefaultImageServerBuilder` |
| `image::server::ImageServer`, `region::RegionRequest` | `ImageServer`, `RegionRequest`, `ServerTools.getPreferredResolutionLevel` |
| `image::tiff` | Bio-Formats' TIFF/OME-TIFF reading and `OMEPyramidWriter` (simplified) |
| `image::openslide` | `qupath-extension-openslide` (`OpenslideImageServer`) |
| `image::stains` | `ColorDeconvolutionStains`, `StainVector` (defaults, residual = cross product), `ColorDeconvolutionHelper.makeOD` |
| `image::display` | `ImageDisplay`, `ChannelDisplayInfo` |
| `analysis::cells` | `qupath.imagej.detect.cells.WatershedCellDetection`, `PositiveCellDetection` |
| `analysis::tiles` | `TilerPlugin` |
| `analysis::threshold` | "Create thresholder" (`PixelClassifiers` thresholding, simplified) |
| `viewer` | `QuPathViewer`, `HierarchyOverlay`, `ImageOverview`, `Scalebar`, `GridOverlay` |
| `viewer::tools` | `PathTools`, `PathToolEventHandlers`, ROI editor handles |
| `app::commands` + `menu_table` | `*MenuActions` (`@ActionConfig`, `@ActionAccelerator`), `CommandFinderTools` |
| `ui::panes` | `AnalysisTabPane` (Project, Image, Annotations, Hierarchy, Workflow) |

## Threads and data flow

- **Tiles:** `TileManager` sends tile requests to 2 to 8 reader threads. Requests the viewer no longer wants are skipped. Decoded tiles go into a byte-bounded LRU. Textures are rebuilt lazily, a few per frame, when the display (brightness/contrast) version changes. The viewer draws the coarsest level, then any cached intermediate levels, then the target level, so the view is never blank.
- **Analyses** run as `Tasks` on their own threads with progress and cancel, and return `TaskResult::Objects`. Results are applied on the UI thread with an undo checkpoint and a workflow entry.
- **Cell detection** splits each annotation into 1536 px chunks plus padding, processed in parallel with rayon. A cell is kept by the chunk whose core contains its centroid, so no cell is detected twice.
- **Saves** go through `fastframe_file::Writer` (atomic, off the UI thread). Opening images and extracting OpenSlide happen off the UI thread too. fastframe's UI-thread guard caught the first version extracting OpenSlide on the UI thread.

## File formats

- `project.qpproj`: QuPath's JSON (`version`, `createTimestamp`, `uri`, `lastID`, `images[].serverBuilder{builderType:"uri", providerClassName, uri, args}`). URIs use Java's `File.toURI()` escaping.
- `classifiers/classes.json`: `{"pathClasses":[{"name","color"}]}`, with colours as Java packed ARGB ints.
- `data/<id>/objects.geojson`: the hierarchy as a FeatureCollection, with children nested in `properties.childObjects`.
- `data/<id>/qupath-rs.json`: image type, stains, pixel-size overrides and workflow. QuPath's own `data.qpdata` is Java serialization and is not read or written.
- `data/<id>/thumbnail.jpg`: as in QuPath.

## Verification

- `tests/geojson_interop.rs` reads a fixture exported by QuPath 0.7.0 (cells with nuclei, classes, 31 measurements) and round-trips it.
- QuPath 0.7.0, run headless (`QuPath script -p project.qpproj check.groovy`), opened a project, OME-TIFF and GeoJSON written by qupath-rs. Results: 24000×18000 px, 6 levels, 0.25 µm, identical class colours, 734 cells with nucleus ROIs and measurements under the named annotation.
- `tests/cell_detection.rs` checks positive cell detection against the synthetic slide's ground truth (30% positive).
- `QUPATH_RS_AUTOMATION` screenshots catch rendering bugs that unit tests can't. They found egui's unlimited miter joins on closed paths, which turned A→B→A contour hairs into lines across the screen.

## Parity with QuPath

Ported: the viewer, all drawing tools except the arrow variants of the line tool,
the object hierarchy and the operations in the Objects menu, classes, cell
detection and positive cell detection, tiles, a thresholder, brightness/contrast,
measurement tables and export, projects, GeoJSON import/export, OME-TIFF export
and snapshots, undo/redo, the command list, preferences and the log viewer.

Not ported yet, roughly by importance:

1. Groovy scripting and running workflows. Workflow steps are recorded as QuPath script lines for use in QuPath.
2. Object classifiers (random trees/ANN), pixel classifiers beyond thresholds, density maps, spatial analysis (Delaunay clustering, distances).
3. Deep learning (InstanSeg, StarDist, DJL extensions) and the extension manager.
4. TMA dearraying and the TMA data viewer.
5. Bio-Formats breadth (CZI, LIF, ND2, multi-series and z/t stacks). Formats not read natively go through OpenSlide.
6. Multi-view grids, mini/channel viewers, view tracker, viewer rotation.
7. Reading QuPath's Java-serialized `.qpdata`. QuPath data moves over via GeoJSON instead.
8. Stain vector estimation, measurement maps, and the full `ImageServer` transform stack (crops, channel transforms, affine).
