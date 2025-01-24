mod mime_type;
#[cfg(target_os = "linux")]
mod wayland;
mod x;

use super::protocol::SourceData;
use anyhow::{bail, Result};
use std::io::Write;

pub trait ClipBackend {
    fn copy(&self, config: CopyConfig) -> Result<()>;
    fn paste(&self, config: PasteConfig) -> Result<()>;
}

pub struct PasteConfig {
    // Only list mime-types
    pub list_types_only: bool,
    pub use_primary: bool,
    pub expected_mime_type: String,
    pub writter: Box<dyn Write>,
}

pub struct CopyConfig {
    pub use_primary: bool,
    pub source_data: Box<dyn SourceData>,
    // For testing X INCR mode
    pub x_chunk_size: usize,
}

pub use x::XBackend;

#[cfg(target_os = "linux")]
pub use wayland::WaylandBackend;

#[cfg(target_os = "linux")]
pub fn create_backend() -> Result<Box<dyn ClipBackend>> {
    if std::env::var("WAYLAND_DISPLAY").is_ok() {
        return Ok(Box::new(WaylandBackend {}));
    } else if std::env::var("DISPLAY").is_ok() {
        return Ok(Box::new(XBackend {}));
    }

    bail!("Could not decide the clip backend");
}

#[cfg(target_os = "macos")]
pub fn create_backend() -> Result<Box<dyn ClipBackend>> {
    if std::env::var("DISPLAY").is_ok() {
        return Ok(Box::new(XBackend {}));
    }

    bail!("Could not decide the clip backend");
}
