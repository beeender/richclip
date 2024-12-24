mod wayland;
mod mime_type;

use std::os::fd::AsFd;
use std::io::Write;
pub struct PasteConfig<'a, T: AsFd + Write> {
    // Only list mime-types
    pub list_types_only: bool,
    pub use_primary: bool,
    pub expected_mime_type: String,
    pub fd_to_write: &'a mut T
}

pub use wayland::{copy_wayland, paste_wayland};
