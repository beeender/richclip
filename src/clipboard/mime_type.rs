use anyhow::{Result, bail};

const TEXT_TYPE_EXACT: &[&str] = &[
    "text/plain;charset=utf-8",
    "text/plain",
    "TEXT",
    "STRING",
    "UTF8_STRING",
];

const TEXT_TYPE_SUFFIX: &[&str] = &[
    "script",
    "xml",
    "yaml",
    "csv",
    "ini",
];

fn try_any_text(supported: &Vec<String>) -> Option<String> {
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

pub(super) fn decide_mime_type(preferred: &str, supported: &Vec<String>) -> Result<String> {
    log::debug!("preferred mime-type '{}', supported mime-types:", preferred);
    for s in supported {
        log::debug!("{}", s);
    }

    if preferred.is_empty() {
        if let Some(ret) = try_any_text(supported) {
            return Ok(ret);
        }
    }
    bail!("No mime-type matches")
}
