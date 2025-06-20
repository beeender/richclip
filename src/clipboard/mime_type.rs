use anyhow::{Result, bail};

const TEXT_TYPE_EXACT: &[&str] = &[
    "text/plain;charset=utf-8",
    "text/plain",
    "TEXT",
    "STRING",
    "UTF8_STRING",
    "json",

    "CF_TEXT",
    "CF_UNICODETEXT"
];

const TEXT_TYPE_SUFFIX: &[&str] = &["script", "xml", "yaml", "csv", "ini"];

fn try_any_text(supported: &[String]) -> Option<String> {
    // Match the exact type with priorities
    for expected in TEXT_TYPE_EXACT {
        if let Some(r) = supported
            .iter()
            .find(|str| str.eq_ignore_ascii_case(expected))
        {
            return Some(r.clone());
        }
    }
    // Match the suffix
    for suffix in TEXT_TYPE_SUFFIX {
        if let Some(r) = supported
            .iter()
            .find(|str| str.to_ascii_lowercase().ends_with(suffix))
        {
            return Some(r.clone());
        }
    }
    // Try any types if it starts with "text/"
    if let Some(r) = supported
        .iter()
        .find(|str| str.to_ascii_lowercase().starts_with("text/"))
    {
        return Some(r.clone());
    }
    None
}

/// Based on the given preferred mime-type, and the mime-types supported by the current clipboard
/// content, return the best match mime-type to paste.
pub(super) fn decide_mime_type(preferred: &str, supported: &Vec<String>) -> Result<String> {
    log::debug!("preferred mime-type '{}', supported mime-types:", preferred);
    for s in supported {
        log::debug!("{}", s);
    }

    if preferred.is_empty()
        || preferred.eq_ignore_ascii_case("text")
        || preferred.eq_ignore_ascii_case("UTF8_STRING")
    {
        // Assume the normal text is requested
        if let Some(ret) = try_any_text(supported) {
            log::debug!("Use mime-type '{}'", ret);
            return Ok(ret);
        }
    } else if let Some(ret) = supported.iter().find(|t| t.eq_ignore_ascii_case(preferred)) {
        log::debug!("Use mime-type '{}'", ret);
        return Ok(ret.clone());
    }

    bail!("No mime-type matches")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_preferred() {
        // match a text type
        let r = decide_mime_type(
            "",
            &vec![
                "image/webp".to_string(),
                "text/plain;charset=utf-8".to_string(),
            ],
        )
        .unwrap();
        assert_eq!(r, "text/plain;charset=utf-8");

        // No match
        let r = decide_mime_type(
            "",
            &vec!["image/webp".to_string(), "video/x-flv".to_string()],
        );
        assert!(r.is_err());

        // Match suffix
        let r = decide_mime_type(
            "",
            &vec![
                "image/webp".to_string(),
                "application/postscript".to_string(),
            ],
        )
        .unwrap();
        assert_eq!(r, "application/postscript");
    }

    #[test]
    fn test_text_preferred() {
        // match a text type
        let r = decide_mime_type(
            "text",
            &vec![
                "image/webp".to_string(),
                "text/plain;charset=utf-8".to_string(),
            ],
        )
        .unwrap();
        assert_eq!(r, "text/plain;charset=utf-8");
    }

    #[test]
    fn test_exact_preferred() {
        // match a text type
        let r = decide_mime_type(
            "text/html",
            &vec![
                "text/plain;charset=utf-8".to_string(),
                "text/html".to_string(),
            ],
        )
        .unwrap();
        assert_eq!(r, "text/html");
    }
}
