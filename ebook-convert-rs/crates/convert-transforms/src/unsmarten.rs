//! UnsmartenPunctuation — converts typographic quotes/dashes/ellipsis to ASCII.

use rayon::prelude::*;

use convert_core::book::{BookDocument, ManifestData};
use convert_core::error::Result;
use convert_core::options::ConversionOptions;
use convert_core::plugin::Transform;

/// Replaces smart quotes, en/em dashes, and ellipsis with their ASCII equivalents.
pub struct UnsmartenPunctuation;

/// Character replacement pairs: (from, to).
const REPLACEMENTS: &[(&str, &str)] = &[
    ("\u{201c}", "\""),  // left double quote
    ("\u{201d}", "\""),  // right double quote
    ("\u{201e}", "\""),  // double low-9 quote
    ("\u{2018}", "'"),   // left single quote
    ("\u{2019}", "'"),   // right single quote
    ("\u{201a}", "'"),   // single low-9 quote
    ("\u{2013}", "-"),   // en-dash
    ("\u{2014}", "--"),  // em-dash
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
        // Collect XHTML items
        let xhtml_items: Vec<(String, String)> = book
            .manifest
            .iter()
            .filter(|item| item.is_xhtml())
            .filter_map(|item| {
                item.data
                    .as_xhtml()
                    .map(|x| (item.id.clone(), x.to_string()))
            })
            .collect();

        // Process in parallel
        let results: Vec<(String, String)> = xhtml_items
            .into_par_iter()
            .filter_map(|(id, xhtml)| {
                let mut new_xhtml = xhtml;
                let mut changed = false;
                for &(from, to) in REPLACEMENTS {
                    if new_xhtml.contains(from) {
                        new_xhtml = new_xhtml.replace(from, to);
                        changed = true;
                    }
                }
                if changed {
                    Some((id, new_xhtml))
                } else {
                    None
                }
            })
            .collect();

        // Apply back sequentially
        let count = results.len() as u32;
        for (id, new_xhtml) in results {
            if let Some(item) = book.manifest.by_id_mut(&id) {
                item.data = ManifestData::Xhtml(new_xhtml);
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

        let opts = ConversionOptions {
            unsmarten_punctuation: true,
            ..Default::default()
        };

        UnsmartenPunctuation.apply(&mut book, &opts).unwrap();

        let content = book.manifest.by_id("ch1").unwrap().data.as_xhtml().unwrap();
        assert!(content.contains(r#""Hello,""#));
        assert!(content.contains("'World!'"));
    }

    #[test]
    fn test_unsmarten_dashes_and_ellipsis() {
        let mut book = BookDocument::new();
        let xhtml = "<html><body><p>A\u{2013}B and C\u{2014}D and more\u{2026}</p></body></html>"
            .to_string();
        let item = ManifestItem::new(
            "ch1",
            "ch1.xhtml",
            "application/xhtml+xml",
            ManifestData::Xhtml(xhtml),
        );
        book.manifest.add(item);

        let opts = ConversionOptions {
            unsmarten_punctuation: true,
            ..Default::default()
        };

        UnsmartenPunctuation.apply(&mut book, &opts).unwrap();

        let content = book.manifest.by_id("ch1").unwrap().data.as_xhtml().unwrap();
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

        let opts = ConversionOptions {
            unsmarten_punctuation: true,
            ..Default::default()
        };

        UnsmartenPunctuation.apply(&mut book, &opts).unwrap();

        let content = book.manifest.by_id("ch1").unwrap().data.as_xhtml().unwrap();
        assert_eq!(content, &xhtml);
    }
}
