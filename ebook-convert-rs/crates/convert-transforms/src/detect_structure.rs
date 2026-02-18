//! DetectStructure transform — detects chapters, headings, and builds TOC.

use rayon::prelude::*;

use convert_core::book::{BookDocument, TocEntry};
use convert_core::error::Result;
use convert_core::options::ConversionOptions;
use convert_core::plugin::Transform;

use regex::Regex;

/// Detects document structure: chapter breaks, headings, and TOC entries.
pub struct DetectStructure;

impl Transform for DetectStructure {
    fn name(&self) -> &str {
        "DetectStructure"
    }

    fn apply(&self, book: &mut BookDocument, options: &ConversionOptions) -> Result<()> {
        // If TOC already has entries (e.g., from input plugin), skip detection
        if !book.toc.entries.is_empty() {
            log::info!("TOC already has {} entries, skipping structure detection", book.toc.entries.len());
            return Ok(());
        }

        let chapter_re = options
            .chapter_regex
            .as_deref()
            .and_then(|r| Regex::new(r).ok());

        // Collect XHTML items for parallel heading extraction
        let xhtml_items: Vec<(String, String)> = book.manifest.iter()
            .filter(|item| item.is_xhtml())
            .filter_map(|item| item.data.as_xhtml().map(|x| (item.href.clone(), x.to_string())))
            .collect();

        // Extract headings in parallel
        let all_headings: Vec<(String, Vec<(u8, String)>)> = xhtml_items.into_par_iter()
            .map(|(href, xhtml)| {
                let headings = extract_headings(&xhtml, chapter_re.as_ref());
                (href, headings)
            })
            .collect();

        // Build TOC entries sequentially
        for (href, headings) in all_headings {
            for (level, title) in headings {
                let entry_href = if level <= 2 {
                    href.clone()
                } else {
                    format!("{}#heading-{}", href, title.len())
                };
                let mut entry = TocEntry::new(&title, &entry_href);
                entry.klass = Some(format!("h{}", level));
                book.toc.add(entry);
            }
        }

        // If still no TOC entries, generate from spine
        if book.toc.entries.is_empty() {
            log::info!("No headings found, generating TOC from spine");
            for (i, spine_item) in book.spine.iter().enumerate() {
                if let Some(manifest_item) = book.manifest.by_id(&spine_item.idref) {
                    book.toc.add(TocEntry::new(
                        format!("Section {}", i + 1),
                        &manifest_item.href,
                    ));
                }
            }
        }

        book.toc.rationalize_play_orders();
        log::info!("Detected {} TOC entries", book.toc.entries.len());
        Ok(())
    }
}

/// Extract heading text from XHTML content.
/// Returns (heading_level, title_text) pairs.
fn extract_headings(xhtml: &str, chapter_re: Option<&Regex>) -> Vec<(u8, String)> {
    let mut headings = Vec::new();
    let tag_re = Regex::new(r"<[^>]+>").unwrap();

    // Match each heading level separately (regex crate doesn't support backreferences)
    for level in 1u8..=6 {
        let pattern = format!(r"(?i)<h{}[^>]*>(.*?)</h{}>", level, level);
        let heading_re = Regex::new(&pattern).unwrap();

        for cap in heading_re.captures_iter(xhtml) {
            let raw_title = &cap[1];

            // Strip HTML tags from title
            let title = tag_re.replace_all(raw_title, "").trim().to_string();

            if title.is_empty() {
                continue;
            }

            // If chapter regex is set, only include matching headings
            if let Some(re) = chapter_re {
                if !re.is_match(&title) {
                    continue;
                }
            }

            // Store with byte offset for ordering
            let offset = cap.get(0).map(|m| m.start()).unwrap_or(0);
            headings.push((offset, level, title));
        }
    }

    // Sort by position in document
    headings.sort_by_key(|(offset, _, _)| *offset);
    headings.into_iter().map(|(_, level, title)| (level, title)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_headings() {
        let xhtml = r#"
            <html><body>
            <h1>Chapter 1: Introduction</h1>
            <p>Some text</p>
            <h2>Section 1.1</h2>
            <h3>Sub <em>section</em></h3>
            </body></html>
        "#;
        let headings = extract_headings(xhtml, None);
        assert_eq!(headings.len(), 3);
        assert_eq!(headings[0], (1, "Chapter 1: Introduction".to_string()));
        assert_eq!(headings[1], (2, "Section 1.1".to_string()));
        assert_eq!(headings[2], (3, "Sub section".to_string()));
    }

    #[test]
    fn test_extract_headings_with_regex() {
        let xhtml = "<html><body><h1>Chapter 1</h1><h1>Preface</h1></body></html>";
        let re = Regex::new(r"^Chapter").unwrap();
        let headings = extract_headings(xhtml, Some(&re));
        assert_eq!(headings.len(), 1);
        assert_eq!(headings[0].1, "Chapter 1");
    }
}
