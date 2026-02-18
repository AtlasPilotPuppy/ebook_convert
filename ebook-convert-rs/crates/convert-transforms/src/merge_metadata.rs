//! MergeMetadata transform — ensures metadata is complete.

use convert_core::book::BookDocument;
use convert_core::error::Result;
use convert_core::options::ConversionOptions;
use convert_core::plugin::Transform;

/// Ensures the book has required metadata fields.
pub struct MergeMetadata;

impl Transform for MergeMetadata {
    fn name(&self) -> &str {
        "MergeMetadata"
    }

    fn apply(&self, book: &mut BookDocument, _options: &ConversionOptions) -> Result<()> {
        // Ensure title exists
        if book.metadata.title().is_none() {
            book.metadata.set_title("Untitled");
        }

        // Ensure language exists
        if !book.metadata.contains("language") {
            book.metadata.set("language", "en");
        }

        // Generate a UID if missing
        if book.uid.is_none() {
            book.uid = Some(format!("urn:uuid:{}", uuid::Uuid::new_v4()));
        }

        log::info!("Metadata merged: title={:?}", book.metadata.title());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_merge_metadata_fills_defaults() {
        let mut book = BookDocument::new();
        let opts = ConversionOptions::default();
        MergeMetadata.apply(&mut book, &opts).unwrap();

        assert_eq!(book.metadata.title(), Some("Untitled"));
        assert!(book.metadata.contains("language"));
        assert!(book.uid.is_some());
    }

    #[test]
    fn test_merge_metadata_preserves_existing() {
        let mut book = BookDocument::new();
        book.metadata.set_title("My Book");
        book.metadata.set("language", "fr");
        book.uid = Some("existing-uid".to_string());

        let opts = ConversionOptions::default();
        MergeMetadata.apply(&mut book, &opts).unwrap();

        assert_eq!(book.metadata.title(), Some("My Book"));
        assert_eq!(book.metadata.language(), Some("fr"));
        assert_eq!(book.uid.as_deref(), Some("existing-uid"));
    }
}
