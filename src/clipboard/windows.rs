use super::mime_type::decide_mime_type;
use super::{ClipBackend, CopyConfig, PasteConfig};
use anyhow::{Context, Result, anyhow};
use clipboard_win::formats::{self, Html};
use clipboard_win::options::NoClear;
use clipboard_win::raw;
use clipboard_win::{Clipboard, Getter};
use std::collections::HashSet;
use std::io::Write;

pub struct WindowsBackend {}

impl ClipBackend for WindowsBackend {
    fn copy(&self, config: CopyConfig) -> Result<()> {
        copy_windows(config)
    }

    fn paste(&self, config: PasteConfig) -> Result<()> {
        paste_windows(config)
    }
}

fn copy_windows(config: CopyConfig) -> Result<()> {
    let _clipboard = open_clipboard()?;
    raw::empty().map_err(|err| anyhow!("Failed to empty the clipboard: {err}"))?;

    let mut written_formats = HashSet::new();
    for mime_type in config.source_data.mime_types() {
        let (found, content) = config.source_data.content_by_mime_type(&mime_type);
        if !found {
            continue;
        }

        let format = format_for_mime_type(&mime_type)?;
        if !written_formats.insert(format) {
            continue;
        }

        if format == formats::CF_UNICODETEXT {
            let text = std::str::from_utf8(&content)
                .with_context(|| format!("Clipboard text for '{mime_type}' is not valid UTF-8"))?;
            raw::set_string_with::<NoClear>(text, NoClear)
                .map_err(|err| anyhow!("Failed to set Windows Unicode text: {err}"))?;
        } else if is_html_type(&mime_type) {
            let html = std::str::from_utf8(&content)
                .with_context(|| format!("Clipboard HTML for '{mime_type}' is not valid UTF-8"))?;
            raw::set_html(format, html)
                .map_err(|err| anyhow!("Failed to set Windows HTML: {err}"))?;
        } else {
            raw::set_without_clear(format, &content)
                .map_err(|err| anyhow!("Failed to set clipboard format '{mime_type}': {err}"))?;
        }
    }

    Ok(())
}

fn paste_windows(mut config: PasteConfig) -> Result<()> {
    let _clipboard = open_clipboard()?;
    let formats = available_formats();

    if config.list_types_only {
        for (mime_type, _) in formats {
            writeln!(config.writer, "{mime_type}")?;
        }
        return Ok(());
    }

    let supported: Vec<String> = formats
        .iter()
        .map(|(mime_type, _)| mime_type.clone())
        .collect();
    let selected = decide_mime_type(&config.expected_mime_type, &supported).with_context(|| {
        format!(
            "Content for mime-type '{}' doesn't exist",
            config.expected_mime_type
        )
    })?;
    let format = formats
        .iter()
        .find_map(|(mime_type, format)| {
            mime_type.eq_ignore_ascii_case(&selected).then_some(*format)
        })
        .context("Selected clipboard format disappeared")?;

    if format == formats::CF_UNICODETEXT {
        let mut text = String::new();
        clipboard_win::formats::Unicode
            .read_clipboard(&mut text)
            .map_err(|err| anyhow!("Failed to read Windows Unicode text: {err}"))?;
        config.writer.write_all(text.as_bytes())?;
    } else if is_html_format(format) {
        let html = Html::new().context("Failed to register the Windows HTML clipboard format")?;
        let mut content = Vec::new();
        html.read_clipboard(&mut content)
            .map_err(|err| anyhow!("Failed to read Windows HTML: {err}"))?;
        config.writer.write_all(&content)?;
    } else {
        let mut content = Vec::new();
        raw::get_vec(format, &mut content)
            .map_err(|err| anyhow!("Failed to read clipboard format '{selected}': {err}"))?;
        config.writer.write_all(&content)?;
    }
    config.writer.flush()?;

    Ok(())
}

fn open_clipboard() -> Result<Clipboard> {
    Clipboard::new_attempts(10)
        .map_err(|err| anyhow!("Failed to open the Windows clipboard: {err}"))
}

fn available_formats() -> Vec<(String, u32)> {
    let mut formats = Vec::new();
    let mut seen = HashSet::new();

    for format in raw::EnumFormats::new() {
        let Some(mime_type) = mime_type_for_format(format) else {
            continue;
        };
        if seen.insert(mime_type.to_ascii_lowercase()) {
            formats.push((mime_type, format));
        }
    }

    formats
}

fn format_for_mime_type(mime_type: &str) -> Result<u32> {
    if is_text_type(mime_type) {
        return Ok(formats::CF_UNICODETEXT);
    }

    let format_name = if is_html_type(mime_type) {
        "HTML Format"
    } else if is_rtf_type(mime_type) {
        "Rich Text Format"
    } else {
        mime_type
    };

    raw::register_format(format_name)
        .map(|format| format.get())
        .ok_or_else(|| anyhow!("Failed to register clipboard format '{format_name}'"))
}

fn mime_type_for_format(format: u32) -> Option<String> {
    if format == formats::CF_UNICODETEXT {
        return Some("text/plain;charset=utf-8".to_string());
    }

    let format_name = raw::format_name_big(format)?;
    match format_name.as_str() {
        "HTML Format" => Some("text/html".to_string()),
        "Rich Text Format" => Some("application/rtf".to_string()),
        name if name.starts_with("CF_") => None,
        _ => Some(format_name),
    }
}

fn is_html_format(format: u32) -> bool {
    Html::new().is_some_and(|html| html.code() == format)
}

fn is_text_type(mime_type: &str) -> bool {
    matches!(
        mime_type.to_ascii_lowercase().as_str(),
        "text/plain" | "text/plain;charset=utf-8" | "text" | "string" | "utf8_string"
    )
}

fn is_html_type(mime_type: &str) -> bool {
    matches!(
        mime_type.to_ascii_lowercase().as_str(),
        "text/html" | "text/html;charset=utf-8" | "html"
    )
}

fn is_rtf_type(mime_type: &str) -> bool {
    matches!(
        mime_type.to_ascii_lowercase().as_str(),
        "application/rtf" | "text/rtf" | "rtf"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_common_mime_types() {
        assert_eq!(
            format_for_mime_type("text/plain").unwrap(),
            formats::CF_UNICODETEXT
        );
        assert_eq!(
            raw::format_name_big(format_for_mime_type("text/html").unwrap()).unwrap(),
            "HTML Format"
        );
        assert_eq!(
            raw::format_name_big(format_for_mime_type("application/rtf").unwrap()).unwrap(),
            "Rich Text Format"
        );
        assert_eq!(
            raw::format_name_big(format_for_mime_type("application/x-richclip-test").unwrap())
                .unwrap(),
            "application/x-richclip-test"
        );
    }

    #[test]
    fn ignores_unmapped_standard_formats() {
        assert_eq!(
            mime_type_for_format(formats::CF_UNICODETEXT).unwrap(),
            "text/plain;charset=utf-8"
        );
        assert!(mime_type_for_format(formats::CF_BITMAP).is_none());
    }
}
