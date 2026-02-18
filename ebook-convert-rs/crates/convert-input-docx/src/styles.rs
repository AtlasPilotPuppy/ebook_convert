//! Parse Word styles and numbering definitions for heading/list detection.

use std::collections::HashMap;
use quick_xml::events::Event;
use quick_xml::Reader;

/// Style information extracted from word/styles.xml.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct StyleInfo {
    /// The style name (e.g., "heading 1", "Normal", "List Paragraph")
    pub name: String,
    /// The parent style ID, if any
    pub based_on: Option<String>,
    /// Outline level (0 = heading 1, 1 = heading 2, etc.)
    pub outline_level: Option<u8>,
}

/// Parse `word/styles.xml` and return a map of style_id → StyleInfo.
///
/// This identifies heading styles (Heading 1-6) and list styles.
pub fn parse_styles(xml: &str) -> HashMap<String, StyleInfo> {
    let mut styles = HashMap::new();
    let mut reader = Reader::from_str(xml);

    let mut current_id = String::new();
    let mut current_name = String::new();
    let mut current_based_on: Option<String> = None;
    let mut current_outline: Option<u8> = None;
    let mut in_style = false;

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                let local = String::from_utf8_lossy(e.local_name().as_ref()).to_string();

                match local.as_str() {
                    "style" => {
                        in_style = true;
                        current_id.clear();
                        current_name.clear();
                        current_based_on = None;
                        current_outline = None;

                        for attr in e.attributes().flatten() {
                            let key = String::from_utf8_lossy(attr.key.local_name().as_ref()).to_string();
                            if key == "styleId" {
                                current_id = String::from_utf8_lossy(&attr.value).to_string();
                            }
                        }
                    }
                    "name" if in_style => {
                        for attr in e.attributes().flatten() {
                            let key = String::from_utf8_lossy(attr.key.local_name().as_ref()).to_string();
                            if key == "val" {
                                current_name = String::from_utf8_lossy(&attr.value).to_string();
                            }
                        }
                    }
                    "basedOn" if in_style => {
                        for attr in e.attributes().flatten() {
                            let key = String::from_utf8_lossy(attr.key.local_name().as_ref()).to_string();
                            if key == "val" {
                                current_based_on = Some(String::from_utf8_lossy(&attr.value).to_string());
                            }
                        }
                    }
                    "outlineLvl" if in_style => {
                        for attr in e.attributes().flatten() {
                            let key = String::from_utf8_lossy(attr.key.local_name().as_ref()).to_string();
                            if key == "val" {
                                if let Ok(lvl) = String::from_utf8_lossy(&attr.value).parse::<u8>() {
                                    current_outline = Some(lvl);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::End(ref e)) => {
                let local = String::from_utf8_lossy(e.local_name().as_ref()).to_string();
                if local == "style" && in_style && !current_id.is_empty() {
                    // Detect heading by name pattern if outline level not set
                    if current_outline.is_none() {
                        let lower = current_name.to_lowercase();
                        if lower.starts_with("heading") || lower.starts_with("titre") {
                            if let Some(num) = lower.chars().find(|c| c.is_ascii_digit()) {
                                if let Some(n) = num.to_digit(10) {
                                    if (1..=9).contains(&n) {
                                        current_outline = Some((n - 1) as u8);
                                    }
                                }
                            }
                        }
                    }

                    styles.insert(
                        current_id.clone(),
                        StyleInfo {
                            name: current_name.clone(),
                            based_on: current_based_on.clone(),
                            outline_level: current_outline,
                        },
                    );
                    in_style = false;
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
    }

    styles
}

/// Numbering format info.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct NumberingInfo {
    /// "decimal", "bullet", "lowerLetter", etc.
    pub format: String,
    /// Indentation level
    pub level: u8,
}

/// Parse `word/numbering.xml` and return a map of numId → NumberingInfo.
pub fn parse_numbering(xml: &str) -> HashMap<String, NumberingInfo> {
    let mut numbering = HashMap::new();
    let mut reader = Reader::from_str(xml);

    let mut current_num_id = String::new();
    let mut current_format = String::from("decimal");
    let mut current_level: u8 = 0;
    let mut in_abstract = false;

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                let local = String::from_utf8_lossy(e.local_name().as_ref()).to_string();

                match local.as_str() {
                    "abstractNum" => {
                        in_abstract = true;
                        current_format = "decimal".to_string();
                        for attr in e.attributes().flatten() {
                            let key = String::from_utf8_lossy(attr.key.local_name().as_ref()).to_string();
                            if key == "abstractNumId" {
                                current_num_id = String::from_utf8_lossy(&attr.value).to_string();
                            }
                        }
                    }
                    "lvl" if in_abstract => {
                        for attr in e.attributes().flatten() {
                            let key = String::from_utf8_lossy(attr.key.local_name().as_ref()).to_string();
                            if key == "ilvl" {
                                current_level = String::from_utf8_lossy(&attr.value).parse().unwrap_or(0);
                            }
                        }
                    }
                    "numFmt" if in_abstract => {
                        for attr in e.attributes().flatten() {
                            let key = String::from_utf8_lossy(attr.key.local_name().as_ref()).to_string();
                            if key == "val" {
                                current_format = String::from_utf8_lossy(&attr.value).to_string();
                            }
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::End(ref e)) => {
                let local = String::from_utf8_lossy(e.local_name().as_ref()).to_string();
                if local == "abstractNum" && in_abstract {
                    numbering.insert(
                        current_num_id.clone(),
                        NumberingInfo {
                            format: current_format.clone(),
                            level: current_level,
                        },
                    );
                    in_abstract = false;
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
    }

    numbering
}

/// Determine the heading level (1-6) for a given style ID.
/// Returns None if the style is not a heading.
pub fn heading_level(style_id: &str, styles: &HashMap<String, StyleInfo>) -> Option<u8> {
    if let Some(info) = styles.get(style_id) {
        if let Some(outline) = info.outline_level {
            // outline level 0 = h1, 1 = h2, etc.
            return Some((outline + 1).min(6));
        }
        // Check parent style
        if let Some(ref parent) = info.based_on {
            return heading_level(parent, styles);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_styles() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:style w:type="paragraph" w:styleId="Heading1">
    <w:name w:val="heading 1"/>
    <w:pPr><w:outlineLvl w:val="0"/></w:pPr>
  </w:style>
  <w:style w:type="paragraph" w:styleId="Heading2">
    <w:name w:val="heading 2"/>
    <w:pPr><w:outlineLvl w:val="1"/></w:pPr>
  </w:style>
  <w:style w:type="paragraph" w:styleId="Normal">
    <w:name w:val="Normal"/>
  </w:style>
</w:styles>"#;

        let styles = parse_styles(xml);
        assert!(styles.contains_key("Heading1"));
        assert_eq!(styles["Heading1"].outline_level, Some(0));
        assert_eq!(heading_level("Heading1", &styles), Some(1));
        assert_eq!(heading_level("Heading2", &styles), Some(2));
        assert_eq!(heading_level("Normal", &styles), None);
    }

    #[test]
    fn test_heading_by_name_fallback() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:style w:type="paragraph" w:styleId="Heading3">
    <w:name w:val="heading 3"/>
  </w:style>
</w:styles>"#;

        let styles = parse_styles(xml);
        assert_eq!(heading_level("Heading3", &styles), Some(3));
    }
}
