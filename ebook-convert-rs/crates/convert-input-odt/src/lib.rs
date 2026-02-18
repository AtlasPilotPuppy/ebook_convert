//! ODT (OpenDocument Text) input plugin — reads ODT files into BookDocument.
//!
//! ODT files are ZIP archives containing XML content (similar to DOCX).
//! Main content is in `content.xml`, metadata in `meta.xml`, styles in `styles.xml`.

use std::io::Read;
use std::path::Path;

use convert_core::book::{BookDocument, EbookFormat, ManifestData, ManifestItem, TocEntry};
use convert_core::error::{ConvertError, Result};
use convert_core::options::ConversionOptions;
use convert_core::plugin::InputPlugin;
use quick_xml::events::Event;
use quick_xml::Reader;
use regex::Regex;

pub struct OdtInputPlugin;

impl InputPlugin for OdtInputPlugin {
    fn name(&self) -> &str {
        "ODT Input"
    }

    fn supported_formats(&self) -> &[EbookFormat] {
        &[EbookFormat::Odt]
    }

    fn convert(&self, input_path: &Path, _options: &ConversionOptions) -> Result<BookDocument> {
        log::info!("Reading ODT: {}", input_path.display());
        parse_odt(input_path)
    }
}

fn parse_odt(path: &Path) -> Result<BookDocument> {
    let file = std::fs::File::open(path)
        .map_err(|e| ConvertError::Odt(format!("Cannot open {}: {}", path.display(), e)))?;

    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| ConvertError::Odt(format!("Invalid ODT (ZIP) file: {}", e)))?;

    let mut book = BookDocument::new();
    book.base_path = path.parent().map(|p| p.to_path_buf());

    // Parse metadata from meta.xml
    if let Ok(meta_xml) = read_zip_string(&mut archive, "meta.xml") {
        parse_metadata(&meta_xml, &mut book);
    }

    // Parse styles to detect heading levels
    let heading_styles = if let Ok(styles_xml) = read_zip_string(&mut archive, "styles.xml") {
        parse_heading_styles(&styles_xml)
    } else {
        Vec::new()
    };

    // Also check content.xml for automatic styles
    let content_xml = read_zip_string(&mut archive, "content.xml")
        .map_err(|e| ConvertError::Odt(format!("Missing content.xml: {}", e)))?;

    let auto_heading_styles = parse_heading_styles(&content_xml);
    let all_heading_styles: Vec<String> = heading_styles
        .into_iter()
        .chain(auto_heading_styles)
        .collect();

    // Extract images from Pictures/ directory
    let image_files: Vec<String> = (0..archive.len())
        .filter_map(|i| {
            let file = archive.by_index(i).ok()?;
            let name = file.name().to_string();
            if name.starts_with("Pictures/") && !name.ends_with('/') {
                Some(name)
            } else {
                None
            }
        })
        .collect();

    for img_name in &image_files {
        if let Ok(data) = read_zip_binary(&mut archive, img_name) {
            let ext = img_name.rsplit('.').next().unwrap_or("jpg");
            let mime = match ext {
                "png" => "image/png",
                "gif" => "image/gif",
                "svg" => "image/svg+xml",
                "bmp" => "image/bmp",
                "webp" => "image/webp",
                _ => "image/jpeg",
            };
            let id = img_name.replace(['/', '.'], "_");
            let href = format!("images/{}", img_name.trim_start_matches("Pictures/"));
            let item = ManifestItem::new(&id, &href, mime, ManifestData::Binary(data));
            book.manifest.add(item);
        }
    }

    // Convert content.xml to HTML
    let html = convert_content_xml(&content_xml, &all_heading_styles);

    // Set title from filename if not in metadata
    if book.metadata.title().is_none() {
        let title = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Untitled");
        book.metadata.set_title(title);
    }

    let title = book.metadata.title().unwrap_or("Untitled").to_string();
    let lang = book
        .metadata
        .get_first_value("language")
        .unwrap_or("en")
        .to_string();
    let xhtml = convert_utils::xml::xhtml11_document(&title, &lang, Some("style.css"), &html);

    let content_item = ManifestItem::new(
        "content",
        "content.xhtml",
        "application/xhtml+xml",
        ManifestData::Xhtml(xhtml),
    );
    book.manifest.add(content_item);
    book.spine.push("content", true);

    // Default stylesheet
    let css = r#"body { font-family: serif; line-height: 1.6; margin: 1em; }
p { margin: 0.5em 0; text-indent: 1.5em; }
p:first-child { text-indent: 0; }
h1, h2, h3, h4, h5, h6 { text-indent: 0; margin: 1em 0 0.5em; }
img { max-width: 100%; height: auto; }
.center { text-align: center; }
.right { text-align: right; }"#;
    let css_item = ManifestItem::new(
        "style",
        "style.css",
        "text/css",
        ManifestData::Css(css.to_string()),
    );
    book.manifest.add(css_item);

    // Build TOC from headings
    build_toc(&html, &mut book);

    let image_count = book
        .manifest
        .iter()
        .filter(|i| i.media_type.starts_with("image/"))
        .count();
    log::info!("Parsed ODT: \"{}\" with {} images", title, image_count);

    Ok(book)
}

/// Parse metadata from meta.xml.
fn parse_metadata(xml: &str, book: &mut BookDocument) {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut buf = Vec::new();
    let mut current_tag = String::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                current_tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
            }
            Ok(Event::Text(ref e)) => {
                let text = e.unescape().unwrap_or_default().to_string();
                let text = text.trim();
                if text.is_empty() {
                    buf.clear();
                    continue;
                }
                match current_tag.as_str() {
                    "dc:title" => book.metadata.set_title(text),
                    "dc:creator" | "meta:initial-creator" => book.metadata.add("creator", text),
                    "dc:description" | "dc:subject" => book.metadata.set("description", text),
                    "dc:language" => book.metadata.set("language", text),
                    "dc:date" | "meta:creation-date" => book.metadata.set("date", text),
                    "meta:keyword" => book.metadata.add("subject", text),
                    _ => {}
                }
            }
            Ok(Event::End(_)) => {
                current_tag.clear();
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
}

/// Parse styles to find heading style names.
fn parse_heading_styles(xml: &str) -> Vec<String> {
    let mut styles = Vec::new();
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                if name == "style:style" || name == "text:list-style" {
                    let mut style_name = String::new();
                    let mut parent = String::new();
                    for attr in e.attributes().flatten() {
                        let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                        let val = String::from_utf8_lossy(&attr.value).to_string();
                        match key.as_str() {
                            "style:name" => style_name = val,
                            "style:parent-style-name" => parent = val,
                            _ => {}
                        }
                    }
                    if parent.starts_with("Heading") || style_name.starts_with("Heading") {
                        styles.push(style_name);
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    styles
}

/// Convert content.xml to HTML.
fn convert_content_xml(xml: &str, heading_styles: &[String]) -> String {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);

    let mut html = String::new();
    let mut buf = Vec::new();
    let mut in_text_body = false;
    let mut in_para = false;
    let mut current_tag = String::new(); // "p" or "h1"-"h6"
    let mut para_buf = String::new();
    let mut span_stack: Vec<SpanFormat> = Vec::new();
    let mut in_list = false;
    let mut list_depth = 0;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                // Empty events are handled by the Start branch matching

                match name.as_str() {
                    "office:text" => {
                        in_text_body = true;
                    }
                    "text:p" | "text:h" if in_text_body => {
                        // Determine tag (p or heading level)
                        let mut style_name = String::new();
                        let mut outline_level = 0u8;
                        for attr in e.attributes().flatten() {
                            let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                            let val = String::from_utf8_lossy(&attr.value).to_string();
                            match key.as_str() {
                                "text:style-name" => style_name = val,
                                "text:outline-level" => {
                                    outline_level = val.parse().unwrap_or(0);
                                }
                                _ => {}
                            }
                        }

                        let heading_level = if name == "text:h" && outline_level > 0 {
                            outline_level.min(6)
                        } else if heading_styles.iter().any(|s| s == &style_name) {
                            // Try to extract level from style name like "Heading_20_1"
                            extract_heading_level(&style_name).unwrap_or(1)
                        } else {
                            0
                        };

                        if heading_level > 0 {
                            current_tag = format!("h{}", heading_level);
                        } else {
                            current_tag = "p".to_string();
                        }
                        in_para = true;
                        para_buf.clear();
                    }
                    "text:span" if in_para => {
                        // Check for bold/italic in style
                        let mut style_name = String::new();
                        for attr in e.attributes().flatten() {
                            let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                            if key == "text:style-name" {
                                style_name = String::from_utf8_lossy(&attr.value).to_string();
                            }
                        }
                        // We'll detect bold/italic from style name heuristics
                        let fmt = detect_span_format(&style_name);
                        apply_span_open(&mut para_buf, &fmt);
                        span_stack.push(fmt);
                    }
                    "text:line-break" if in_para => {
                        para_buf.push_str("<br/>");
                    }
                    "text:tab" if in_para => {
                        para_buf.push_str("&#9;");
                    }
                    "text:s" if in_para => {
                        // Space(s)
                        let count: usize = e
                            .attributes()
                            .flatten()
                            .find(|a| String::from_utf8_lossy(a.key.as_ref()) == "text:c")
                            .and_then(|a| String::from_utf8_lossy(&a.value).parse().ok())
                            .unwrap_or(1);
                        for _ in 0..count {
                            para_buf.push(' ');
                        }
                    }
                    "text:list" if in_text_body => {
                        list_depth += 1;
                        if !in_list {
                            html.push_str("<ul>\n");
                            in_list = true;
                        } else {
                            para_buf.push_str("<ul>\n");
                        }
                    }
                    "text:list-item" if in_list => {
                        if list_depth == 1 && !in_para {
                            html.push_str("<li>");
                        }
                    }
                    "text:a" if in_para => {
                        for attr in e.attributes().flatten() {
                            let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                            if key == "xlink:href" {
                                let href = String::from_utf8_lossy(&attr.value).to_string();
                                para_buf.push_str(&format!(
                                    r#"<a href="{}">"#,
                                    convert_utils::xml::escape_xml_attr(&href)
                                ));
                            }
                        }
                    }
                    "draw:image" if in_para => {
                        for attr in e.attributes().flatten() {
                            let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                            if key == "xlink:href" {
                                let href = String::from_utf8_lossy(&attr.value).to_string();
                                let clean_href = href.trim_start_matches("Pictures/");
                                para_buf.push_str(&format!(
                                    r#"<img src="images/{}" alt=""/>"#,
                                    clean_href
                                ));
                            }
                        }
                    }
                    "table:table" if in_text_body => {
                        html.push_str("<table>\n");
                    }
                    "table:table-row" => {
                        html.push_str("<tr>");
                    }
                    "table:table-cell" => {
                        html.push_str("<td>");
                    }
                    _ => {}
                }
            }
            Ok(Event::End(ref e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                match name.as_str() {
                    "office:text" => {
                        in_text_body = false;
                    }
                    "text:p" | "text:h" if in_para => {
                        html.push_str(&format!(
                            "<{}>{}</{}>\n",
                            current_tag, para_buf, current_tag
                        ));
                        in_para = false;
                        para_buf.clear();
                    }
                    "text:span" if in_para => {
                        if let Some(fmt) = span_stack.pop() {
                            apply_span_close(&mut para_buf, &fmt);
                        }
                    }
                    "text:a" if in_para => {
                        para_buf.push_str("</a>");
                    }
                    "text:list" => {
                        list_depth -= 1;
                        if list_depth == 0 {
                            html.push_str("</ul>\n");
                            in_list = false;
                        }
                    }
                    "text:list-item" if in_list && list_depth == 1 => {
                        html.push_str("</li>\n");
                    }
                    "table:table" => html.push_str("</table>\n"),
                    "table:table-row" => html.push_str("</tr>\n"),
                    "table:table-cell" => html.push_str("</td>"),
                    _ => {}
                }
            }
            Ok(Event::Text(ref e)) => {
                if in_para {
                    let text = e.unescape().unwrap_or_default().to_string();
                    para_buf.push_str(&convert_utils::xml::escape_xml_text(&text));
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    html
}

#[derive(Default)]
struct SpanFormat {
    bold: bool,
    italic: bool,
    underline: bool,
}

fn detect_span_format(style_name: &str) -> SpanFormat {
    let lower = style_name.to_lowercase();
    SpanFormat {
        bold: lower.contains("bold") || lower.contains("strong"),
        italic: lower.contains("italic") || lower.contains("emphasis"),
        underline: lower.contains("underline"),
    }
}

fn apply_span_open(buf: &mut String, fmt: &SpanFormat) {
    if fmt.bold {
        buf.push_str("<strong>");
    }
    if fmt.italic {
        buf.push_str("<em>");
    }
    if fmt.underline {
        buf.push_str("<u>");
    }
}

fn apply_span_close(buf: &mut String, fmt: &SpanFormat) {
    if fmt.underline {
        buf.push_str("</u>");
    }
    if fmt.italic {
        buf.push_str("</em>");
    }
    if fmt.bold {
        buf.push_str("</strong>");
    }
}

fn extract_heading_level(style_name: &str) -> Option<u8> {
    // Try patterns like "Heading_20_1", "Heading 1", "Heading1"
    let re = Regex::new(r"(?i)heading[_ ]*(?:20[_ ]*)?([\d])").unwrap();
    re.captures(style_name)
        .and_then(|c| c[1].parse::<u8>().ok())
        .filter(|&n| (1..=6).contains(&n))
}

/// Build TOC from heading elements in HTML.
fn build_toc(html: &str, book: &mut BookDocument) {
    let tag_re = Regex::new(r"<[^>]+>").unwrap();
    let patterns = [
        Regex::new(r"(?i)<h1[^>]*>(.*?)</h1>").unwrap(),
        Regex::new(r"(?i)<h2[^>]*>(.*?)</h2>").unwrap(),
        Regex::new(r"(?i)<h3[^>]*>(.*?)</h3>").unwrap(),
    ];

    let mut headings: Vec<(usize, String)> = Vec::new();
    for pattern in &patterns {
        for cap in pattern.captures_iter(html) {
            let pos = cap.get(0).unwrap().start();
            let text = tag_re.replace_all(&cap[1], "").trim().to_string();
            if !text.is_empty() {
                headings.push((pos, text));
            }
        }
    }
    headings.sort_by_key(|(pos, _)| *pos);

    for (_, text) in &headings {
        book.toc.add(TocEntry::new(text, "content.xhtml"));
    }

    if headings.is_empty() {
        if let Some(title) = book.metadata.title() {
            book.toc.add(TocEntry::new(title, "content.xhtml"));
        }
    }
}

fn read_zip_string(
    archive: &mut zip::ZipArchive<std::fs::File>,
    name: &str,
) -> std::result::Result<String, String> {
    let mut file = archive
        .by_name(name)
        .map_err(|e| format!("{}: {}", name, e))?;
    let mut s = String::new();
    file.read_to_string(&mut s)
        .map_err(|e| format!("{}: {}", name, e))?;
    Ok(s)
}

fn read_zip_binary(
    archive: &mut zip::ZipArchive<std::fs::File>,
    name: &str,
) -> std::result::Result<Vec<u8>, String> {
    let mut file = archive
        .by_name(name)
        .map_err(|e| format!("{}: {}", name, e))?;
    let mut data = Vec::new();
    file.read_to_end(&mut data)
        .map_err(|e| format!("{}: {}", name, e))?;
    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_heading_level() {
        assert_eq!(extract_heading_level("Heading_20_1"), Some(1));
        assert_eq!(extract_heading_level("Heading_20_2"), Some(2));
        assert_eq!(extract_heading_level("Heading 3"), Some(3));
        assert_eq!(extract_heading_level("Normal"), None);
    }

    #[test]
    fn test_detect_span_format() {
        let fmt = detect_span_format("T1_Bold");
        assert!(fmt.bold);
        assert!(!fmt.italic);

        let fmt = detect_span_format("Emphasis_Italic");
        assert!(fmt.italic);
    }

    #[test]
    fn test_parse_metadata() {
        let xml = r#"<?xml version="1.0"?>
<office:document-meta xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:meta="urn:oasis:names:tc:opendocument:xmlns:meta:1.0" xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0">
  <office:meta>
    <dc:title>My Document</dc:title>
    <dc:creator>John Doe</dc:creator>
    <dc:language>en-US</dc:language>
  </office:meta>
</office:document-meta>"#;

        let mut book = BookDocument::new();
        parse_metadata(xml, &mut book);
        assert_eq!(book.metadata.title().unwrap(), "My Document");
        assert_eq!(book.metadata.get_first_value("language").unwrap(), "en-US");
    }

    #[test]
    fn test_convert_content_xml_simple() {
        let xml = r#"<?xml version="1.0"?>
<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0">
  <office:body>
    <office:text>
      <text:h text:outline-level="1">Chapter One</text:h>
      <text:p>First paragraph of text.</text:p>
      <text:p>Second paragraph.</text:p>
    </office:text>
  </office:body>
</office:document-content>"#;

        let html = convert_content_xml(xml, &[]);
        assert!(html.contains("<h1>Chapter One</h1>"));
        assert!(html.contains("<p>First paragraph of text.</p>"));
        assert!(html.contains("<p>Second paragraph.</p>"));
    }

    #[test]
    fn test_convert_content_xml_with_list() {
        let xml = r#"<?xml version="1.0"?>
<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0">
  <office:body>
    <office:text>
      <text:p>Intro</text:p>
      <text:list>
        <text:list-item>
          <text:p>Item 1</text:p>
        </text:list-item>
        <text:list-item>
          <text:p>Item 2</text:p>
        </text:list-item>
      </text:list>
    </office:text>
  </office:body>
</office:document-content>"#;

        let html = convert_content_xml(xml, &[]);
        assert!(html.contains("<ul>"));
        assert!(html.contains("Item 1"));
        assert!(html.contains("Item 2"));
    }

    #[test]
    fn test_build_toc() {
        let html = "<h1>Title</h1><p>text</p><h2>Section</h2>";
        let mut book = BookDocument::new();
        book.metadata.set_title("Test");
        build_toc(html, &mut book);
        assert_eq!(book.toc.entries.len(), 2);
        assert_eq!(book.toc.entries[0].title, "Title");
    }
}
