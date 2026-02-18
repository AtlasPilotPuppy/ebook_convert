//! TOC (Table of Contents) generation from PDF outline/bookmarks.

use crate::pdftohtml::OutlineItem;
use convert_core::book::TocEntry;

/// Build a TOC from a PDF outline.
///
/// `page_href_map` maps 1-based page numbers to their XHTML href paths.
/// If the outline is empty or has fewer than `min_entries` items,
/// falls back to a simple page-number TOC.
pub fn build_toc(
    outline: &[OutlineItem],
    page_href_map: &std::collections::HashMap<u32, String>,
    total_pages: u32,
    min_entries: usize,
) -> Vec<TocEntry> {
    if outline.len() >= min_entries {
        let entries = outline_to_toc(outline, page_href_map);
        if !entries.is_empty() {
            return entries;
        }
    }

    // Fall back to page-number TOC
    build_page_number_toc(page_href_map, total_pages)
}

/// Convert outline items to TocEntry tree recursively.
fn outline_to_toc(
    items: &[OutlineItem],
    page_href_map: &std::collections::HashMap<u32, String>,
) -> Vec<TocEntry> {
    let mut entries = Vec::new();

    for item in items {
        let href = page_href_map
            .get(&item.page)
            .cloned()
            .unwrap_or_else(|| format!("page{}.xhtml", item.page));

        let mut entry = TocEntry::new(&item.title, &href);

        // Recursively add children
        if !item.children.is_empty() {
            for child in outline_to_toc(&item.children, page_href_map) {
                entry.add_child(child);
            }
        }

        entries.push(entry);
    }

    entries
}

/// Build a simple page-number-based TOC.
fn build_page_number_toc(
    page_href_map: &std::collections::HashMap<u32, String>,
    total_pages: u32,
) -> Vec<TocEntry> {
    let mut entries = Vec::new();

    for page_num in 1..=total_pages {
        if let Some(href) = page_href_map.get(&page_num) {
            entries.push(TocEntry::new(format!("Page {}", page_num), href));
        }
    }

    entries
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_page_map(n: u32) -> HashMap<u32, String> {
        (1..=n).map(|i| (i, format!("page{}.xhtml", i))).collect()
    }

    #[test]
    fn test_outline_to_toc() {
        let outline = vec![
            OutlineItem {
                title: "Introduction".to_string(),
                page: 1,
                children: vec![],
            },
            OutlineItem {
                title: "Chapter 1".to_string(),
                page: 5,
                children: vec![OutlineItem {
                    title: "Section 1.1".to_string(),
                    page: 7,
                    children: vec![],
                }],
            },
            OutlineItem {
                title: "Chapter 2".to_string(),
                page: 20,
                children: vec![],
            },
        ];

        let page_map = make_page_map(30);
        let toc = build_toc(&outline, &page_map, 30, 3);

        assert_eq!(toc.len(), 3);
        assert_eq!(toc[0].title, "Introduction");
        assert_eq!(toc[0].href, "page1.xhtml");
        assert_eq!(toc[1].title, "Chapter 1");
        assert_eq!(toc[1].children.len(), 1);
        assert_eq!(toc[1].children[0].title, "Section 1.1");
        assert_eq!(toc[1].children[0].href, "page7.xhtml");
    }

    #[test]
    fn test_fallback_to_page_numbers() {
        let outline = vec![OutlineItem {
            title: "Only One".to_string(),
            page: 1,
            children: vec![],
        }];

        let page_map = make_page_map(5);
        // min_entries=3, but outline only has 1 item → fallback
        let toc = build_toc(&outline, &page_map, 5, 3);

        assert_eq!(toc.len(), 5);
        assert_eq!(toc[0].title, "Page 1");
        assert_eq!(toc[4].title, "Page 5");
    }

    #[test]
    fn test_empty_outline_fallback() {
        let page_map = make_page_map(3);
        let toc = build_toc(&[], &page_map, 3, 3);

        assert_eq!(toc.len(), 3);
    }
}
