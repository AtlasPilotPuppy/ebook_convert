//! PageMargin — removes fake margins and Adobe page template margins from content.

use std::collections::HashMap;

use convert_core::book::{BookDocument, ManifestData};
use convert_core::error::Result;
use convert_core::options::ConversionOptions;
use convert_core::plugin::Transform;
use regex::Regex;

/// Removes artificial margins: Adobe page-template margins and
/// fake margins (where >95% of paragraphs share the same non-zero margin).
pub struct PageMargin;

impl Transform for PageMargin {
    fn name(&self) -> &str {
        "PageMargin"
    }

    fn apply(&self, book: &mut BookDocument, _options: &ConversionOptions) -> Result<()> {
        remove_adobe_margins(book);
        remove_fake_margins(book);
        Ok(())
    }
}

/// Remove margins from Adobe page template items.
fn remove_adobe_margins(book: &mut BookDocument) {
    let margin_re = Regex::new(r"margin\s*:\s*[^;]+;?").unwrap();

    for item in book.manifest.iter_mut() {
        // Adobe page templates use specific media types
        let is_adobe_template = item.media_type == "application/vnd.adobe-page-template+xml"
            || item.media_type == "application/adobe-page-template+xml";
        if !is_adobe_template {
            continue;
        }

        if let Some(xhtml) = item.data.as_xhtml() {
            let new_xhtml = margin_re.replace_all(xhtml, "").to_string();
            if new_xhtml != xhtml {
                log::debug!("Removed Adobe margins from {}", item.id);
                item.data = ManifestData::Xhtml(new_xhtml);
            }
        }
    }
}

/// Scan XHTML items for paragraphs/divs where >95% share the same non-zero
/// margin-left or margin-right, and remove those margins.
fn remove_fake_margins(book: &mut BookDocument) {
    let margin_left_re = Regex::new(r"margin-left\s*:\s*([^;]+)").unwrap();
    let margin_right_re = Regex::new(r"margin-right\s*:\s*([^;]+)").unwrap();

    // First pass: collect margin statistics across all XHTML items
    let mut left_counts: HashMap<String, u32> = HashMap::new();
    let mut right_counts: HashMap<String, u32> = HashMap::new();
    let mut total_styled = 0u32;

    let xhtml_ids: Vec<String> = book
        .manifest
        .iter()
        .filter(|item| item.is_xhtml())
        .map(|item| item.id.clone())
        .collect();

    // Regex for <p> and <div> with style attributes
    let element_re = Regex::new(r#"<(?:p|div)\s[^>]*style\s*=\s*"([^"]*)"[^>]*>"#).unwrap();

    for id in &xhtml_ids {
        let xhtml = match book.manifest.by_id(id).and_then(|i| i.data.as_xhtml()) {
            Some(x) => x.to_string(),
            None => continue,
        };

        for cap in element_re.captures_iter(&xhtml) {
            let style = &cap[1];
            total_styled += 1;

            if let Some(m) = margin_left_re.captures(style) {
                let val = m[1].trim().to_string();
                if val != "0" && val != "0px" && val != "0pt" && val != "0em" {
                    *left_counts.entry(val).or_insert(0) += 1;
                }
            }
            if let Some(m) = margin_right_re.captures(style) {
                let val = m[1].trim().to_string();
                if val != "0" && val != "0px" && val != "0pt" && val != "0em" {
                    *right_counts.entry(val).or_insert(0) += 1;
                }
            }
        }
    }

    if total_styled == 0 {
        return;
    }

    let threshold = (total_styled as f64 * 0.95) as u32;

    // Find dominant margins that appear in >95% of styled elements
    let dominant_left: Option<String> = left_counts
        .iter()
        .find(|(_, &count)| count >= threshold)
        .map(|(val, _)| val.clone());

    let dominant_right: Option<String> = right_counts
        .iter()
        .find(|(_, &count)| count >= threshold)
        .map(|(val, _)| val.clone());

    if dominant_left.is_none() && dominant_right.is_none() {
        return;
    }

    let empty_style = Regex::new(r#"\s*style\s*=\s*"\s*""#).unwrap();

    // Second pass: remove dominant margins
    for id in &xhtml_ids {
        let xhtml = match book.manifest.by_id(id).and_then(|i| i.data.as_xhtml()) {
            Some(x) => x.to_string(),
            None => continue,
        };

        let mut new_xhtml = xhtml.clone();

        if let Some(ref dominant) = dominant_left {
            let pattern = format!(r"margin-left\s*:\s*{}\s*;?", regex::escape(dominant));
            if let Ok(re) = Regex::new(&pattern) {
                new_xhtml = re.replace_all(&new_xhtml, "").to_string();
            }
        }

        if let Some(ref dominant) = dominant_right {
            let pattern = format!(r"margin-right\s*:\s*{}\s*;?", regex::escape(dominant));
            if let Ok(re) = Regex::new(&pattern) {
                new_xhtml = re.replace_all(&new_xhtml, "").to_string();
            }
        }

        // Clean up empty style attributes
        new_xhtml = empty_style.replace_all(&new_xhtml, "").to_string();

        if new_xhtml != xhtml {
            if let Some(item) = book.manifest.by_id_mut(id) {
                item.data = ManifestData::Xhtml(new_xhtml);
            }
        }
    }

    if let Some(ref l) = dominant_left {
        log::info!("Removed fake margin-left: {} from content", l);
    }
    if let Some(ref r) = dominant_right {
        log::info!("Removed fake margin-right: {} from content", r);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use convert_core::book::ManifestItem;

    #[test]
    fn test_remove_adobe_margins() {
        let mut book = BookDocument::new();
        let xhtml = r#"<html><body style="margin: 20px;">Content</body></html>"#.to_string();
        let item = ManifestItem::new(
            "template1",
            "template.xhtml",
            "application/vnd.adobe-page-template+xml",
            ManifestData::Xhtml(xhtml),
        );
        book.manifest.add(item);

        let opts = ConversionOptions::default();
        PageMargin.apply(&mut book, &opts).unwrap();

        let content = book
            .manifest
            .by_id("template1")
            .unwrap()
            .data
            .as_xhtml()
            .unwrap();
        assert!(!content.contains("margin"));
    }

    #[test]
    fn test_remove_fake_margins() {
        let mut book = BookDocument::new();

        // Create content where >95% of paragraphs have the same margin-left
        let mut paragraphs = String::new();
        for i in 0..20 {
            paragraphs.push_str(&format!(
                r#"<p style="margin-left: 2em;">Paragraph {}</p>"#,
                i
            ));
        }
        let xhtml = format!("<html><body>{}</body></html>", paragraphs);

        let item = ManifestItem::new(
            "ch1",
            "ch1.xhtml",
            "application/xhtml+xml",
            ManifestData::Xhtml(xhtml),
        );
        book.manifest.add(item);
        book.spine.push("ch1", true);

        let opts = ConversionOptions::default();
        PageMargin.apply(&mut book, &opts).unwrap();

        let content = book
            .manifest
            .by_id("ch1")
            .unwrap()
            .data
            .as_xhtml()
            .unwrap();
        assert!(!content.contains("margin-left: 2em"));
    }

    #[test]
    fn test_mixed_margins_not_removed() {
        let mut book = BookDocument::new();

        // Create content where margins are varied (no dominant margin)
        let mut paragraphs = String::new();
        let margins = ["1em", "2em", "3em", "4em"];
        for (i, margin) in margins.iter().cycle().take(20).enumerate() {
            paragraphs.push_str(&format!(
                r#"<p style="margin-left: {};">Paragraph {}</p>"#,
                margin, i
            ));
        }
        let xhtml = format!("<html><body>{}</body></html>", paragraphs);

        let item = ManifestItem::new(
            "ch1",
            "ch1.xhtml",
            "application/xhtml+xml",
            ManifestData::Xhtml(xhtml.clone()),
        );
        book.manifest.add(item);

        let opts = ConversionOptions::default();
        PageMargin.apply(&mut book, &opts).unwrap();

        let content = book
            .manifest
            .by_id("ch1")
            .unwrap()
            .data
            .as_xhtml()
            .unwrap();
        // All margins should remain since none is dominant
        assert!(content.contains("margin-left"));
    }
}
