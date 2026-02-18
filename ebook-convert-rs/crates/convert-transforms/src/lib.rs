//! Ebook conversion transforms — applied between input and output.

pub mod data_url;
pub mod clean_guide;
pub mod merge_metadata;
pub mod detect_structure;
pub mod jacket;
pub mod linearize_tables;
pub mod unsmarten;
pub mod css_flattener;
pub mod page_margin;
pub mod image_rescale;
pub mod split_chapters;
pub mod manifest_trimmer;

use convert_core::plugin::Transform;

/// Return the standard transform chain in Calibre's execution order.
///
/// Conditional transforms use `should_run()` to skip when not needed.
/// Order:
///  1. DataURL              (always)
///  2. CleanGuide           (always)
///  3. MergeMetadata        (always)
///  4. DetectStructure      (always)
///  5. Jacket               (conditional: insert_metadata || remove_first_image)
///  6. LinearizeTables      (conditional: linearize_tables)
///  7. UnsmartenPunctuation (conditional: unsmarten_punctuation)
///  8. CSSFlattener         (always)
///  9. PageMargin           (always)
/// 10. ImageRescale         (always)
/// 11. SplitChapters        (always, splits large XHTML at heading boundaries)
/// 12. ManifestTrimmer      (always)
pub fn standard_transforms() -> Vec<Box<dyn Transform>> {
    vec![
        Box::new(data_url::DataUrl),
        Box::new(clean_guide::CleanGuide),
        Box::new(merge_metadata::MergeMetadata),
        Box::new(detect_structure::DetectStructure),
        Box::new(jacket::Jacket),
        Box::new(linearize_tables::LinearizeTables),
        Box::new(unsmarten::UnsmartenPunctuation),
        Box::new(css_flattener::CssFlattener),
        Box::new(page_margin::PageMargin),
        Box::new(image_rescale::ImageRescale),
        Box::new(split_chapters::SplitChapters),
        Box::new(manifest_trimmer::ManifestTrimmer),
    ]
}
