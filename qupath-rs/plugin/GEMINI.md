# qupath-rs

The `qupath` MCP server analyzes whole-slide and microscopy images with qupath-rs (QuPath's core in Rust): projects, annotations, tissue detection, (positive) cell detection, measurements and export to QuPath.

Before using its tools, read `skills/qupath-analysis/SKILL.md` in this extension. Its key rules:

- Coordinates are full-resolution pixels; objects are referred to by UUID `id`.
- Work in a project (`create_project`, `add_images`, `open_project_image`) so edits are saved.
- Call `view_image` before choosing regions and again after detection to check results.
