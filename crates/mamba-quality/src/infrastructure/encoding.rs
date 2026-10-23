use crate::domain::{model::QualityIssue, source::SourceEncoding};
use std::path::Path;

pub fn newline(source: &str) -> &'static str {
    match source.find(['\r', '\n']) {
        Some(position) if source[position..].starts_with("\r\n") => "\r\n",
        Some(position) if source.as_bytes()[position] == b'\r' => "\r",
        _ => "\n",
    }
}

fn failure(path: &Path, message: impl Into<String>) -> QualityIssue {
    QualityIssue::new("source-encoding", message, Some(path.into()))
}

pub fn decode(path: &Path, bytes: &[u8]) -> Result<(String, SourceEncoding), QualityIssue> {
    let bom = bytes.starts_with(&[0xef, 0xbb, 0xbf]);
    let content = if bom { &bytes[3..] } else { bytes };
    // PEP 263: only a comment on line one or two can declare the encoding.
    let cookie = regex::Regex::new(r"^[ \t\x0c]*#.*?coding[=:][ \t]*([-_.a-zA-Z0-9]+)")
        .expect("static encoding regex");
    let mut label = None;
    for (index, line) in content.split(|byte| *byte == b'\n').take(2).enumerate() {
        let ascii = String::from_utf8_lossy(line);
        if index == 1 {
            let first = content
                .split(|byte| *byte == b'\n')
                .next()
                .unwrap_or_default();
            let first = String::from_utf8_lossy(first);
            let trimmed = first.trim_start_matches([' ', '\t', '\x0c']);
            if !trimmed.is_empty() && !trimmed.starts_with('#') && !trimmed.starts_with('\r') {
                break;
            }
        }
        if let Some(matches) = cookie.captures(&ascii) {
            label = Some(matches[1].to_ascii_lowercase().replace('_', "-"));
            break;
        }
    }
    let label = label.as_deref().unwrap_or("utf-8");
    let utf8 = matches!(label, "utf-8" | "utf8" | "utf-8-sig");
    if bom && !utf8 {
        return Err(failure(
            path,
            "UTF-8 BOM conflicts with the encoding declaration",
        ));
    }
    if utf8 {
        let text = std::str::from_utf8(content)
            .map_err(|error| failure(path, format!("Invalid UTF-8: {error}")))?;
        return Ok((text.into(), SourceEncoding::Utf8 { bom }));
    }
    if matches!(label, "latin-1" | "latin1" | "iso-8859-1" | "iso-latin-1") {
        return Ok((
            content.iter().map(|byte| char::from(*byte)).collect(),
            SourceEncoding::Latin1,
        ));
    }
    if matches!(label, "ascii" | "us-ascii") {
        if !content.is_ascii() {
            return Err(failure(path, "Non-ASCII bytes in an ASCII source file"));
        }
        return Ok((
            String::from_utf8(content.to_vec()).unwrap(),
            SourceEncoding::Ascii,
        ));
    }
    let encoding = encoding_rs::Encoding::for_label(label.as_bytes())
        .ok_or_else(|| failure(path, format!("Unsupported source encoding: {label}")))?;
    let (text, errors) = encoding.decode_without_bom_handling(content);
    if errors {
        return Err(failure(path, format!("Invalid bytes for {label}")));
    }
    let text = text.into_owned();
    let source_encoding = SourceEncoding::Named(encoding.name());
    if encode(path, &text, &source_encoding)? != bytes {
        return Err(failure(
            path,
            format!("Encoding {label} cannot round-trip the original bytes"),
        ));
    }
    Ok((text, source_encoding))
}

pub fn encode(path: &Path, text: &str, encoding: &SourceEncoding) -> Result<Vec<u8>, QualityIssue> {
    match encoding {
        SourceEncoding::Utf8 { bom } => {
            let mut bytes = if *bom {
                vec![0xef, 0xbb, 0xbf]
            } else {
                Vec::new()
            };
            bytes.extend_from_slice(text.as_bytes());
            Ok(bytes)
        }
        SourceEncoding::Latin1 | SourceEncoding::Ascii => {
            let maximum = if matches!(encoding, SourceEncoding::Ascii) {
                127
            } else {
                255
            };
            text.chars()
                .map(|character| {
                    let value = u32::from(character);
                    if value <= maximum {
                        Ok(value as u8)
                    } else {
                        Err(failure(
                            path,
                            "The candidate cannot be saved in the original encoding",
                        ))
                    }
                })
                .collect()
        }
        SourceEncoding::Named(label) => {
            let encoding = encoding_rs::Encoding::for_label(label.as_bytes()).unwrap();
            let (bytes, _, errors) = encoding.encode(text);
            if errors {
                Err(failure(
                    path,
                    "The candidate cannot be saved in the original encoding",
                ))
            } else {
                Ok(bytes.into_owned())
            }
        }
    }
}
