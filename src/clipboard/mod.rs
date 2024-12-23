mod wayland;
mod mime_type;

use std::os::fd::AsFd;
pub struct PasteConfig<'a> {
    // Only list mime-types
    pub list_types_only: bool,
    pub expected_mime_type: String,
    pub fd_to_write: &'a dyn AsFd
}

pub use wayland::{copy_wayland, paste_wayland};
