//! HTML input plugin — reads a single HTML file into BookDocument.

use std::path::Path;

use convert_core::book::{
    BookDocument, EbookFormat, ManifestData, ManifestItem, TocEntry,
};
use convert_core::error::{ConvertError, Result};
use convert_core::options::ConversionOptions;
use convert_core::plugin::InputPlugin;

pub struct HtmlInputPlugin;

impl InputPlugin for HtmlInputPlugin {
    fn name(&self) -> &str {
        "HTML Input"
    }

    fn supported_formats(&self) -> &[EbookFormat] {
        &[EbookFormat::Html, EbookFormat::Xhtml]
    }

    fn convert(&self, input_path: &Path, _options: &ConversionOptions) -> Result<BookDocument> {
        log::info!("Reading HTML: {}", input_path.display());

        let content = std::fs::read_to_string(input_path)
            .map_err(|e| ConvertError::Html(format!("Cannot read {}: {}", input_path.display(), e)))?;

        let mut book = BookDocument::new();
        book.base_path = input_path.parent().map(|p| p.to_path_buf());

        // Extract title from <title> tag if present
        let title = extract_title(&content)
            .unwrap_or_else(|| {
                input_path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("Untitled")
                    .to_string()
            });
        book.metadata.set_title(&title);
        book.metadata.set("language", "en");

        // Scan for referenced resources (CSS, images) in the same directory
        if let Some(base_dir) = book.base_path.clone() {
            collect_resources(&base_dir, &content, &mut book);
        }

        // Wrap in proper XHTML 1.1 if needed
        let xhtml = if content.contains("<html") {
            content
        } else {
            convert_utils::xml::xhtml11_document(&title, "en", None, &content)
        };

        let item = ManifestItem::new(
            "content",
            "content.xhtml",
            "application/xhtml+xml",
            ManifestData::Xhtml(xhtml),
        );
        book.manifest.add(item);
        book.spine.push("content", true);
        book.toc.add(TocEntry::new(&title, "content.xhtml"));

        Ok(book)
    }
}

fn extract_title(html: &str) -> Option<String> {
    let lower = html.to_lowercase();
    let start = lower.find("<title>")?;
    let after = &html[start + 7..];
    let end = after.to_lowercase().find("</title>")?;
    let title = after[..end].trim().to_string();
    if title.is_empty() { None } else { Some(title) }
}

/// Collect CSS and image files referenced in the HTML.
fn collect_resources(base_dir: &Path, html: &str, book: &mut BookDocument) {
    // Find CSS links
    let css_re = regex::Regex::new(r#"(?i)<link[^>]+href\s*=\s*["']([^"']+\.css)["']"#).unwrap();
    for cap in css_re.captures_iter(html) {
        let href = &cap[1];
        let file_path = base_dir.join(href);
        if file_path.exists() {
            if let Ok(css_content) = std::fs::read_to_string(&file_path) {
                let id = book.manifest.generate_id("css");
                let item = ManifestItem::new(id, href, "text/css", ManifestData::Css(css_content));
                book.manifest.add(item);
            }
        }
    }

    // Find images
    let img_re = regex::Regex::new(r#"(?i)<img[^>]+src\s*=\s*["']([^"']+)["']"#).unwrap();
    for cap in img_re.captures_iter(html) {
        let src = &cap[1];
        // Skip data URIs and remote URLs
        if src.starts_with("data:") || src.starts_with("http") {
            continue;
        }
        let file_path = base_dir.join(src);
        if file_path.exists() {
            if let Ok(data) = std::fs::read(&file_path) {
                let mime = convert_utils::mime::mime_from_path(&file_path);
                let id = book.manifest.generate_id("img");
                let item = ManifestItem::new(id, src, mime, ManifestData::Binary(data));
                book.manifest.add(item);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_title() {
        assert_eq!(extract_title("<html><head><title>My Book</title></head>"), Some("My Book".to_string()));
        assert_eq!(extract_title("<html><head></head>"), None);
        assert_eq!(extract_title("<title></title>"), None);
    }
}
