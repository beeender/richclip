use std::collections::HashMap;
use std::io::Write;
use crate::Cli;
use crate::clipboard::{ClipBackend, CopyConfig, PasteConfig};
use clipboard_win::{get_clipboard, get_clipboard_string, raw};
use anyhow::{anyhow, Context, Error};
use clipboard_win::raw::{get, get_clipboard_data};
use log::{debug, warn};
use crate::clipboard::mime_type::decide_mime_type;

pub struct WinBackend {}

impl ClipBackend for WinBackend {
    fn copy(&self, config: CopyConfig) -> anyhow::Result<()> {
        todo!()
    }

    fn paste(&self, config: PasteConfig) -> anyhow::Result<()> {
        raw::open().or_else(|err| Err(anyhow!("Error code {err}"))).context("Failed to open clipboard")?;

        let mut writer = config.writer;

        let enum_formats = raw::EnumFormats::new();
        let mut type_list = vec![0u32, 0];
        let mut mime_type_map = HashMap::<String, u32>::new();
        for f in enum_formats {
            if raw::is_format_avail(f) {
                type_list.push(f);
                if let Some(type_str) = raw::format_name_big(f) {
                    mime_type_map.insert(type_str, f);
                } else {
                    warn!("Unknown format type: {}", f);
                }
            }
        }

        if config.list_types_only {
            for (type_str, _) in mime_type_map {
                writeln!(&mut writer, "{type_str}").context("Failed to write to the output")?;
            }
            return Ok(());
        }

        let mime_types = mime_type_map.clone().into_iter().map(|(k, v)| k).collect();
        let type_str = decide_mime_type(&config.expected_mime_type, &mime_types)?;
        let type_id = mime_type_map.get(&type_str).context("Failed to get the mime type id")?;
        let mut buf = vec![0u8, 0];
        // FIXME: handle error
        let data = raw::get_vec(*type_id, &mut buf).ok().unwrap();
        writer.write_all(buf.as_slice())?;

        raw::close().or_else(|err| Err(anyhow!("Error code {err}"))).context("Failed to close clipboard")?;
        Ok(())
    }
}