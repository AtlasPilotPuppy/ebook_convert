//! ManifestTrimmer — removes unreferenced items from the manifest.

use std::collections::HashSet;

use convert_core::book::BookDocument;
use convert_core::error::Result;
use convert_core::options::ConversionOptions;
use convert_core::plugin::Transform;

use regex::Regex;

/// Removes manifest items that are not referenced by the spine, TOC, or other items.
pub struct ManifestTrimmer;

impl Transform for ManifestTrimmer {
    fn name(&self) -> &str {
        "ManifestTrimmer"
    }

    fn apply(&self, book: &mut BookDocument, _options: &ConversionOptions) -> Result<()> {
        let mut referenced: HashSet<String> = HashSet::new();

        // All spine items are referenced
        for item in book.spine.iter() {
            referenced.insert(item.idref.clone());
        }

        // All TOC entries reference items
        for entry in book.toc.iter_depth_first() {
            // The href may contain a fragment; strip it
            let href = entry.href.split('#').next().unwrap_or(&entry.href);
            if let Some(item) = book.manifest.by_href(href) {
                referenced.insert(item.id.clone());
            }
        }

        // Guide references
        for guide_ref in book.guide.iter() {
            let href = guide_ref.href.split('#').next().unwrap_or(&guide_ref.href);
            if let Some(item) = book.manifest.by_href(href) {
                referenced.insert(item.id.clone());
            }
        }

        // Scan XHTML content for referenced resources (CSS, images, etc.)
        let href_re = Regex::new(r#"(?:src|href)\s*=\s*["']([^"']+)["']"#).unwrap();
        for item in book.manifest.iter() {
            if !item.is_xhtml() {
                continue;
            }
            if let Some(xhtml) = item.data.as_xhtml() {
                for cap in href_re.captures_iter(xhtml) {
                    let href = &cap[1];
                    if let Some(ref_item) = book.manifest.by_href(href) {
                        referenced.insert(ref_item.id.clone());
                    }
                }
            }
        }

        // Also scan CSS for url() references
        let url_re = Regex::new(r#"url\s*\(\s*['"]?([^'")\s]+)['"]?\s*\)"#).unwrap();
        for item in book.manifest.iter() {
            if !item.is_css() {
                continue;
            }
            if let Some(css) = item.data.as_css() {
                for cap in url_re.captures_iter(css) {
                    let href = &cap[1];
                    if let Some(ref_item) = book.manifest.by_href(href) {
                        referenced.insert(ref_item.id.clone());
                    }
                }
            }
        }

        // Collect IDs to remove
        let to_remove: Vec<String> = book
            .manifest
            .iter()
            .filter(|item| !referenced.contains(&item.id))
            .map(|item| item.id.clone())
            .collect();

        let removed_count = to_remove.len();
        for id in to_remove {
            log::debug!("Trimming unreferenced manifest item: {}", id);
            book.manifest.remove_by_id(&id);
        }

        log::info!("Trimmed {} unreferenced manifest items", removed_count);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use convert_core::book::{ManifestData, ManifestItem};

    #[test]
    fn test_trims_unreferenced() {
        let mut book = BookDocument::new();

        // Referenced item (in spine)
        let ch1 = ManifestItem::new(
            "ch1",
            "chapter1.xhtml",
            "application/xhtml+xml",
            ManifestData::Xhtml("<html><body><link href=\"style.css\"/></body></html>".to_string()),
        );
        book.manifest.add(ch1);
        book.spine.push("ch1", true);

        // Referenced via XHTML href
        let css = ManifestItem::new("style", "style.css", "text/css", ManifestData::Css("body {}".to_string()));
        book.manifest.add(css);

        // Unreferenced item
        let orphan = ManifestItem::new("orphan", "orphan.png", "image/png", ManifestData::Binary(vec![0]));
        book.manifest.add(orphan);

        assert_eq!(book.manifest.len(), 3);

        let opts = ConversionOptions::default();
        ManifestTrimmer.apply(&mut book, &opts).unwrap();

        assert_eq!(book.manifest.len(), 2);
        assert!(book.manifest.by_id("ch1").is_some());
        assert!(book.manifest.by_id("style").is_some());
        assert!(book.manifest.by_id("orphan").is_none());
    }
}
