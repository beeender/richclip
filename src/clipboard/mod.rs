mod mime_type;
mod wayland;
mod x;

use super::source_data::SourceData;
use std::io::Write;
use std::os::fd::AsFd;

pub struct PasteConfig<'a, T: AsFd + Write> {
    // Only list mime-types
    pub list_types_only: bool,
    pub use_primary: bool,
    pub expected_mime_type: String,
    pub fd_to_write: &'a mut T,
}

pub struct CopyConfig<T: SourceData> {
    pub use_primary: bool,
    pub source_data: T,
}

pub use wayland::{copy_wayland, paste_wayland};
pub use x::{copy_x, paste_x};
