# Setting up the qupath MCP server

The tools come from the `qupath-rs` binary run as `qupath-rs mcp`, which speaks MCP over stdio. The plugin's launcher, `scripts/qupath-rs-mcp`, looks for the binary in this order:

1. `$QUPATH_RS_BIN`
2. `qupath-rs` on `PATH`
3. `/Applications/QuPath-rs.app` or `~/Applications/QuPath-rs.app` (macOS)
4. `target/release` of the qupath-rs checkout that holds the plugin

If the tools are missing, ask the user to install the binary in one of these ways:

- Download a release from <https://github.com/TobiasPol/qupath/releases> (tags `qupath-rs-v*`).
- Build it: `cd qupath-rs && cargo install --path . --bin qupath-rs`. This needs Rust 1.98 or later.

Then restart the agent session. Vendor formats (NDPI, MRXS, SCN…) also need OpenSlide (`brew install openslide`), or set `QUPATH_RS_OPENSLIDE`.

To check the server by hand, this should print one JSON line listing the tools:

```bash
printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"tools/list"}' | qupath-rs mcp
```
