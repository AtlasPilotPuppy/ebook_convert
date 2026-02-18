//! LinearizeTables — converts HTML table markup to divs for devices without table support.

use convert_core::book::{BookDocument, ManifestData};
use convert_core::error::Result;
use convert_core::options::ConversionOptions;
use convert_core::plugin::Transform;
use regex::Regex;

/// Replaces `<table>`, `<tr>`, `<td>`, `<th>` elements with styled `<div>`s
/// for e-readers that lack table rendering support.
pub struct LinearizeTables;

impl Transform for LinearizeTables {
    fn name(&self) -> &str {
        "LinearizeTables"
    }

    fn should_run(&self, options: &ConversionOptions) -> bool {
        options.linearize_tables
    }

    fn apply(&self, book: &mut BookDocument, _options: &ConversionOptions) -> Result<()> {
        let table_open = Regex::new(r"(?i)<table[^>]*>").unwrap();
        let table_close = Regex::new(r"(?i)</table\s*>").unwrap();
        let tr_open = Regex::new(r"(?i)<tr[^>]*>").unwrap();
        let tr_close = Regex::new(r"(?i)</tr\s*>").unwrap();
        let td_open = Regex::new(r"(?i)<td[^>]*>").unwrap();
        let td_close = Regex::new(r"(?i)</td\s*>").unwrap();
        let th_open = Regex::new(r"(?i)<th[^>]*>").unwrap();
        let th_close = Regex::new(r"(?i)</th\s*>").unwrap();
        let thead_open = Regex::new(r"(?i)<thead[^>]*>").unwrap();
        let thead_close = Regex::new(r"(?i)</thead\s*>").unwrap();
        let tbody_open = Regex::new(r"(?i)<tbody[^>]*>").unwrap();
        let tbody_close = Regex::new(r"(?i)</tbody\s*>").unwrap();
        let tfoot_open = Regex::new(r"(?i)<tfoot[^>]*>").unwrap();
        let tfoot_close = Regex::new(r"(?i)</tfoot\s*>").unwrap();
        let caption_open = Regex::new(r"(?i)<caption[^>]*>").unwrap();
        let caption_close = Regex::new(r"(?i)</caption\s*>").unwrap();
        let colgroup = Regex::new(r"(?i)</?colgroup[^>]*>").unwrap();
        let col_tag = Regex::new(r"(?i)<col[^>]*>").unwrap();

        let mut count = 0u32;

        for item in book.manifest.iter_mut() {
            if !item.is_xhtml() {
                continue;
            }
            if let Some(xhtml) = item.data.as_xhtml() {
                if !xhtml.contains("<table") && !xhtml.contains("<TABLE") {
                    continue;
                }

                let mut s = xhtml.to_string();
                s = table_open.replace_all(&s, r#"<div class="linearized-table">"#).to_string();
                s = table_close.replace_all(&s, "</div>").to_string();
                s = tr_open.replace_all(&s, r#"<div class="linearized-row">"#).to_string();
                s = tr_close.replace_all(&s, "</div>").to_string();
                s = td_open.replace_all(&s, r#"<div class="linearized-cell">"#).to_string();
                s = td_close.replace_all(&s, "</div>").to_string();
                s = th_open.replace_all(&s, r#"<div class="linearized-cell linearized-header">"#).to_string();
                s = th_close.replace_all(&s, "</div>").to_string();
                s = thead_open.replace_all(&s, r#"<div class="linearized-thead">"#).to_string();
                s = thead_close.replace_all(&s, "</div>").to_string();
                s = tbody_open.replace_all(&s, r#"<div class="linearized-tbody">"#).to_string();
                s = tbody_close.replace_all(&s, "</div>").to_string();
                s = tfoot_open.replace_all(&s, r#"<div class="linearized-tfoot">"#).to_string();
                s = tfoot_close.replace_all(&s, "</div>").to_string();
                s = caption_open.replace_all(&s, r#"<div class="linearized-caption">"#).to_string();
                s = caption_close.replace_all(&s, "</div>").to_string();
                s = colgroup.replace_all(&s, "").to_string();
                s = col_tag.replace_all(&s, "").to_string();

                item.data = ManifestData::Xhtml(s);
                count += 1;
            }
        }

        if count > 0 {
            log::info!("Linearized tables in {} items", count);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use convert_core::book::ManifestItem;

    #[test]
    fn test_linearize_basic_table() {
        let mut book = BookDocument::new();
        let xhtml = r#"<html><body><table><tr><td>A</td><td>B</td></tr><tr><td>C</td><td>D</td></tr></table></body></html>"#.to_string();
        let item = ManifestItem::new(
            "ch1",
            "ch1.xhtml",
            "application/xhtml+xml",
            ManifestData::Xhtml(xhtml),
        );
        book.manifest.add(item);

        let mut opts = ConversionOptions::default();
        opts.linearize_tables = true;

        LinearizeTables.apply(&mut book, &opts).unwrap();

        let content = book.manifest.by_id("ch1").unwrap().data.as_xhtml().unwrap();
        assert!(!content.contains("<table"));
        assert!(!content.contains("<tr"));
        assert!(!content.contains("<td"));
        assert!(content.contains("linearized-table"));
        assert!(content.contains("linearized-row"));
        assert!(content.contains("linearized-cell"));
        assert!(content.contains("A"));
        assert!(content.contains("D"));
    }

    #[test]
    fn test_linearize_th_headers() {
        let mut book = BookDocument::new();
        let xhtml = r#"<html><body><table><tr><th>Name</th><th>Value</th></tr></table></body></html>"#.to_string();
        let item = ManifestItem::new(
            "ch1",
            "ch1.xhtml",
            "application/xhtml+xml",
            ManifestData::Xhtml(xhtml),
        );
        book.manifest.add(item);

        let mut opts = ConversionOptions::default();
        opts.linearize_tables = true;

        LinearizeTables.apply(&mut book, &opts).unwrap();

        let content = book.manifest.by_id("ch1").unwrap().data.as_xhtml().unwrap();
        assert!(content.contains("linearized-header"));
    }

    #[test]
    fn test_should_run() {
        let mut opts = ConversionOptions::default();
        assert!(!LinearizeTables.should_run(&opts));
        opts.linearize_tables = true;
        assert!(LinearizeTables.should_run(&opts));
    }

    #[test]
    fn test_no_tables_unchanged() {
        let mut book = BookDocument::new();
        let xhtml = "<html><body><p>No tables here</p></body></html>".to_string();
        let item = ManifestItem::new(
            "ch1",
            "ch1.xhtml",
            "application/xhtml+xml",
            ManifestData::Xhtml(xhtml.clone()),
        );
        book.manifest.add(item);

        let mut opts = ConversionOptions::default();
        opts.linearize_tables = true;

        LinearizeTables.apply(&mut book, &opts).unwrap();

        let content = book.manifest.by_id("ch1").unwrap().data.as_xhtml().unwrap();
        assert_eq!(content, &xhtml);
    }
}
