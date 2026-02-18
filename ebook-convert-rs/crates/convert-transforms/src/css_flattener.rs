//! CSSFlattener transform — processes and optimizes CSS stylesheets.
//!
//! Uses lightningcss for parsing and minification, rayon for parallel processing
//! of multiple stylesheets and XHTML spine items.
//!
//! This handles:
//! - CSS parsing and minification via lightningcss
//! - Extra CSS injection from user options
//! - Ensuring XHTML documents have proper stylesheet links
//! - Parallel processing of multiple CSS files with rayon

use convert_core::book::{BookDocument, ManifestData, ManifestItem};
use convert_core::error::Result;
use convert_core::options::ConversionOptions;
use convert_core::plugin::Transform;
use rayon::prelude::*;

/// Processes and optimizes CSS stylesheets in the book.
pub struct CssFlattener;

impl Transform for CssFlattener {
    fn name(&self) -> &str {
        "CSSFlattener"
    }

    fn apply(&self, book: &mut BookDocument, options: &ConversionOptions) -> Result<()> {
        // Step 1: Inject extra CSS if provided
        if let Some(extra_css) = &options.extra_css {
            inject_extra_css(book, extra_css);
        }

        // Step 2: Collect all CSS items for parallel minification
        let css_items: Vec<(usize, String, String)> = book
            .manifest
            .iter()
            .enumerate()
            .filter_map(|(i, item)| {
                if let ManifestData::Css(ref css) = item.data {
                    Some((i, css.clone(), item.href.clone()))
                } else {
                    None
                }
            })
            .collect();

        if !css_items.is_empty() {
            // Process CSS files in parallel with rayon
            let minified: Vec<(usize, String)> = css_items
                .into_par_iter()
                .map(|(idx, css, href)| {
                    let result = minify_css(&css, &href);
                    (idx, result)
                })
                .collect();

            // Apply minified CSS back
            for (idx, css) in minified {
                if let Some(item) = book.manifest.iter_mut().nth(idx) {
                    item.data = ManifestData::Css(css);
                }
            }
        }

        // Step 3: Collect CSS hrefs for link injection
        let css_hrefs: Vec<String> = book
            .manifest
            .iter()
            .filter(|item| item.is_css())
            .map(|item| item.href.clone())
            .collect();

        if css_hrefs.is_empty() {
            log::info!("No CSS stylesheets to process");
            return Ok(());
        }

        // Step 4: Ensure all XHTML documents reference the stylesheets
        // Collect XHTML items that need updating
        let xhtml_updates: Vec<(usize, String)> = book
            .manifest
            .iter()
            .enumerate()
            .filter_map(|(i, item)| {
                if let ManifestData::Xhtml(ref xhtml) = item.data {
                    Some((i, xhtml.clone()))
                } else {
                    None
                }
            })
            .collect();

        // Process XHTML items in parallel — add missing stylesheet links
        let updated: Vec<(usize, String)> = xhtml_updates
            .into_par_iter()
            .map(|(idx, xhtml)| {
                let updated = ensure_css_links(&xhtml, &css_hrefs);
                (idx, updated)
            })
            .collect();

        // Apply back
        for (idx, xhtml) in updated {
            if let Some(item) = book.manifest.iter_mut().nth(idx) {
                item.data = ManifestData::Xhtml(xhtml);
            }
        }

        log::info!(
            "CSS flattening complete: {} stylesheets processed",
            css_hrefs.len()
        );
        Ok(())
    }
}

/// Minify a CSS string using lightningcss.
fn minify_css(css: &str, href: &str) -> String {
    use lightningcss::stylesheet::{MinifyOptions, ParserOptions, PrinterOptions, StyleSheet};

    match StyleSheet::parse(css, ParserOptions::default()) {
        Ok(mut stylesheet) => {
            if let Err(e) = stylesheet.minify(MinifyOptions::default()) {
                log::warn!("CSS minification warning for {}: {}", href, e);
            }
            match stylesheet.to_css(PrinterOptions::default()) {
                Ok(result) => {
                    let saved = css.len() as i64 - result.code.len() as i64;
                    if saved > 0 {
                        log::debug!(
                            "Minified {} ({} → {} bytes, saved {})",
                            href,
                            css.len(),
                            result.code.len(),
                            saved
                        );
                    }
                    result.code
                }
                Err(e) => {
                    log::warn!("CSS print failed for {}: {}, keeping original", href, e);
                    css.to_string()
                }
            }
        }
        Err(e) => {
            log::warn!("CSS parse failed for {}: {}, keeping original", href, e);
            css.to_string()
        }
    }
}

/// Inject extra CSS into the book's first stylesheet or create a new one.
fn inject_extra_css(book: &mut BookDocument, extra_css: &str) {
    let mut found_css = false;
    for item in book.manifest.iter_mut() {
        if item.is_css() {
            if let Some(existing) = item.data.as_css() {
                let combined = format!("{}\n\n/* Extra CSS */\n{}", existing, extra_css);
                item.data = ManifestData::Css(combined);
                found_css = true;
                break;
            }
        }
    }

    if !found_css {
        let item = ManifestItem::new(
            book.manifest.generate_id("css"),
            "extra.css",
            "text/css",
            ManifestData::Css(extra_css.to_string()),
        );
        book.manifest.add(item);
    }
}

/// Ensure XHTML has <link> tags for all stylesheets.
fn ensure_css_links(xhtml: &str, css_hrefs: &[String]) -> String {
    let mut result = xhtml.to_string();

    for href in css_hrefs {
        let link_patterns = [format!("href=\"{}\"", href), format!("href='{}'", href)];

        let already_linked = link_patterns
            .iter()
            .any(|pat| result.contains(pat.as_str()));

        if !already_linked {
            // Insert link before </head>
            let link_tag = format!(
                "<link rel=\"stylesheet\" type=\"text/css\" href=\"{}\"/>\n",
                href
            );

            if let Some(pos) = result.to_lowercase().find("</head>") {
                result.insert_str(pos, &link_tag);
            }
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use convert_core::book::ManifestData;

    #[test]
    fn test_css_flattener_with_extra_css() {
        let mut book = BookDocument::new();
        let css_item = ManifestItem::new(
            "style",
            "style.css",
            "text/css",
            ManifestData::Css("body { margin: 0; }".to_string()),
        );
        book.manifest.add(css_item);

        let opts = ConversionOptions {
            extra_css: Some("p { color: red; }".to_string()),
            ..Default::default()
        };

        CssFlattener.apply(&mut book, &opts).unwrap();

        let css = book.manifest.by_id("style").unwrap();
        let content = css.data.as_css().unwrap();
        assert!(content.contains("color"));
        assert!(content.contains("margin"));
    }

    #[test]
    fn test_css_flattener_creates_css_if_missing() {
        let mut book = BookDocument::new();
        let opts = ConversionOptions {
            extra_css: Some("body { font-size: 14px; }".to_string()),
            ..Default::default()
        };

        CssFlattener.apply(&mut book, &opts).unwrap();

        assert!(book.manifest.by_href("extra.css").is_some());
    }

    #[test]
    fn test_minify_css() {
        let css = "body {\n  margin: 0;\n  padding: 0;\n}\n\np {\n  color: red;\n}\n";
        let minified = minify_css(css, "test.css");
        // Should be shorter than original
        assert!(minified.len() <= css.len());
        assert!(minified.contains("margin"));
        assert!(minified.contains("color"));
    }

    #[test]
    fn test_ensure_css_links() {
        let xhtml = "<html><head><title>Test</title></head><body></body></html>";
        let hrefs = vec!["style.css".to_string()];
        let result = ensure_css_links(xhtml, &hrefs);
        assert!(result.contains("href=\"style.css\""));
    }

    #[test]
    fn test_ensure_css_links_no_duplicate() {
        let xhtml =
            "<html><head><link rel=\"stylesheet\" href=\"style.css\"/></head><body></body></html>";
        let hrefs = vec!["style.css".to_string()];
        let result = ensure_css_links(xhtml, &hrefs);
        // Should not add a duplicate
        assert_eq!(result.matches("style.css").count(), 1);
    }
}
