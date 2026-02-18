//! DOCX input plugin — reads Word documents into BookDocument.
//!
//! DOCX is a ZIP archive containing Office Open XML. This plugin:
//! - Parses `docProps/core.xml` for metadata (title, author, description)
//! - Converts `word/document.xml` paragraphs and runs to HTML
//! - Extracts images from `word/media/`
//! - Handles basic styling (bold, italic, underline, headings, lists)
//! - Converts tables to HTML tables

mod document;
mod metadata;
mod styles;

use std::io::Read;
use std::path::Path;

use convert_core::book::{
    BookDocument, EbookFormat, ManifestData, ManifestItem, TocEntry,
};
use convert_core::error::{ConvertError, Result};
use convert_core::options::ConversionOptions;
use convert_core::plugin::InputPlugin;

pub struct DocxInputPlugin;

impl InputPlugin for DocxInputPlugin {
    fn name(&self) -> &str {
        "DOCX Input"
    }

    fn supported_formats(&self) -> &[EbookFormat] {
        &[EbookFormat::Docx]
    }

    fn convert(&self, input_path: &Path, _options: &ConversionOptions) -> Result<BookDocument> {
        log::info!("Reading DOCX: {}", input_path.display());
        parse_docx(input_path)
    }
}

fn parse_docx(path: &Path) -> Result<BookDocument> {
    let file = std::fs::File::open(path)
        .map_err(|e| ConvertError::Docx(format!("Cannot open {}: {}", path.display(), e)))?;

    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| ConvertError::Docx(format!("Invalid DOCX ZIP: {}", e)))?;

    let mut book = BookDocument::new();
    book.base_path = path.parent().map(|p| p.to_path_buf());

    // -- Metadata from docProps/core.xml --
    if let Ok(meta) = read_zip_string(&mut archive, "docProps/core.xml") {
        metadata::parse_core_metadata(&meta, &mut book);
    }

    // Fallback title from filename
    if book.metadata.title().is_none() {
        let title = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Untitled");
        book.metadata.set_title(title);
    }
    if !book.metadata.contains("language") {
        book.metadata.set("language", "en");
    }

    // -- Read relationships for image mapping --
    let rels = if let Ok(rels_xml) = read_zip_string(&mut archive, "word/_rels/document.xml.rels") {
        document::parse_relationships(&rels_xml)
    } else {
        std::collections::HashMap::new()
    };

    // -- Read styles for heading detection --
    let style_map = if let Ok(styles_xml) = read_zip_string(&mut archive, "word/styles.xml") {
        styles::parse_styles(&styles_xml)
    } else {
        std::collections::HashMap::new()
    };

    // -- Read numbering definitions for list detection --
    let numbering_map = if let Ok(num_xml) = read_zip_string(&mut archive, "word/numbering.xml") {
        styles::parse_numbering(&num_xml)
    } else {
        std::collections::HashMap::new()
    };

    // -- Extract images from word/media/ --
    let image_names: Vec<String> = archive
        .file_names()
        .filter(|n| n.starts_with("word/media/"))
        .map(|n| n.to_string())
        .collect();

    for img_name in &image_names {
        if let Ok(data) = read_zip_binary(&mut archive, img_name) {
            let filename = img_name
                .strip_prefix("word/")
                .unwrap_or(img_name);
            let mime = convert_utils::mime::mime_from_path(Path::new(filename));
            let id = book.manifest.generate_id("img");
            let item = ManifestItem::new(id, filename, mime, ManifestData::Binary(data));
            book.manifest.add(item);
        }
    }

    // -- Convert document.xml to HTML --
    let doc_xml = read_zip_string(&mut archive, "word/document.xml")
        .map_err(|e| ConvertError::Docx(format!("Missing word/document.xml: {}", e)))?;

    let body_html = document::convert_document(&doc_xml, &rels, &style_map, &numbering_map);

    let title = book.metadata.title().unwrap_or("Untitled").to_string();
    let xhtml = convert_utils::xml::xhtml11_document(&title, "en", Some("style.css"), &body_html);

    let content_item = ManifestItem::new(
        "content",
        "content.xhtml",
        "application/xhtml+xml",
        ManifestData::Xhtml(xhtml),
    );
    book.manifest.add(content_item);
    book.spine.push("content", true);

    // -- Default stylesheet --
    let css = r#"body { font-family: serif; line-height: 1.6; margin: 1em; }
p { margin: 0.3em 0; }
h1 { font-size: 1.8em; margin: 1em 0 0.5em; }
h2 { font-size: 1.4em; margin: 0.8em 0 0.4em; }
h3 { font-size: 1.2em; margin: 0.6em 0 0.3em; }
h4, h5, h6 { font-size: 1.1em; margin: 0.5em 0 0.3em; }
table { border-collapse: collapse; margin: 0.5em 0; width: 100%; }
td, th { border: 1px solid #ccc; padding: 0.3em 0.5em; }
th { font-weight: bold; background: #f5f5f5; }
img { max-width: 100%; height: auto; }
ul, ol { margin: 0.5em 0; padding-left: 2em; }
blockquote { margin: 0.5em 1em; padding-left: 1em; border-left: 3px solid #ccc; }
.docx-center { text-align: center; }
.docx-right { text-align: right; }
.docx-justify { text-align: justify; }"#;
    let css_item = ManifestItem::new("style", "style.css", "text/css", ManifestData::Css(css.to_string()));
    book.manifest.add(css_item);

    // -- Build TOC from headings --
    build_toc(&body_html, &title, &mut book);

    let img_count = image_names.len();
    log::info!("Parsed DOCX: \"{}\" with {} images", title, img_count);

    Ok(book)
}

fn build_toc(html: &str, title: &str, book: &mut BookDocument) {
    let tag_re = regex::Regex::new(r"<[^>]+>").unwrap();

    // Rust regex doesn't support backreferences, so use separate patterns per level
    let patterns = [
        regex::Regex::new(r"(?i)<h1[^>]*>(.*?)</h1>").unwrap(),
        regex::Regex::new(r"(?i)<h2[^>]*>(.*?)</h2>").unwrap(),
        regex::Regex::new(r"(?i)<h3[^>]*>(.*?)</h3>").unwrap(),
    ];

    // Collect all headings with their byte positions for ordering
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
        book.toc.add(TocEntry::new(title, "content.xhtml"));
    }
}

fn read_zip_string(archive: &mut zip::ZipArchive<std::fs::File>, name: &str) -> std::result::Result<String, String> {
    let mut file = archive
        .by_name(name)
        .map_err(|e| format!("{}: {}", name, e))?;
    let mut s = String::new();
    file.read_to_string(&mut s)
        .map_err(|e| format!("{}: {}", name, e))?;
    Ok(s)
}

fn read_zip_binary(archive: &mut zip::ZipArchive<std::fs::File>, name: &str) -> std::result::Result<Vec<u8>, String> {
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
    fn test_build_toc_from_headings() {
        let html = "<h1>Chapter One</h1><p>text</p><h2>Section A</h2>";
        let mut book = BookDocument::new();
        build_toc(html, "Test", &mut book);
        assert_eq!(book.toc.entries.len(), 2);
        assert_eq!(book.toc.entries[0].title, "Chapter One");
    }

    #[test]
    fn test_build_toc_fallback() {
        let html = "<p>No headings here</p>";
        let mut book = BookDocument::new();
        build_toc(html, "My Document", &mut book);
        assert_eq!(book.toc.entries.len(), 1);
        assert_eq!(book.toc.entries[0].title, "My Document");
    }
}
