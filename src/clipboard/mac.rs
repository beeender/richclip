#![cfg(target_os = "macos")]

use super::ClipBackend;
use super::CopyConfig;
use super::PasteConfig;
use anyhow::{bail, Context, Result};

use cocoa::appkit;
use cocoa::appkit::NSPasteboard;
use cocoa::base::id;
use cocoa::base::nil;
use cocoa::foundation::NSArray;
use cocoa::foundation::NSData;
use cocoa::foundation::NSString;
use cocoa::foundation::NSAutoreleasePool;

use std::collections::HashMap;
use std::ffi::CStr;
use std::sync::LazyLock;

// cocoa's pasteboard system is strange, just support what is needed for now.
// See https://developer.apple.com/documentation/appkit/nspasteboard/pasteboardtype
static SUPPORTED_TYPES_MAP: LazyLock<HashMap<String, Vec<&str>>> = unsafe {
    LazyLock::new(|| {
        HashMap::from([
            (
                nsstring_to_string(cocoa::appkit::NSPasteboardTypeString),
                vec![
                    "public.utf8-plain-text",
                    "text/plain;charset=utf-8",
                    "text/plain",
                    "text",
                    "string",
                    "utf8_string",
                ],
            ),
            (
                nsstring_to_string(cocoa::appkit::NSPasteboardTypeHTML),
                vec![
                    "public.html",
                    "text/html;charset=utf-8",
                    "text/html",
                    "html",
                ],
            ),
            (
                nsstring_to_string(cocoa::appkit::NSPasteboardTypeRTF),
                vec!["public.rtf", "application/rtf", "rtf"],
            ),
        ])
    })
};

pub struct MacBackend {}

impl ClipBackend for MacBackend {
    fn copy(&self, config: CopyConfig) -> Result<()> {
        unsafe { copy_mac(config) }
    }

    fn paste(&self, config: PasteConfig) -> Result<()> {
        unsafe { paste_mac(config) }
    }
}

unsafe fn copy_mac(config: CopyConfig) -> Result<()> {
    let _pool = NSAutoreleasePool::new(nil);

    let pb = NSPasteboard::generalPasteboard(nil);
    let types = config.source_data.mime_types();

    pb.clearContents();

    for t in &types {
        let ns_pb_type = match_ns_pasteboard_type(t);
        if ns_pb_type.is_empty() {
            bail!("Failed to copy content of type {t}")
        }
        let res = config.source_data.content_by_mime_type(t);
        if !res.0 {
            log::warn!("No content found fro {t}");
            continue;
        }
        let nstr_type = NSString::alloc(nil).init_str(ns_pb_type.as_str());
        let bytes = res.1.as_ptr() as *const std::os::raw::c_void;
        let length = res.1.len() as u64;
        let nsdata = NSData::dataWithBytesNoCopy_length_(nil, bytes, length);
        let r = pb.setData_forType(nsdata, nstr_type);
        if r != 1 {
            log::error!("Failed to call setData_forType on {t}");
        }
    }

    Ok(())
}

unsafe fn paste_mac(config: PasteConfig) -> Result<()> {
    let _pool = NSAutoreleasePool::new(nil);

    let mut writer = config.writter;
    let mut type_list: Vec<String> = vec![];

    let pb = NSPasteboard::generalPasteboard(nil);
    let types = pb.types();
    let count = types.count();

    for i in 0..count {
        let t = types.objectAtIndex(i);
        let str = nsstring_to_string(t);
        if SUPPORTED_TYPES_MAP.contains_key(&str) {
            type_list.push(str);
        }
    }

    if config.list_types_only {
        for str in type_list {
            writeln!(&mut writer, "{}", str).context("Failed to write to the output")?;
        }
        return Ok(());
    }

    let expected_type = match_ns_pasteboard_type(&config.expected_mime_type);
    if expected_type.is_empty() {
        bail!(
            "Content for mime-type {} doesn't exist",
            config.expected_mime_type
        )
    }

    let nstr_type = NSString::alloc(nil).init_str(expected_type.as_str());
    let data = pb.dataForType(nstr_type);
    let bytes = data.bytes() as *const u8;
    let length = data.length() as usize;
    let slice = std::slice::from_raw_parts(bytes, length);
    writer.write_all(slice)?;
    writer.flush()?;

    Ok(())
}

unsafe fn nsstring_to_string(ns_str: id) -> String {
    let c_str: *const i8 = NSString::UTF8String(ns_str);

    if c_str.is_null() {
        panic!("Empty or null NSString content");
    }

    CStr::from_ptr(c_str)
        .to_str()
        .expect("Invalid UTF-8 string")
        .to_string()
}

unsafe fn match_ns_pasteboard_type(mime_type: &str) -> String {
    if !mime_type.is_empty() {
        let target = mime_type.to_lowercase();
        if let Some((key, _)) = SUPPORTED_TYPES_MAP
            .iter()
            .find(|(_, types)| types.iter().any(|s| s.to_lowercase().contains(&target)))
        {
            key.clone()
        } else {
            "".to_string()
        }
    } else {
        nsstring_to_string(appkit::NSPasteboardTypeString)
    }
}
