//! MOBI/AZW/AZW3 input plugin — reads Mobipocket format files into BookDocument.
//!
//! Uses the `mobi` crate for header parsing, decompression, and content extraction.
//! MOBI files contain HTML content with embedded images stored as PDB records.

use std::path::Path;

use convert_core::book::{
    BookDocument, EbookFormat, ManifestData, ManifestItem, TocEntry,
};
use convert_core::error::{ConvertError, Result};
use convert_core::options::ConversionOptions;
use convert_core::plugin::InputPlugin;
use regex::Regex;

pub struct MobiInputPlugin;

impl InputPlugin for MobiInputPlugin {
    fn name(&self) -> &str {
        "MOBI Input"
    }

    fn supported_formats(&self) -> &[EbookFormat] {
        &[EbookFormat::Mobi, EbookFormat::Azw, EbookFormat::Azw3]
    }

    fn convert(&self, input_path: &Path, _options: &ConversionOptions) -> Result<BookDocument> {
        log::info!("Reading MOBI: {}", input_path.display());
        parse_mobi(input_path)
    }
}

fn parse_mobi(path: &Path) -> Result<BookDocument> {
    let data = std::fs::read(path)
        .map_err(|e| ConvertError::Mobi(format!("Cannot read {}: {}", path.display(), e)))?;

    // The mobi crate can panic on malformed files, so catch panics
    let mobi = std::panic::catch_unwind(|| mobi::Mobi::new(&data))
        .map_err(|_| ConvertError::Mobi("MOBI parser panicked (malformed file)".to_string()))?
        .map_err(|e| ConvertError::Mobi(format!("Invalid MOBI file: {}", e)))?;

    let mut book = BookDocument::new();
    book.base_path = path.parent().map(|p| p.to_path_buf());

    // -- Metadata --
    let title = mobi.title();
    book.metadata.set_title(&title);

    if let Some(author) = mobi.author() {
        // MOBI may have multiple authors separated by semicolons or ampersands
        for a in author.split(';').flat_map(|s| s.split('&')) {
            let a = a.trim();
            if !a.is_empty() {
                book.metadata.add("creator", a);
            }
        }
    }

    if let Some(publisher) = mobi.publisher() {
        book.metadata.set("publisher", publisher);
    }

    if let Some(description) = mobi.description() {
        book.metadata.set("description", description);
    }

    if let Some(isbn) = mobi.isbn() {
        book.metadata.set("identifier", isbn);
    }

    if let Some(date) = mobi.publish_date() {
        book.metadata.set("date", date);
    }

    let lang = format!("{:?}", mobi.language());
    book.metadata.set("language", lang_to_code(&lang));

    // -- Extract HTML content (can panic on malformed records) --
    // Try strict first, fall back to lossy
    let html_content = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        mobi.content_as_string()
    }))
    .ok()
    .and_then(|r| r.ok());

    let html_content = match html_content {
        Some(s) => s,
        None => {
            log::warn!("Strict MOBI content extraction failed, trying lossy mode");
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                mobi.content_as_string_lossy()
            }))
            .map_err(|_| ConvertError::Mobi("MOBI content extraction panicked".to_string()))?
        }
    };

    // -- Extract images (can panic on malformed records) --
    let image_records = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        mobi.image_records()
    }))
    .unwrap_or_default();
    let mut image_map: Vec<(String, String)> = Vec::new(); // (id, href)

    for (i, record) in image_records.iter().enumerate() {
        let content = record.content;
        if content.len() < 4 {
            continue;
        }

        let (mime, ext) = detect_image_type(content);
        let idx = i + 1; // MOBI image references are 1-based (recindex)
        let href = format!("images/image_{:04}.{}", idx, ext);
        let id = format!("img_{}", idx);

        let item = ManifestItem::new(&id, &href, mime, ManifestData::Binary(content.to_vec()));
        book.manifest.add(item);
        image_map.push((id, href));
    }

    // -- Process HTML: fix image references --
    // MOBI uses <img recindex="NNNN"> for image references
    let processed_html = fix_image_references(&html_content, &image_map);

    // -- Clean up MOBI-specific markup --
    let cleaned_html = clean_mobi_html(&processed_html);

    // Wrap in proper XHTML
    let xhtml = convert_utils::xml::xhtml11_document(&title, "en", Some("style.css"), &cleaned_html);

    let content_item = ManifestItem::new(
        "content",
        "content.xhtml",
        "application/xhtml+xml",
        ManifestData::Xhtml(xhtml),
    );
    book.manifest.add(content_item);
    book.spine.push("content", true);

    // Add default stylesheet
    let css = r#"body { font-family: serif; line-height: 1.6; margin: 1em; }
p { margin: 0.5em 0; text-indent: 1.5em; }
p:first-child { text-indent: 0; }
h1, h2, h3 { text-indent: 0; margin: 1em 0 0.5em; }
img { max-width: 100%; height: auto; }
.mbp_pagebreak { page-break-after: always; }"#;
    let css_item = ManifestItem::new("style", "style.css", "text/css", ManifestData::Css(css.to_string()));
    book.manifest.add(css_item);

    // Build basic TOC from headings
    build_toc_from_headings(&cleaned_html, &mut book);

    log::info!(
        "Parsed MOBI: \"{}\" with {} images",
        title,
        image_map.len()
    );

    Ok(book)
}

/// Fix MOBI image references: `<img recindex="N">` → `<img src="images/image_NNNN.ext">`
fn fix_image_references(html: &str, image_map: &[(String, String)]) -> String {
    let recindex_re = Regex::new(r#"<img\s[^>]*recindex\s*=\s*["']?(\d+)["']?[^>]*>"#).unwrap();

    recindex_re
        .replace_all(html, |caps: &regex::Captures| {
            let idx: usize = caps[1].parse().unwrap_or(0);
            if idx > 0 && idx <= image_map.len() {
                let (_, href) = &image_map[idx - 1];
                format!(r#"<img src="{}"/>"#, href)
            } else {
                caps[0].to_string()
            }
        })
        .to_string()
}

/// Clean MOBI-specific HTML artifacts.
fn clean_mobi_html(html: &str) -> String {
    let mut s = html.to_string();

    // Strip outer <html>/<head>/<body> wrapper — extract body content only
    let body_re = Regex::new(r"(?is)<body[^>]*>(.*)</body>").unwrap();
    if let Some(cap) = body_re.captures(&s) {
        s = cap[1].to_string();
    } else {
        // No <body>, try stripping <html> and <head> tags
        let html_re = Regex::new(r"(?is)</?html[^>]*>").unwrap();
        s = html_re.replace_all(&s, "").to_string();
    }

    // Remove <head>...</head> block if still present
    let head_re = Regex::new(r"(?is)<head[^>]*>.*?</head>").unwrap();
    s = head_re.replace_all(&s, "").to_string();

    // Remove MOBI-specific tags
    let filepos_re = Regex::new(r#"\s*filepos\s*=\s*["']?\d+["']?"#).unwrap();
    s = filepos_re.replace_all(&s, "").to_string();

    // Convert <mbp:pagebreak/> to standard page breaks
    let mbp_re = Regex::new(r"(?i)<mbp:pagebreak\s*/?>").unwrap();
    s = mbp_re
        .replace_all(&s, r#"<div class="mbp_pagebreak"></div>"#)
        .to_string();

    // Remove <mbp:nu> and </mbp:nu> tags
    let mbp_nu_re = Regex::new(r"(?i)</?mbp:[^>]+>").unwrap();
    s = mbp_nu_re.replace_all(&s, "").to_string();

    // Remove <guide>...</guide> blocks (MOBI metadata, not content)
    let guide_re = Regex::new(r"(?is)<guide[^>]*>.*?</guide>").unwrap();
    s = guide_re.replace_all(&s, "").to_string();

    // Remove empty anchors often found in MOBI
    let empty_a_re = Regex::new(r#"<a\s+[^>]*>\s*</a>"#).unwrap();
    s = empty_a_re.replace_all(&s, "").to_string();

    s
}

/// Build a basic TOC from heading elements in the HTML.
fn build_toc_from_headings(html: &str, book: &mut BookDocument) {
    let tag_re = Regex::new(r"<[^>]+>").unwrap();

    // Rust regex doesn't support backreferences; use separate patterns per level
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

/// Detect image type from magic bytes.
fn detect_image_type(data: &[u8]) -> (&str, &str) {
    if data.starts_with(b"\x89PNG") {
        ("image/png", "png")
    } else if data.starts_with(b"\xff\xd8\xff") {
        ("image/jpeg", "jpg")
    } else if data.starts_with(b"GIF8") {
        ("image/gif", "gif")
    } else if data.starts_with(b"BM") {
        ("image/bmp", "bmp")
    } else if data.starts_with(b"RIFF") && data.len() > 12 && &data[8..12] == b"WEBP" {
        ("image/webp", "webp")
    } else {
        // Default to JPEG for unknown (most MOBI images are JPEG)
        ("image/jpeg", "jpg")
    }
}

/// Map Language debug name to ISO 639-1 code.
fn lang_to_code(lang: &str) -> &str {
    match lang {
        "English" => "en",
        "French" => "fr",
        "German" => "de",
        "Spanish" => "es",
        "Italian" => "it",
        "Portuguese" => "pt",
        "Dutch" => "nl",
        "Russian" => "ru",
        "Japanese" => "ja",
        "Chinese" => "zh",
        "Korean" => "ko",
        "Arabic" => "ar",
        "Hindi" => "hi",
        "Swedish" => "sv",
        "Danish" => "da",
        "Norwegian" => "no",
        "Finnish" => "fi",
        "Polish" => "pl",
        "Turkish" => "tr",
        "Czech" => "cs",
        _ => "en",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fix_image_references() {
        let html = r#"<p><img recindex="1"> text <img recindex="2"></p>"#;
        let map = vec![
            ("img_1".to_string(), "images/image_0001.jpg".to_string()),
            ("img_2".to_string(), "images/image_0002.png".to_string()),
        ];
        let result = fix_image_references(html, &map);
        assert!(result.contains(r#"src="images/image_0001.jpg""#));
        assert!(result.contains(r#"src="images/image_0002.png""#));
        assert!(!result.contains("recindex"));
    }

    #[test]
    fn test_clean_mobi_html() {
        let html = r#"<p filepos="123">Hello</p><mbp:pagebreak/><mbp:nu>text</mbp:nu>"#;
        let result = clean_mobi_html(html);
        assert!(!result.contains("filepos"));
        assert!(!result.contains("<mbp:nu>"));
        assert!(result.contains("mbp_pagebreak"));
    }

    #[test]
    fn test_clean_mobi_html_strips_wrapper() {
        let html = r#"<html><head><guide><reference type="toc"/></guide></head><body><p>Hello</p></body></html>"#;
        let result = clean_mobi_html(html);
        assert!(!result.contains("<html>"));
        assert!(!result.contains("<head>"));
        assert!(!result.contains("<body>"));
        assert!(!result.contains("<guide>"));
        assert!(result.contains("<p>Hello</p>"));
    }

    #[test]
    fn test_detect_image_type() {
        assert_eq!(detect_image_type(b"\x89PNG\r\n\x1a\n"), ("image/png", "png"));
        assert_eq!(detect_image_type(b"\xff\xd8\xff\xe0"), ("image/jpeg", "jpg"));
        assert_eq!(detect_image_type(b"GIF89a"), ("image/gif", "gif"));
        assert_eq!(detect_image_type(b"\x00\x00"), ("image/jpeg", "jpg")); // fallback
    }

    #[test]
    fn test_lang_to_code() {
        assert_eq!(lang_to_code("English"), "en");
        assert_eq!(lang_to_code("French"), "fr");
        assert_eq!(lang_to_code("Unknown"), "en");
    }

    #[test]
    fn test_build_toc_from_headings() {
        let html = r#"<h1>Chapter 1</h1><p>text</p><h2>Section 1.1</h2><p>more</p><h1>Chapter 2</h1>"#;
        let mut book = BookDocument::new();
        book.metadata.set_title("Test");
        build_toc_from_headings(html, &mut book);
        assert_eq!(book.toc.entries.len(), 3);
        assert_eq!(book.toc.entries[0].title, "Chapter 1");
        assert_eq!(book.toc.entries[2].title, "Chapter 2");
    }

    #[test]
    fn test_empty_html_toc_fallback() {
        let html = "<p>Just a paragraph, no headings.</p>";
        let mut book = BookDocument::new();
        book.metadata.set_title("Fallback Title");
        build_toc_from_headings(html, &mut book);
        assert_eq!(book.toc.entries.len(), 1);
        assert_eq!(book.toc.entries[0].title, "Fallback Title");
    }
}
