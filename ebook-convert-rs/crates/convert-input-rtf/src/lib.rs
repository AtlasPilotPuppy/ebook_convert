//! RTF input plugin — reads Rich Text Format files into BookDocument.
//!
//! Uses the `rtf-parser` crate for tokenization and parsing, then converts
//! the styled blocks into HTML content.

use std::path::Path;

use convert_core::book::{BookDocument, EbookFormat, ManifestData, ManifestItem, TocEntry};
use convert_core::error::{ConvertError, Result};
use convert_core::options::ConversionOptions;
use convert_core::plugin::InputPlugin;
use regex::Regex;
use rtf_parser::{Lexer, Parser};

pub struct RtfInputPlugin;

impl InputPlugin for RtfInputPlugin {
    fn name(&self) -> &str {
        "RTF Input"
    }

    fn supported_formats(&self) -> &[EbookFormat] {
        &[EbookFormat::Rtf]
    }

    fn convert(&self, input_path: &Path, _options: &ConversionOptions) -> Result<BookDocument> {
        log::info!("Reading RTF: {}", input_path.display());
        parse_rtf(input_path)
    }
}

fn parse_rtf(path: &Path) -> Result<BookDocument> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| ConvertError::Rtf(format!("Cannot read {}: {}", path.display(), e)))?;

    let tokens = Lexer::scan(&content)
        .map_err(|e| ConvertError::Rtf(format!("RTF lexer error: {:?}", e)))?;

    let doc = Parser::new(tokens)
        .parse()
        .map_err(|e| ConvertError::Rtf(format!("RTF parser error: {:?}", e)))?;

    let mut book = BookDocument::new();
    book.base_path = path.parent().map(|p| p.to_path_buf());

    // Extract title from filename
    let title = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Untitled")
        .to_string();
    book.metadata.set_title(&title);

    // Convert styled blocks to HTML
    let html = blocks_to_html(&doc);

    // Wrap in XHTML
    let xhtml = convert_utils::xml::xhtml11_document(&title, "en", Some("style.css"), &html);

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
h1, h2, h3 { text-indent: 0; margin: 1em 0 0.5em; }
.center { text-align: center; }
.right { text-align: right; }
.justify { text-align: justify; }"#;
    let css_item = ManifestItem::new(
        "style",
        "style.css",
        "text/css",
        ManifestData::Css(css.to_string()),
    );
    book.manifest.add(css_item);

    // Build TOC from headings in the HTML
    build_toc(&html, &mut book);

    log::info!("Parsed RTF: \"{}\"", title);

    Ok(book)
}

/// Convert rtf-parser's styled blocks into HTML.
fn blocks_to_html(doc: &rtf_parser::RtfDocument) -> String {
    let mut html = String::new();
    let mut in_para = false;
    for block in &doc.body {
        let text = &block.text;
        let painter = &block.painter;

        // Handle paragraph breaks
        if text == "\n" || text == "\r\n" {
            if in_para {
                html.push_str("</p>\n");
                in_para = false;
            }
            continue;
        }

        if text.trim().is_empty() && !in_para {
            continue;
        }

        // Start new paragraph if needed
        if !in_para {
            let align_class = match block.paragraph.alignment {
                rtf_parser::Alignment::Center => " class=\"center\"",
                rtf_parser::Alignment::RightAligned => " class=\"right\"",
                rtf_parser::Alignment::Justify => " class=\"justify\"",
                _ => "",
            };
            html.push_str(&format!("<p{}>", align_class));
            in_para = true;
        }

        // Apply character formatting
        let escaped = convert_utils::xml::escape_xml_text(text);
        let mut formatted = escaped.to_string();

        if painter.bold {
            formatted = format!("<strong>{}</strong>", formatted);
        }
        if painter.italic {
            formatted = format!("<em>{}</em>", formatted);
        }
        if painter.underline {
            formatted = format!("<u>{}</u>", formatted);
        }
        if painter.strike {
            formatted = format!("<del>{}</del>", formatted);
        }
        if painter.superscript {
            formatted = format!("<sup>{}</sup>", formatted);
        }
        if painter.subscript {
            formatted = format!("<sub>{}</sub>", formatted);
        }
        if painter.smallcaps {
            formatted = format!(
                r#"<span style="font-variant: small-caps">{}</span>"#,
                formatted
            );
        }

        html.push_str(&formatted);
    }

    if in_para {
        html.push_str("</p>\n");
    }

    html
}

/// Build TOC from heading-like content in HTML.
fn build_toc(html: &str, book: &mut BookDocument) {
    let heading_re = Regex::new(r"(?i)<h([1-3])[^>]*>(.*?)</h[1-3]>").unwrap();
    let tag_re = Regex::new(r"<[^>]+>").unwrap();

    let mut found = false;
    for cap in heading_re.captures_iter(html) {
        let title = tag_re.replace_all(&cap[2], "").trim().to_string();
        if !title.is_empty() {
            book.toc.add(TocEntry::new(&title, "content.xhtml"));
            found = true;
        }
    }

    if !found {
        if let Some(title) = book.metadata.title() {
            book.toc.add(TocEntry::new(title, "content.xhtml"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_rtf() {
        let rtf = r"{\rtf1\ansi{\fonttbl\f0 Times New Roman;}
\f0\fs24 Hello, world! This is a test document.
\par Second paragraph here.
}";
        let dir = std::env::temp_dir().join("test_rtf");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.rtf");
        std::fs::write(&path, rtf).unwrap();

        let result = parse_rtf(&path).unwrap();
        assert_eq!(result.metadata.title().unwrap(), "test");

        let xhtml = result
            .manifest
            .by_id("content")
            .unwrap()
            .data
            .as_xhtml()
            .unwrap();
        assert!(xhtml.contains("Hello"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_parse_rtf_with_formatting() {
        let rtf = r"{\rtf1\ansi{\fonttbl\f0 Arial;}
\f0\fs24 Normal text \b bold text\b0  and \i italic text\i0.
\par Another paragraph.
}";
        let dir = std::env::temp_dir().join("test_rtf_fmt");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("formatted.rtf");
        std::fs::write(&path, rtf).unwrap();

        let result = parse_rtf(&path).unwrap();
        let xhtml = result
            .manifest
            .by_id("content")
            .unwrap()
            .data
            .as_xhtml()
            .unwrap();
        assert!(xhtml.contains("<strong>"));
        assert!(xhtml.contains("<em>"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_blocks_to_html_empty() {
        let doc = rtf_parser::RtfDocument {
            header: rtf_parser::RtfHeader::default(),
            body: vec![],
        };
        let html = blocks_to_html(&doc);
        assert!(html.is_empty());
    }

    #[test]
    fn test_build_toc_with_headings() {
        let html = "<h1>Chapter 1</h1><p>text</p><h2>Section 1.1</h2>";
        let mut book = BookDocument::new();
        book.metadata.set_title("Test");
        build_toc(html, &mut book);
        assert_eq!(book.toc.entries.len(), 2);
    }

    #[test]
    fn test_build_toc_fallback() {
        let html = "<p>No headings here.</p>";
        let mut book = BookDocument::new();
        book.metadata.set_title("My Book");
        build_toc(html, &mut book);
        assert_eq!(book.toc.entries.len(), 1);
        assert_eq!(book.toc.entries[0].title, "My Book");
    }
}
