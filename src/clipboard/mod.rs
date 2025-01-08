mod mime_type;
mod wayland;
mod x;

use super::protocol::SourceData;
use std::io::Write;

pub struct PasteConfig<'a, T: Write> {
    // Only list mime-types
    pub list_types_only: bool,
    pub use_primary: bool,
    pub expected_mime_type: String,
    pub writter: &'a mut T,
}

pub struct CopyConfig<T: SourceData> {
    pub use_primary: bool,
    pub source_data: T,
    // For testing X INCR mode
    pub x_chunk_size: usize,
}

pub use wayland::{copy_wayland, paste_wayland};
pub use x::{copy_x, paste_x};
