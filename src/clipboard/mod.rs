#[cfg(target_os = "macos")]
mod mac;
mod mime_type;
#[cfg(target_os = "linux")]
mod wayland;
#[cfg(target_os = "linux")]
mod x;
mod win;

use super::protocol::SourceData;
use anyhow::Result;
#[cfg(target_os = "linux")]
use anyhow::bail;
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
    pub writer: Box<dyn Write>,
}

pub struct CopyConfig {
    pub use_primary: bool,
    pub source_data: Box<dyn SourceData>,
    // For testing X INCR mode
    pub x_chunk_size: usize,
}

#[cfg(target_os = "macos")]
use mac::MacBackend;

#[cfg(target_os = "linux")]
pub use wayland::WaylandBackend;
#[cfg(target_os = "linux")]
pub use x::XBackend;
use crate::clipboard::win::WinBackend;

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
    // NOTE: X clipboard can be supported on Mac if Mac has Xserver installed like XQuartz.
    //       However it doesn't make too much sense since XQuartz should be above to read/write the
    //       cocoa pasteboard.
    // if std::env::var("DISPLAY").is_ok() {
    //     return Ok(Box::new(XBackend {}));
    // }

    Ok(Box::new(MacBackend {}))
}

#[cfg(target_os = "windows")]
pub fn create_backend() -> Result<Box<dyn ClipBackend>> {
    Ok(Box::new(WinBackend {}))
}
