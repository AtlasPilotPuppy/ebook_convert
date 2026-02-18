//! Ebook conversion transforms — applied between input and output.

pub mod merge_metadata;
pub mod detect_structure;
pub mod css_flattener;
pub mod image_rescale;
pub mod manifest_trimmer;

use convert_core::plugin::Transform;

/// Return the standard transform chain for PDF→EPUB conversion.
pub fn standard_transforms() -> Vec<Box<dyn Transform>> {
    vec![
        Box::new(merge_metadata::MergeMetadata),
        Box::new(detect_structure::DetectStructure),
        Box::new(css_flattener::CssFlattener),
        Box::new(image_rescale::ImageRescale),
        Box::new(manifest_trimmer::ManifestTrimmer),
    ]
}
