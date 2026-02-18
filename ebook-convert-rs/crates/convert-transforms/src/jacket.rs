//! Jacket — optionally inserts a metadata page at the beginning of the book.

use convert_core::book::{BookDocument, ManifestData, ManifestItem};
use convert_core::error::Result;
use convert_core::options::ConversionOptions;
use convert_core::plugin::Transform;
use regex::Regex;

/// Inserts a metadata jacket page (title, author, publisher, etc.) at spine[0],
/// and optionally removes the first image from the content.
pub struct Jacket;

impl Transform for Jacket {
    fn name(&self) -> &str {
        "Jacket"
    }

    fn should_run(&self, options: &ConversionOptions) -> bool {
        options.insert_metadata || options.remove_first_image
    }

    fn apply(&self, book: &mut BookDocument, options: &ConversionOptions) -> Result<()> {
        if options.remove_first_image {
            remove_first_image(book);
        }

        if options.insert_metadata {
            insert_jacket(book);
        }

        Ok(())
    }
}

/// Remove the first `<img>` element from the first spine item.
fn remove_first_image(book: &mut BookDocument) {
    let first_idref = match book.spine.items().first() {
        Some(item) => item.idref.clone(),
        None => return,
    };

    if let Some(item) = book.manifest.by_id_mut(&first_idref) {
        if let Some(xhtml) = item.data.as_xhtml() {
            let re = Regex::new(r"<img[^>]*>").unwrap();
            if let Some(m) = re.find(xhtml) {
                let new_xhtml =
                    format!("{}{}", &xhtml[..m.start()], &xhtml[m.end()..]);
                item.data = ManifestData::Xhtml(new_xhtml);
                log::info!("Removed first image from spine item {}", first_idref);
            }
        }
    }
}

/// Build and insert a jacket XHTML page at spine[0].
fn insert_jacket(book: &mut BookDocument) {
    let title = book
        .metadata
        .title()
        .unwrap_or("Unknown Title")
        .to_string();
    let authors = book.metadata.authors().join(", ");
    let publisher = book
        .metadata
        .publisher()
        .unwrap_or("")
        .to_string();
    let date = book.metadata.date().unwrap_or("").to_string();
    let description = book.metadata.description().unwrap_or("").to_string();

    // Build series info
    let series = book
        .metadata
        .get_first_value("series")
        .unwrap_or("")
        .to_string();
    let series_index = book
        .metadata
        .get_first_value("series_index")
        .unwrap_or("")
        .to_string();

    let mut body_parts = Vec::new();
    body_parts.push(format!(
        r#"<h1 class="jacket-title">{}</h1>"#,
        escape_html(&title)
    ));

    if !authors.is_empty() {
        body_parts.push(format!(
            r#"<p class="jacket-authors">{}</p>"#,
            escape_html(&authors)
        ));
    }

    if !series.is_empty() {
        let series_text = if series_index.is_empty() {
            escape_html(&series)
        } else {
            format!("{} #{}", escape_html(&series), escape_html(&series_index))
        };
        body_parts.push(format!(
            r#"<p class="jacket-series">{}</p>"#,
            series_text
        ));
    }

    if !publisher.is_empty() {
        body_parts.push(format!(
            r#"<p class="jacket-publisher">{}</p>"#,
            escape_html(&publisher)
        ));
    }

    if !date.is_empty() {
        body_parts.push(format!(
            r#"<p class="jacket-date">{}</p>"#,
            escape_html(&date)
        ));
    }

    if !description.is_empty() {
        body_parts.push(format!(
            r#"<div class="jacket-description">{}</div>"#,
            description // description may contain HTML, pass through
        ));
    }

    let body_html = body_parts.join("\n    ");

    let jacket_xhtml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE html PUBLIC "-//W3C//DTD XHTML 1.1//EN" "http://www.w3.org/TR/xhtml11/DTD/xhtml11.dtd">
<html xmlns="http://www.w3.org/1999/xhtml">
<head>
  <title>{title}</title>
  <style type="text/css">
    .jacket-title {{ font-size: 1.8em; text-align: center; margin: 1em 0 0.5em; }}
    .jacket-authors {{ font-size: 1.2em; text-align: center; margin: 0.5em 0; }}
    .jacket-series {{ text-align: center; font-style: italic; margin: 0.5em 0; }}
    .jacket-publisher {{ text-align: center; margin: 0.5em 0; }}
    .jacket-date {{ text-align: center; color: #666; margin: 0.5em 0; }}
    .jacket-description {{ margin: 1.5em 1em; }}
  </style>
</head>
<body>
    {body_html}
</body>
</html>"#,
        title = escape_html(&title),
        body_html = body_html,
    );

    let jacket_id = book.manifest.generate_id("jacket");
    let jacket_href = book.manifest.generate_href("jacket", "xhtml");

    let jacket_item = ManifestItem::new(
        &jacket_id,
        &jacket_href,
        "application/xhtml+xml",
        ManifestData::Xhtml(jacket_xhtml),
    );
    book.manifest.add(jacket_item);
    book.spine.insert(0, &jacket_id, true);

    log::info!("Inserted metadata jacket page at spine[0]");
}

/// Simple HTML entity escaping for text content.
fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use convert_core::book::BookDocument;

    #[test]
    fn test_insert_jacket() {
        let mut book = BookDocument::new();
        book.metadata.set_title("Test Book");
        book.metadata.add("creator", "Test Author");

        let ch1 = ManifestItem::new(
            "ch1",
            "chapter1.xhtml",
            "application/xhtml+xml",
            ManifestData::Xhtml("<html><body>Content</body></html>".to_string()),
        );
        book.manifest.add(ch1);
        book.spine.push("ch1", true);

        let mut opts = ConversionOptions::default();
        opts.insert_metadata = true;

        Jacket.apply(&mut book, &opts).unwrap();

        // Should have 2 items: jacket + original
        assert_eq!(book.manifest.len(), 2);
        assert_eq!(book.spine.len(), 2);
        // Jacket should be first in spine
        assert!(book.spine.items()[0].idref.starts_with("jacket"));

        // Verify jacket content
        let jacket_id = &book.spine.items()[0].idref;
        let jacket = book.manifest.by_id(jacket_id).unwrap();
        let content = jacket.data.as_xhtml().unwrap();
        assert!(content.contains("Test Book"));
        assert!(content.contains("Test Author"));
    }

    #[test]
    fn test_remove_first_image() {
        let mut book = BookDocument::new();
        let xhtml =
            r#"<html><body><img src="cover.png"/><p>Hello</p></body></html>"#.to_string();
        let ch1 = ManifestItem::new(
            "ch1",
            "chapter1.xhtml",
            "application/xhtml+xml",
            ManifestData::Xhtml(xhtml),
        );
        book.manifest.add(ch1);
        book.spine.push("ch1", true);

        let mut opts = ConversionOptions::default();
        opts.remove_first_image = true;

        Jacket.apply(&mut book, &opts).unwrap();

        let ch1 = book.manifest.by_id("ch1").unwrap();
        let content = ch1.data.as_xhtml().unwrap();
        assert!(!content.contains("<img"));
        assert!(content.contains("<p>Hello</p>"));
    }

    #[test]
    fn test_should_run() {
        let mut opts = ConversionOptions::default();
        assert!(!Jacket.should_run(&opts));

        opts.insert_metadata = true;
        assert!(Jacket.should_run(&opts));

        opts.insert_metadata = false;
        opts.remove_first_image = true;
        assert!(Jacket.should_run(&opts));
    }
}
