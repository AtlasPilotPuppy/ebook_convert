//! SplitChapters transform — splits large XHTML documents at chapter boundaries.
//!
//! Calibre splits at `<h1>` and `<h2>` tags (configurable via `split_on_page_break`).
//! This produces multiple smaller XHTML files for better e-reader performance.

use rayon::prelude::*;

use convert_core::book::{BookDocument, ManifestData, ManifestItem, TocEntry};
use convert_core::error::Result;
use convert_core::options::ConversionOptions;
use convert_core::plugin::Transform;
use regex::Regex;

/// Minimum content size (bytes) to trigger splitting.
const MIN_SPLIT_SIZE: usize = 10_000;

/// Split large XHTML documents at heading boundaries into separate chapter files.
pub struct SplitChapters;

impl Transform for SplitChapters {
    fn name(&self) -> &str {
        "SplitChapters"
    }

    fn apply(&self, book: &mut BookDocument, _options: &ConversionOptions) -> Result<()> {
        // Collect spine items that are candidates for splitting
        let candidates: Vec<(String, String, String)> = book
            .spine
            .iter()
            .filter_map(|s| {
                book.manifest.by_id(&s.idref).and_then(|item| {
                    if item.is_xhtml() {
                        item.data.as_xhtml().and_then(|x| {
                            if x.len() >= MIN_SPLIT_SIZE {
                                Some((s.idref.clone(), item.href.clone(), x.to_string()))
                            } else {
                                None
                            }
                        })
                    } else {
                        None
                    }
                })
            })
            .collect();

        // Pre-compute chunk splits in parallel
        let split_results: Vec<(String, String, Vec<ContentChunk>)> = candidates
            .into_par_iter()
            .filter_map(|(idref, href, xhtml)| {
                let chunks = split_at_headings(&xhtml);
                if chunks.len() > 1 {
                    Some((idref, href, chunks))
                } else {
                    None
                }
            })
            .collect();

        // Apply splits sequentially (modifies spine, manifest, TOC)
        for (idref, original_href, chunks) in split_results {
            log::info!(
                "Splitting '{}' into {} chapters",
                original_href,
                chunks.len()
            );

            // Find spine position of this item
            let spine_pos = book.spine.iter().position(|s| s.idref == *idref).unwrap();

            // Remove original from spine (we'll replace it)
            book.spine.remove(&idref);

            // Create new manifest items for each chunk
            let mut new_ids: Vec<String> = Vec::new();
            for (i, chunk) in chunks.iter().enumerate() {
                let new_id = if i == 0 {
                    // Reuse the original ID for the first chunk
                    idref.clone()
                } else {
                    book.manifest.generate_id(&format!("{}_ch", idref))
                };

                let new_href = if i == 0 {
                    original_href.clone()
                } else {
                    let base = original_href.trim_end_matches(".xhtml");
                    book.manifest
                        .generate_href(&format!("{}_ch{}", base, i), "xhtml")
                };

                let xhtml_doc = wrap_body_xhtml(&chunk.body, &chunk.title);

                if i == 0 {
                    // Update existing manifest item
                    if let Some(item) = book.manifest.by_id_mut(&new_id) {
                        item.data = ManifestData::Xhtml(xhtml_doc);
                    }
                } else {
                    let item = ManifestItem::new(
                        &new_id,
                        &new_href,
                        "application/xhtml+xml",
                        ManifestData::Xhtml(xhtml_doc),
                    );
                    book.manifest.add(item);
                }

                new_ids.push(new_id);
            }

            // Insert all new IDs into spine at the original position
            for (i, new_id) in new_ids.iter().enumerate() {
                book.spine.insert(spine_pos + i, new_id, true);
            }

            // Update TOC entries: point to correct chapter files
            update_toc_hrefs(book, &original_href, &chunks, &new_ids);
        }

        Ok(())
    }
}

/// A chunk of content split from a larger document.
struct ContentChunk {
    /// The heading title (empty for the first chunk before any heading)
    title: String,
    /// The body HTML content (without <html>/<body> wrapper)
    body: String,
}

/// Split XHTML content at `<h1>`, `<h2>`, or page-break boundaries.
/// Returns a list of content chunks. The first chunk contains content before
/// the first split point (if any).
fn split_at_headings(xhtml: &str) -> Vec<ContentChunk> {
    // Extract body content
    let body_re = Regex::new(r"(?is)<body[^>]*>(.*)</body>").unwrap();
    let body_content = match body_re.captures(xhtml) {
        Some(cap) => cap[1].to_string(),
        None => {
            return vec![ContentChunk {
                title: String::new(),
                body: xhtml.to_string(),
            }]
        }
    };

    // Find split points at h1/h2 tags
    let heading_re = Regex::new(r"(?i)<h[12][^>]*>").unwrap();
    // Also split at page break divs (from MOBI mbp:pagebreak conversion)
    let pagebreak_re =
        Regex::new(r#"(?i)<div[^>]*class\s*=\s*["']mbp_pagebreak["'][^>]*>\s*</div>"#).unwrap();

    let title_re = Regex::new(r"(?is)<h[12][^>]*>(.*?)</h[12]>").unwrap();
    let tag_re = Regex::new(r"<[^>]+>").unwrap();

    // Collect all split positions (heading or page break)
    let mut split_positions: Vec<(usize, bool)> = Vec::new(); // (position, is_heading)
    for m in heading_re.find_iter(&body_content) {
        split_positions.push((m.start(), true));
    }
    for m in pagebreak_re.find_iter(&body_content) {
        // Split after the pagebreak div
        split_positions.push((m.end(), false));
    }
    split_positions.sort_by_key(|(pos, _)| *pos);
    split_positions.dedup_by_key(|(pos, _)| *pos);

    if split_positions.is_empty() {
        return vec![ContentChunk {
            title: String::new(),
            body: body_content,
        }];
    }

    let mut chunks = Vec::new();

    // First chunk: content before the first split point
    let first_content = &body_content[..split_positions[0].0];
    let first_trimmed = first_content.trim();
    if !first_trimmed.is_empty() && first_trimmed != "<br/>" {
        chunks.push(ContentChunk {
            title: String::new(),
            body: first_trimmed.to_string(),
        });
    }

    // Subsequent chunks: each starts at a split point
    for (i, &(start, _is_heading)) in split_positions.iter().enumerate() {
        let end = if i + 1 < split_positions.len() {
            split_positions[i + 1].0
        } else {
            body_content.len()
        };

        let chunk_html = &body_content[start..end];
        let trimmed = chunk_html.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Extract heading title if present
        let title = title_re
            .captures(trimmed)
            .map(|cap| tag_re.replace_all(&cap[1], "").trim().to_string())
            .unwrap_or_default();

        chunks.push(ContentChunk {
            title,
            body: trimmed.to_string(),
        });
    }

    // Don't split if we'd create too many tiny chunks (< 500 bytes each on average)
    let avg_size = body_content.len() / chunks.len().max(1);
    if avg_size < 500 && chunks.len() > 5 {
        return vec![ContentChunk {
            title: String::new(),
            body: body_content,
        }];
    }

    chunks
}

/// Wrap body HTML in a minimal XHTML document.
fn wrap_body_xhtml(body: &str, title: &str) -> String {
    let title_escaped =
        convert_utils::xml::escape_xml_text(if title.is_empty() { "Chapter" } else { title });
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE html PUBLIC "-//W3C//DTD XHTML 1.1//EN" "http://www.w3.org/TR/xhtml11/DTD/xhtml11.dtd">
<html xmlns="http://www.w3.org/1999/xhtml" xml:lang="en">
<head>
  <title>{}</title>
  <link rel="stylesheet" type="text/css" href="style.css"/>
</head>
<body>
{}
</body>
</html>"#,
        title_escaped, body
    )
}

/// Update TOC entry hrefs to point to the correct split chapter files.
fn update_toc_hrefs(
    book: &mut BookDocument,
    original_href: &str,
    chunks: &[ContentChunk],
    new_ids: &[String],
) {
    // Build a mapping: heading title → new href
    let mut title_to_href: Vec<(String, String)> = Vec::new();
    for (i, chunk) in chunks.iter().enumerate() {
        if !chunk.title.is_empty() {
            if let Some(item) = book.manifest.by_id(&new_ids[i]) {
                title_to_href.push((chunk.title.clone(), item.href.clone()));
            }
        }
    }

    // Update TOC entries that reference the original href
    for entry in &mut book.toc.entries {
        update_toc_entry(entry, original_href, &title_to_href);
    }
}

fn update_toc_entry(entry: &mut TocEntry, original_href: &str, title_to_href: &[(String, String)]) {
    // Strip fragment from href for comparison
    let entry_href_base = entry.href.split('#').next().unwrap_or(&entry.href);
    if entry_href_base == original_href {
        // Try to match by title
        for (title, new_href) in title_to_href {
            if entry.title == *title {
                entry.href = new_href.clone();
                break;
            }
        }
    }

    // Recurse into children
    for child in &mut entry.children {
        update_toc_entry(child, original_href, title_to_href);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use convert_core::book::BookDocument;

    #[test]
    fn test_split_at_headings_basic() {
        let xhtml = r#"<?xml version="1.0"?>
<html><body>
<h1>Chapter 1</h1>
<p>First chapter content.</p>
<h1>Chapter 2</h1>
<p>Second chapter content.</p>
</body></html>"#;

        let chunks = split_at_headings(xhtml);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].title, "Chapter 1");
        assert!(chunks[0].body.contains("First chapter content"));
        assert_eq!(chunks[1].title, "Chapter 2");
        assert!(chunks[1].body.contains("Second chapter content"));
    }

    #[test]
    fn test_split_at_headings_with_preamble() {
        let xhtml = r#"<html><body>
<p>Preamble text before any heading.</p>
<h1>Chapter 1</h1>
<p>Content.</p>
</body></html>"#;

        let chunks = split_at_headings(xhtml);
        assert_eq!(chunks.len(), 2);
        assert!(chunks[0].title.is_empty()); // preamble
        assert!(chunks[0].body.contains("Preamble"));
        assert_eq!(chunks[1].title, "Chapter 1");
    }

    #[test]
    fn test_split_at_headings_h2() {
        let xhtml = r#"<html><body>
<h2>Part A</h2>
<p>Content A.</p>
<h2>Part B</h2>
<p>Content B.</p>
</body></html>"#;

        let chunks = split_at_headings(xhtml);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].title, "Part A");
        assert_eq!(chunks[1].title, "Part B");
    }

    #[test]
    fn test_no_split_without_headings() {
        let xhtml = "<html><body><p>Just text.</p></body></html>";
        let chunks = split_at_headings(xhtml);
        assert_eq!(chunks.len(), 1);
    }

    #[test]
    fn test_split_at_page_breaks() {
        let xhtml = r#"<html><body>
<p>Chapter 1 content.</p>
<div class="mbp_pagebreak"></div>
<p>Chapter 2 content.</p>
<div class="mbp_pagebreak"></div>
<p>Chapter 3 content.</p>
</body></html>"#;

        let chunks = split_at_headings(xhtml);
        assert_eq!(chunks.len(), 3);
        assert!(chunks[0].body.contains("Chapter 1"));
        assert!(chunks[1].body.contains("Chapter 2"));
        assert!(chunks[2].body.contains("Chapter 3"));
    }

    #[test]
    fn test_wrap_body_xhtml() {
        let result = wrap_body_xhtml("<p>Hello</p>", "Test Title");
        assert!(result.contains("<title>Test Title</title>"));
        assert!(result.contains("<p>Hello</p>"));
        assert!(result.contains("xmlns=\"http://www.w3.org/1999/xhtml\""));
    }

    #[test]
    fn test_split_chapters_transform() {
        let mut book = BookDocument::new();
        book.metadata.set_title("Test Book");

        // Create a large XHTML document with chapters
        let mut body = String::new();
        for i in 1..=3 {
            body.push_str(&format!("<h1>Chapter {}</h1>\n", i));
            // Add enough content to exceed MIN_SPLIT_SIZE
            for _ in 0..100 {
                body.push_str(&format!("<p>Content for chapter {}. This is a paragraph of text that adds to the total size of the document.</p>\n", i));
            }
        }
        let xhtml = format!(
            r#"<?xml version="1.0"?><html xmlns="http://www.w3.org/1999/xhtml"><body>{}</body></html>"#,
            body
        );

        let item = ManifestItem::new(
            "content",
            "content.xhtml",
            "application/xhtml+xml",
            ManifestData::Xhtml(xhtml),
        );
        book.manifest.add(item);
        book.spine.push("content", true);

        // Add TOC entries
        book.toc.add(TocEntry::new("Chapter 1", "content.xhtml"));
        book.toc.add(TocEntry::new("Chapter 2", "content.xhtml"));
        book.toc.add(TocEntry::new("Chapter 3", "content.xhtml"));

        let transform = SplitChapters;
        let options = ConversionOptions::default();
        transform.apply(&mut book, &options).unwrap();

        // Should have split into 3 spine items
        assert_eq!(book.spine.len(), 3);

        // All spine items should have valid XHTML content
        for spine_item in book.spine.iter() {
            let item = book.manifest.by_id(&spine_item.idref).unwrap();
            assert!(item.data.as_xhtml().unwrap().contains("<body>"));
        }

        // TOC entries should be updated to point to different files
        let hrefs: Vec<&str> = book.toc.entries.iter().map(|e| e.href.as_str()).collect();
        // At least some should point to new chapter files
        assert!(hrefs.iter().any(|h| h.contains("_ch")));
    }
}
