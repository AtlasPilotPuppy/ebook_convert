//! CleanGuide — normalizes guide references and detects cover images.

use convert_core::book::{BookDocument, GuideRef};
use convert_core::error::Result;
use convert_core::options::ConversionOptions;
use convert_core::plugin::Transform;

/// Standard EPUB guide reference types.
const VALID_GUIDE_TYPES: &[&str] = &[
    "cover",
    "title-page",
    "toc",
    "index",
    "glossary",
    "acknowledgements",
    "bibliography",
    "colophon",
    "copyright-page",
    "dedication",
    "epigraph",
    "foreword",
    "loi",
    "lot",
    "notes",
    "preface",
    "text",
];

/// Microsoft/Adobe cover image metadata type names.
const COVER_TYPE_ALIASES: &[&str] = &[
    "ms-coverimage-standard",
    "ms-titleimage-standard",
    "other.ms-coverimage-standard",
    "other.ms-titleimage-standard",
];

/// Normalizes guide references: detects cover, promotes start→text,
/// removes non-standard guide types.
pub struct CleanGuide;

impl Transform for CleanGuide {
    fn name(&self) -> &str {
        "CleanGuide"
    }

    fn apply(&self, book: &mut BookDocument, _options: &ConversionOptions) -> Result<()> {
        // 1. If no cover, try to find one from MS/Adobe metadata types
        if book.guide.get("cover").is_none() {
            let mut cover_href = None;
            for alias in COVER_TYPE_ALIASES {
                if let Some(guide_ref) = book.guide.get(alias) {
                    cover_href = Some(guide_ref.href.clone());
                    break;
                }
            }
            if let Some(href) = cover_href {
                book.guide.add(GuideRef::new("cover", "Cover", href));
                log::info!("Detected cover from MS/Adobe metadata");
            }
        }

        // 2. If start exists but no text, promote start → text
        if book.guide.get("text").is_none() {
            if let Some(start_ref) = book.guide.get("start") {
                let href = start_ref.href.clone();
                let title = start_ref.title.clone();
                book.guide.add(GuideRef::new("text", title, href));
                log::debug!("Promoted guide 'start' to 'text'");
            }
        }

        // 3. Remove non-standard guide types
        let to_remove: Vec<String> = book
            .guide
            .iter()
            .filter(|r| !VALID_GUIDE_TYPES.contains(&r.ref_type.as_str()))
            .map(|r| r.ref_type.clone())
            .collect();

        for ref_type in &to_remove {
            log::debug!("Removing non-standard guide type: {}", ref_type);
            book.guide.remove(ref_type);
        }

        if !to_remove.is_empty() {
            log::info!("Removed {} non-standard guide references", to_remove.len());
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cover_detection_from_ms_type() {
        let mut book = BookDocument::new();
        book.guide.add(GuideRef::new(
            "other.ms-coverimage-standard",
            "Cover",
            "cover.jpg",
        ));

        let opts = ConversionOptions::default();
        CleanGuide.apply(&mut book, &opts).unwrap();

        assert!(book.guide.get("cover").is_some());
        assert_eq!(book.guide.get("cover").unwrap().href, "cover.jpg");
    }

    #[test]
    fn test_start_promoted_to_text() {
        let mut book = BookDocument::new();
        book.guide
            .add(GuideRef::new("start", "Begin Reading", "chapter1.xhtml"));

        let opts = ConversionOptions::default();
        CleanGuide.apply(&mut book, &opts).unwrap();

        assert!(book.guide.get("text").is_some());
        assert_eq!(book.guide.get("text").unwrap().href, "chapter1.xhtml");
    }

    #[test]
    fn test_removes_nonstandard_types() {
        let mut book = BookDocument::new();
        book.guide
            .add(GuideRef::new("cover", "Cover", "cover.xhtml"));
        book.guide
            .add(GuideRef::new("custom-nonsense", "Nonsense", "foo.xhtml"));
        book.guide.add(GuideRef::new(
            "ms-coverimage-standard",
            "MS Cover",
            "cover.jpg",
        ));

        let opts = ConversionOptions::default();
        CleanGuide.apply(&mut book, &opts).unwrap();

        assert!(book.guide.get("cover").is_some());
        assert!(book.guide.get("custom-nonsense").is_none());
        assert!(book.guide.get("ms-coverimage-standard").is_none());
    }

    #[test]
    fn test_standard_types_preserved() {
        let mut book = BookDocument::new();
        book.guide
            .add(GuideRef::new("cover", "Cover", "cover.xhtml"));
        book.guide
            .add(GuideRef::new("toc", "Table of Contents", "toc.xhtml"));
        book.guide.add(GuideRef::new("text", "Start", "ch1.xhtml"));

        let opts = ConversionOptions::default();
        CleanGuide.apply(&mut book, &opts).unwrap();

        assert!(book.guide.get("cover").is_some());
        assert!(book.guide.get("toc").is_some());
        assert!(book.guide.get("text").is_some());
    }
}
