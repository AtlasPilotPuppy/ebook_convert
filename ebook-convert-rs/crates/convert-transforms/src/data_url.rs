//! DataURL resolver — extracts base64-encoded data URIs from XHTML and creates manifest items.

use base64::Engine;
use rayon::prelude::*;

use convert_core::book::{BookDocument, ManifestData, ManifestItem};
use convert_core::error::Result;
use convert_core::options::ConversionOptions;
use convert_core::plugin::Transform;
use regex::Regex;

/// Finds `<img src="data:...;base64,...">` in XHTML, decodes the data,
/// creates manifest items, and replaces the src attributes.
pub struct DataUrl;

impl Transform for DataUrl {
    fn name(&self) -> &str {
        "DataURL"
    }

    fn apply(&self, book: &mut BookDocument, _options: &ConversionOptions) -> Result<()> {
        let re = Regex::new(r#"src\s*=\s*["'](data:([^;]+);base64,([^"']+))["']"#).unwrap();

        // Collect XHTML items that contain data URIs
        let xhtml_items: Vec<(String, String)> = book
            .manifest
            .iter()
            .filter(|item| item.is_xhtml())
            .filter(|item| {
                item.data.as_xhtml().map(|x| x.contains("data:")).unwrap_or(false)
            })
            .map(|item| (item.id.clone(), item.data.as_xhtml().unwrap().to_string()))
            .collect();

        // Decode data URIs in parallel per XHTML item
        // Each produces: (id, new_xhtml, Vec<(mime, decoded_data, old_uri)>)
        let decoded_results: Vec<(String, String, Vec<(String, Vec<u8>)>)> = xhtml_items
            .into_par_iter()
            .map(|(id, xhtml)| {
                let mut new_xhtml = xhtml.clone();
                let mut decoded_items: Vec<(String, Vec<u8>)> = Vec::new();

                for cap in re.captures_iter(&xhtml) {
                    let full_data_uri = &cap[1];
                    let mime_type = cap[2].to_string();
                    let b64_data = &cap[3];

                    let decoded = match base64::engine::general_purpose::STANDARD.decode(b64_data) {
                        Ok(d) => d,
                        Err(_) => {
                            let cleaned: String = b64_data.chars().filter(|c| !c.is_whitespace()).collect();
                            match base64::engine::general_purpose::STANDARD.decode(&cleaned) {
                                Ok(d) => d,
                                Err(_) => continue,
                            }
                        }
                    };

                    // Use a placeholder that will be replaced with actual href
                    let placeholder = format!("__dataurl_placeholder_{}__", decoded_items.len());
                    new_xhtml = new_xhtml.replace(full_data_uri, &placeholder);
                    decoded_items.push((mime_type, decoded));
                }

                (id, new_xhtml, decoded_items)
            })
            .collect();

        // Apply results sequentially (needs &mut for generate_id/generate_href)
        let mut count = 0u32;
        for (id, mut new_xhtml, decoded_items) in decoded_results {
            for (i, (mime_type, decoded)) in decoded_items.into_iter().enumerate() {
                count += 1;
                let ext = mime_to_ext(&mime_type);
                let href = book.manifest.generate_href(&format!("data_image_{}", count), ext);
                let item_id = book.manifest.generate_id("dataimg");

                let placeholder = format!("__dataurl_placeholder_{}__", i);
                new_xhtml = new_xhtml.replace(&placeholder, &href);

                log::debug!("Extracted data URI → {}", href);
                book.manifest.add(ManifestItem::new(
                    item_id, href, mime_type, ManifestData::Binary(decoded),
                ));
            }

            if let Some(item_mut) = book.manifest.by_id_mut(&id) {
                item_mut.data = ManifestData::Xhtml(new_xhtml);
            }
        }

        if count > 0 {
            log::info!("Resolved {} data URIs into manifest items", count);
        }
        Ok(())
    }
}

fn mime_to_ext(mime: &str) -> &str {
    match mime {
        "image/png" => "png",
        "image/jpeg" | "image/jpg" => "jpg",
        "image/gif" => "gif",
        "image/svg+xml" => "svg",
        "image/webp" => "webp",
        "image/bmp" => "bmp",
        _ => "bin",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extracts_data_uri() {
        let mut book = BookDocument::new();
        // A tiny 1x1 red PNG as base64
        let b64 = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8/5+hHgAHggJ/PchI7wAAAABJRU5ErkJggg==";
        let xhtml = format!(
            r##"<html><body><img src="data:image/png;base64,{}"/></body></html>"##,
            b64
        );
        let item = ManifestItem::new(
            "ch1",
            "chapter1.xhtml",
            "application/xhtml+xml",
            ManifestData::Xhtml(xhtml),
        );
        book.manifest.add(item);
        book.spine.push("ch1", true);

        let opts = ConversionOptions::default();
        DataUrl.apply(&mut book, &opts).unwrap();

        // Should have 2 items now: the XHTML + the extracted image
        assert_eq!(book.manifest.len(), 2);
        // The XHTML should no longer contain "data:"
        let ch1 = book.manifest.by_id("ch1").unwrap();
        let content = ch1.data.as_xhtml().unwrap();
        assert!(!content.contains("data:image"));
        assert!(content.contains("data_image_"));
    }

    #[test]
    fn test_no_data_uris_unchanged() {
        let mut book = BookDocument::new();
        let xhtml = r#"<html><body><img src="image.png"/></body></html>"#.to_string();
        let item = ManifestItem::new(
            "ch1",
            "chapter1.xhtml",
            "application/xhtml+xml",
            ManifestData::Xhtml(xhtml),
        );
        book.manifest.add(item);

        let opts = ConversionOptions::default();
        DataUrl.apply(&mut book, &opts).unwrap();

        assert_eq!(book.manifest.len(), 1);
    }

    #[test]
    fn test_mime_to_ext() {
        assert_eq!(mime_to_ext("image/png"), "png");
        assert_eq!(mime_to_ext("image/jpeg"), "jpg");
        assert_eq!(mime_to_ext("image/svg+xml"), "svg");
        assert_eq!(mime_to_ext("application/octet-stream"), "bin");
    }
}
