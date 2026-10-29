//! Headless sessions for AI agents: one open image (optionally a project entry)
//! changed through the tools in `tools`, which `crate::mcp` serves over MCP.
//!
//! Coordinates are full-resolution pixels, as in QuPath scripts. Objects are named
//! by their UUID (`PathObject::id`), which GeoJSON keeps, so ids stay valid across
//! saves and match what the GUI and QuPath see.

pub mod args;
pub mod render;
pub mod session;
pub mod tools;

pub use session::{OpenImage, Session};
