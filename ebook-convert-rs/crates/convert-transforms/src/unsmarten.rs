//! UnsmartenPunctuation — converts typographic quotes/dashes/ellipsis to ASCII.

use convert_core::book::{BookDocument, ManifestData};
use convert_core::error::Result;
use convert_core::options::ConversionOptions;
use convert_core::plugin::Transform;

/// Replaces smart quotes, en/em dashes, and ellipsis with their ASCII equivalents.
pub struct UnsmartenPunctuation;

/// Character replacement pairs: (from, to).
const REPLACEMENTS: &[(&str, &str)] = &[
    ("\u{201c}", "\""), // left double quote
    ("\u{201d}", "\""), // right double quote
    ("\u{201e}", "\""), // double low-9 quote
    ("\u{2018}", "'"),  // left single quote
    ("\u{2019}", "'"),  // right single quote
    ("\u{201a}", "'"),  // single low-9 quote
    ("\u{2013}", "-"),  // en-dash
    ("\u{2014}", "--"), // em-dash
    ("\u{2026}", "..."), // ellipsis
];

impl Transform for UnsmartenPunctuation {
    fn name(&self) -> &str {
        "UnsmartenPunctuation"
    }

    fn should_run(&self, options: &ConversionOptions) -> bool {
        options.unsmarten_punctuation
    }

    fn apply(&self, book: &mut BookDocument, _options: &ConversionOptions) -> Result<()> {
        let mut count = 0u32;

        for item in book.manifest.iter_mut() {
            if !item.is_xhtml() {
                continue;
            }
            if let Some(xhtml) = item.data.as_xhtml() {
                let mut new_xhtml = xhtml.to_string();
                let mut changed = false;

                for &(from, to) in REPLACEMENTS {
                    if new_xhtml.contains(from) {
                        new_xhtml = new_xhtml.replace(from, to);
                        changed = true;
                    }
                }

                if changed {
                    item.data = ManifestData::Xhtml(new_xhtml);
                    count += 1;
                }
            }
        }

        if count > 0 {
            log::info!("Unsmartened punctuation in {} items", count);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use convert_core::book::ManifestItem;

    #[test]
    fn test_unsmarten_quotes() {
        let mut book = BookDocument::new();
        let xhtml =
            "<html><body><p>\u{201c}Hello,\u{201d} she said. \u{2018}World!\u{2019}</p></body></html>"
                .to_string();
        let item = ManifestItem::new(
            "ch1",
            "ch1.xhtml",
            "application/xhtml+xml",
            ManifestData::Xhtml(xhtml),
        );
        book.manifest.add(item);

        let mut opts = ConversionOptions::default();
        opts.unsmarten_punctuation = true;

        UnsmartenPunctuation.apply(&mut book, &opts).unwrap();

        let content = book
            .manifest
            .by_id("ch1")
            .unwrap()
            .data
            .as_xhtml()
            .unwrap();
        assert!(content.contains(r#""Hello,""#));
        assert!(content.contains("'World!'"));
    }

    #[test]
    fn test_unsmarten_dashes_and_ellipsis() {
        let mut book = BookDocument::new();
        let xhtml =
            "<html><body><p>A\u{2013}B and C\u{2014}D and more\u{2026}</p></body></html>"
                .to_string();
        let item = ManifestItem::new(
            "ch1",
            "ch1.xhtml",
            "application/xhtml+xml",
            ManifestData::Xhtml(xhtml),
        );
        book.manifest.add(item);

        let mut opts = ConversionOptions::default();
        opts.unsmarten_punctuation = true;

        UnsmartenPunctuation.apply(&mut book, &opts).unwrap();

        let content = book
            .manifest
            .by_id("ch1")
            .unwrap()
            .data
            .as_xhtml()
            .unwrap();
        assert!(content.contains("A-B"));
        assert!(content.contains("C--D"));
        assert!(content.contains("more..."));
    }

    #[test]
    fn test_should_run() {
        let mut opts = ConversionOptions::default();
        assert!(!UnsmartenPunctuation.should_run(&opts));
        opts.unsmarten_punctuation = true;
        assert!(UnsmartenPunctuation.should_run(&opts));
    }

    #[test]
    fn test_no_smart_quotes_unchanged() {
        let mut book = BookDocument::new();
        let xhtml = "<html><body><p>Plain ASCII text</p></body></html>".to_string();
        let item = ManifestItem::new(
            "ch1",
            "ch1.xhtml",
            "application/xhtml+xml",
            ManifestData::Xhtml(xhtml.clone()),
        );
        book.manifest.add(item);

        let mut opts = ConversionOptions::default();
        opts.unsmarten_punctuation = true;

        UnsmartenPunctuation.apply(&mut book, &opts).unwrap();

        let content = book
            .manifest
            .by_id("ch1")
            .unwrap()
            .data
            .as_xhtml()
            .unwrap();
        assert_eq!(content, &xhtml);
    }
}
