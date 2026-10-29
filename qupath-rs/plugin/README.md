# qupath-rs plugin for AI coding agents

This plugin lets Claude Code, Codex, Cursor, Gemini CLI and other MCP clients analyze whole-slide images with [qupath-rs](../README.md). The agent can:

- open slides and QuPath projects, and look at regions as images;
- annotate regions, detect tissue, and detect and classify cells;
- read measurements, and export GeoJSON, CSV and QuPath scripts.

Everything the agent does is saved as a normal QuPath project, so you can check its work in QuPath 0.7 or the qupath-rs GUI.

It has two parts:

- **An MCP server**: `qupath-rs mcp`, which is the `qupath-rs` binary run in stdio mode, started by `scripts/qupath-rs-mcp`.
- **A skill**: [`skills/qupath-analysis`](skills/qupath-analysis/SKILL.md), which teaches the agent the workflow, the parameters and the checks. It follows the [Agent Skills](https://agentskills.io) format, which every harness below reads.

## 1. Install the qupath-rs binary

Choose one:

- Download a release from <https://github.com/TobiasPol/qupath/releases> (tags `qupath-rs-v*`). Put `qupath-rs` on your `PATH`; on macOS, move `QuPath-rs.app` to `/Applications`.
- Build it from this repository:

  ```bash
  cargo install --path qupath-rs --bin qupath-rs
  ```

The launcher looks for the binary in this order:

1. `$QUPATH_RS_BIN`
2. `PATH`
3. `~/.cargo/bin`, `/opt/homebrew/bin`, `/usr/local/bin`, `~/.local/bin`
4. `QuPath-rs.app`
5. the checkout's `target/release`

Vendor formats (NDPI, MRXS…) also need OpenSlide (`brew install openslide`).

## 2. Add the plugin to your agent

**Claude Code**

```bash
claude plugin marketplace add TobiasPol/qupath
```

```bash
claude plugin install qupath-rs@qupath-rs
```

You can run the same two steps inside a session with `/plugin marketplace add TobiasPol/qupath` and `/plugin install qupath-rs@qupath-rs`. To try a local checkout without installing: `claude --plugin-dir qupath-rs/plugin`.

**Codex** reads [`.agents/plugins/marketplace.json`](../../.agents/plugins/marketplace.json) at the repository root.

```bash
codex plugin marketplace add TobiasPol/qupath
```

```bash
codex plugin add qupath-rs@qupath-rs
```

**Cursor**: copy the plugin into Cursor's local plugin folder, then restart Cursor.

```bash
mkdir -p ~/.cursor/plugins/local && cp -R qupath-rs/plugin ~/.cursor/plugins/local/qupath-rs
```

**Gemini CLI**: link the extension from a checkout.

```bash
gemini extensions link qupath-rs/plugin
```

**Any other MCP client** (VS Code, Windsurf, Goose, OpenCode, Claude Desktop…): add a stdio server that runs `qupath-rs` with the argument `mcp`. For example:

```json
{ "mcpServers": { "qupath": { "command": "qupath-rs", "args": ["mcp"] } } }
```

VS Code's `.vscode/mcp.json` uses the key `servers` instead of `mcpServers`. To get the workflow guidance in those clients too, copy `skills/qupath-analysis` to `~/.agents/skills/`.

## 3. Use it

Ask in plain words, for example:

> Create a project in ~/analysis/ki67, add the slides in ~/slides, detect the tissue on each one and quantify Ki-67 positive cells, then give me a table of positive % per slide.

To check what the agent did, open the project in qupath-rs or QuPath. If a project image is already open in the GUI while the agent edits it, use File › Reload data to see the changes.

## Tools

| Area | Tools |
| --- | --- |
| Projects | `create_project`, `open_project`, `add_images`, `list_project_images`, `open_project_image`, `save` |
| Images | `open_image`, `image_info`, `set_image_type`, `set_pixel_size`, `view_image` (PNG with object overlay), `region_stats` |
| Objects | `add_annotation`, `list_objects`, `delete_objects`, `set_object_properties`, `annotation_operation`, `import_geojson`, `export_geojson` |
| Analysis | `detect_cells` (QuPath's watershed and positive cell detection), `create_tiles`, `threshold_regions` (tissue detection) |
| Results | `measurements` (QuPath's column names; rows or a summary), `export_measurements`, `export_script` (QuPath Groovy) |

Notes on the tools:

- Coordinates are full-resolution pixels.
- Objects are referred to by their QuPath UUID.
- Edits to project images are saved straight away.

The server speaks MCP `2026-07-28` (`server/discover`), and the `initialize` handshake of `2025-11-25` and earlier versions.

## Layout

| File | For |
| --- | --- |
| `.claude-plugin/plugin.json`, `.mcp.json` | Claude Code (`${CLAUDE_PLUGIN_ROOT}`) |
| `plugin.json`, `mcp.json` | [Agent Plugins](https://agent-plugins.org) 1.0: Codex and others (`${PLUGIN_ROOT}`) |
| `.cursor-plugin/plugin.json`, `.cursor-plugin/mcp.json` | Cursor (`${CURSOR_PLUGIN_ROOT}`) |
| `gemini-extension.json`, `GEMINI.md` | Gemini CLI (`${extensionPath}`) |
| `scripts/qupath-rs-mcp`, `scripts/qupath-rs-mcp.cmd` | Finds the binary and starts `qupath-rs mcp` |
| `skills/qupath-analysis/` | Workflow skill shared by all harnesses |

The marketplaces live at the repository root: [`.claude-plugin/marketplace.json`](../../.claude-plugin/marketplace.json) for Claude Code and [`.agents/plugins/marketplace.json`](../../.agents/plugins/marketplace.json) for Codex.

Results are produced by a reimplementation of QuPath's algorithms. They are for research use, not diagnosis.
