//! TXT output plugin — serializes BookDocument to plain text.

use std::path::Path;

use convert_core::book::{BookDocument, EbookFormat, ManifestData};
use convert_core::error::{ConvertError, Result};
use convert_core::options::ConversionOptions;
use convert_core::plugin::OutputPlugin;

use regex::Regex;

pub struct TxtOutputPlugin;

impl OutputPlugin for TxtOutputPlugin {
    fn name(&self) -> &str {
        "TXT Output"
    }

    fn output_format(&self) -> EbookFormat {
        EbookFormat::Txt
    }

    fn convert(
        &self,
        book: &BookDocument,
        output_path: &Path,
        _options: &ConversionOptions,
    ) -> Result<()> {
        log::info!("Writing TXT: {}", output_path.display());

        let mut text = String::new();

        // Title
        if let Some(title) = book.metadata.title() {
            text.push_str(title);
            text.push('\n');
            for _ in 0..title.len() {
                text.push('=');
            }
            text.push_str("\n\n");
        }

        // Author
        for author in book.metadata.authors() {
            text.push_str("By ");
            text.push_str(author);
            text.push('\n');
        }
        if !book.metadata.authors().is_empty() {
            text.push('\n');
        }

        // Content from spine
        let tag_re = Regex::new(r"<[^>]+>").unwrap();
        let whitespace_re = Regex::new(r"\n{3,}").unwrap();

        for spine_item in book.spine.iter() {
            if let Some(manifest_item) = book.manifest.by_id(&spine_item.idref) {
                if let ManifestData::Xhtml(ref xhtml) = manifest_item.data {
                    let body = extract_body_text(xhtml);
                    // Strip HTML tags
                    let plain = tag_re.replace_all(&body, "");
                    // Decode entities
                    let plain = plain
                        .replace("&amp;", "&")
                        .replace("&lt;", "<")
                        .replace("&gt;", ">")
                        .replace("&quot;", "\"")
                        .replace("&#39;", "'")
                        .replace("&nbsp;", " ");
                    let plain = whitespace_re.replace_all(&plain, "\n\n");
                    let plain = plain.trim();
                    if !plain.is_empty() {
                        text.push_str(plain);
                        text.push_str("\n\n");
                    }
                }
            }
        }

        std::fs::write(output_path, text.trim_end())
            .map_err(|e| ConvertError::Other(format!("Failed to write TXT: {}", e)))?;

        Ok(())
    }
}

/// Extract text content from the body of XHTML, inserting newlines for block elements.
fn extract_body_text(xhtml: &str) -> String {
    // Find body content
    let lower = xhtml.to_lowercase();
    let body = if let Some(start) = lower.find("<body") {
        let after = xhtml[start..].find('>').unwrap_or(0);
        let end = lower.rfind("</body>").unwrap_or(xhtml.len());
        &xhtml[start + after + 1..end]
    } else {
        xhtml
    };

    // Insert newlines before/after block elements for readability
    let block_re = Regex::new(r"(?i)</?(p|div|h[1-6]|br|li|tr|blockquote|pre)[^>]*>").unwrap();
    block_re.replace_all(body, "\n").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use convert_core::book::{ManifestItem, TocEntry};

    #[test]
    fn test_txt_output() {
        let mut book = BookDocument::new();
        book.metadata.set_title("My Book");
        book.metadata.add("creator", "Alice");

        let xhtml = "<html><body><h1>Chapter 1</h1><p>Hello world.</p><p>Second para.</p></body></html>".to_string();
        let item = ManifestItem::new("ch1", "ch1.xhtml", "application/xhtml+xml", ManifestData::Xhtml(xhtml));
        book.manifest.add(item);
        book.spine.push("ch1", true);
        book.toc.add(TocEntry::new("Chapter 1", "ch1.xhtml"));

        let tmp = std::env::temp_dir().join("test_output.txt");
        let opts = ConversionOptions::default();
        TxtOutputPlugin.convert(&book, &tmp, &opts).unwrap();

        let content = std::fs::read_to_string(&tmp).unwrap();
        assert!(content.contains("My Book"));
        assert!(content.contains("By Alice"));
        assert!(content.contains("Hello world."));
        std::fs::remove_file(&tmp).ok();
    }
}
