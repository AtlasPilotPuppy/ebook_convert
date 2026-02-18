//! HTML output plugin — serializes BookDocument to a single HTML file.

use std::path::Path;

use convert_core::book::{BookDocument, EbookFormat, ManifestData};
use convert_core::error::{ConvertError, Result};
use convert_core::options::ConversionOptions;
use convert_core::plugin::OutputPlugin;

pub struct HtmlOutputPlugin;

impl OutputPlugin for HtmlOutputPlugin {
    fn name(&self) -> &str {
        "HTML Output"
    }

    fn output_format(&self) -> EbookFormat {
        EbookFormat::Html
    }

    fn convert(
        &self,
        book: &BookDocument,
        output_path: &Path,
        _options: &ConversionOptions,
    ) -> Result<()> {
        log::info!("Writing HTML: {}", output_path.display());

        let title = book.metadata.title().unwrap_or("Untitled");
        let mut html = String::new();

        html.push_str("<!DOCTYPE html>\n<html>\n<head>\n");
        html.push_str(&format!(
            "<meta charset=\"UTF-8\">\n<title>{}</title>\n",
            convert_utils::xml::escape_xml_text(title)
        ));

        // Inline all CSS
        for item in book.manifest.iter() {
            if item.is_css() {
                if let Some(css) = item.data.as_css() {
                    html.push_str("<style>\n");
                    html.push_str(css);
                    html.push_str("\n</style>\n");
                }
            }
        }
        html.push_str("</head>\n<body>\n");

        // Concatenate all spine XHTML content
        for spine_item in book.spine.iter() {
            if let Some(manifest_item) = book.manifest.by_id(&spine_item.idref) {
                if let ManifestData::Xhtml(ref xhtml) = manifest_item.data {
                    // Extract body content
                    if let Some(body) = extract_body(xhtml) {
                        html.push_str(&body);
                        html.push('\n');
                    }
                }
            }
        }

        html.push_str("</body>\n</html>\n");

        // Write images to output directory
        if let Some(parent) = output_path.parent() {
            for item in book.manifest.iter() {
                if item.is_image() {
                    if let ManifestData::Binary(ref data) = item.data {
                        let img_path = parent.join(&item.href);
                        if let Some(img_parent) = img_path.parent() {
                            std::fs::create_dir_all(img_parent).ok();
                        }
                        std::fs::write(&img_path, data).ok();
                    }
                }
            }
        }

        std::fs::write(output_path, html)
            .map_err(|e| ConvertError::Other(format!("Failed to write HTML: {}", e)))?;

        Ok(())
    }
}

/// Extract content between <body> and </body> tags.
fn extract_body(xhtml: &str) -> Option<String> {
    let lower = xhtml.to_lowercase();
    let start = lower.find("<body")?;
    let after_tag = xhtml[start..].find('>')?;
    let body_start = start + after_tag + 1;

    let body_end = lower.rfind("</body>")?;
    if body_end > body_start {
        Some(xhtml[body_start..body_end].to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_body() {
        let xhtml = "<html><body><p>Hello</p></body></html>";
        assert_eq!(extract_body(xhtml), Some("<p>Hello</p>".to_string()));
    }

    #[test]
    fn test_extract_body_with_attrs() {
        let xhtml = r#"<html><body class="main"><p>Content</p></body></html>"#;
        assert_eq!(extract_body(xhtml), Some("<p>Content</p>".to_string()));
    }
}
