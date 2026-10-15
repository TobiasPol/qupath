//! Pure-Rust TIFF support: reading SVS / OME-TIFF / tiled TIFF pyramids, writing OME-TIFF.

pub mod decode;
pub mod ifd;
mod layout;
pub mod limits;
pub mod meta;
pub mod server;
pub mod writer;

pub use server::TiffServer;
