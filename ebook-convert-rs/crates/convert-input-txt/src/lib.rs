//! TXT/Markdown input plugin — reads plain text or Markdown into BookDocument.

use std::path::Path;

use convert_core::book::{BookDocument, EbookFormat, ManifestData, ManifestItem, TocEntry};
use convert_core::error::{ConvertError, Result};
use convert_core::options::ConversionOptions;
use convert_core::plugin::InputPlugin;

pub struct TxtInputPlugin;

impl InputPlugin for TxtInputPlugin {
    fn name(&self) -> &str {
        "TXT Input"
    }

    fn supported_formats(&self) -> &[EbookFormat] {
        &[EbookFormat::Txt, EbookFormat::Markdown]
    }

    fn convert(&self, input_path: &Path, _options: &ConversionOptions) -> Result<BookDocument> {
        log::info!("Reading text: {}", input_path.display());

        let content = std::fs::read_to_string(input_path).map_err(|e| {
            ConvertError::Other(format!("Cannot read {}: {}", input_path.display(), e))
        })?;

        let ext = input_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("txt");

        let is_markdown = matches!(ext.to_lowercase().as_str(), "md" | "markdown");

        let title = input_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Untitled")
            .to_string();

        let mut book = BookDocument::new();
        book.metadata.set_title(&title);
        book.metadata.set("language", "en");

        let xhtml = if is_markdown {
            markdown_to_xhtml(&title, &content)
        } else {
            plaintext_to_xhtml(&title, &content)
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

        // Add default stylesheet
        let css =
            "body { font-family: serif; line-height: 1.6; margin: 1em; }\np { margin: 0.5em 0; }";
        let css_item = ManifestItem::new(
            "style",
            "style.css",
            "text/css",
            ManifestData::Css(css.to_string()),
        );
        book.manifest.add(css_item);

        Ok(book)
    }
}

/// Convert Markdown to XHTML using pulldown-cmark.
fn markdown_to_xhtml(title: &str, markdown: &str) -> String {
    use pulldown_cmark::{html, Parser};

    let parser = Parser::new(markdown);
    let mut html_output = String::new();
    html::push_html(&mut html_output, parser);

    convert_utils::xml::xhtml11_document(title, "en", Some("style.css"), &html_output)
}

/// Convert plain text to XHTML with paragraph detection.
fn plaintext_to_xhtml(title: &str, text: &str) -> String {
    let mut body = String::new();
    for para in text.split("\n\n") {
        let para = para.trim();
        if !para.is_empty() {
            let escaped = para
                .replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;")
                .replace('\n', "<br/>\n");
            body.push_str(&format!("<p>{}</p>\n", escaped));
        }
    }

    convert_utils::xml::xhtml11_document(title, "en", Some("style.css"), &body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plaintext_to_xhtml() {
        let xhtml = plaintext_to_xhtml("Test", "Hello World\n\nSecond paragraph");
        assert!(xhtml.contains("<title>Test</title>"));
        assert!(xhtml.contains("<p>Hello World</p>"));
        assert!(xhtml.contains("<p>Second paragraph</p>"));
    }

    #[test]
    fn test_markdown_to_xhtml() {
        let xhtml = markdown_to_xhtml("Test", "# Heading\n\nA **bold** paragraph.");
        assert!(xhtml.contains("<h1>Heading</h1>"));
        assert!(xhtml.contains("<strong>bold</strong>"));
    }
}
