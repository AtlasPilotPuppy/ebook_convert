//! XHTML generation for text-based PDF pages.
//!
//! Groups text elements into lines and paragraphs, interleaves images,
//! and produces semantic XHTML suitable for reflowable EPUB.

use std::collections::HashMap;

use crate::pdftohtml::{FontSpec, ImageElement, PdfPage};

/// A line of text composed of one or more text elements at the same vertical position.
#[derive(Debug)]
struct TextLine {
    top: f64,
    height: f64,
    /// Fragments sorted by left position, with their inner HTML.
    fragments: Vec<(f64, String)>,
}

/// A content block — either a paragraph of text or an image.
#[derive(Debug)]
enum ContentBlock {
    Paragraph(String),
    Image { src: String, alt: String },
}

/// Build an XHTML page for a text-based PDF page.
///
/// `image_map` maps pdftohtml image `src` names to their EPUB `href` paths.
pub fn build_text_page_xhtml(
    page: &PdfPage,
    fonts: &[FontSpec],
    image_map: &HashMap<String, String>,
) -> String {
    let lines = group_into_lines(&page.text_elements, fonts);
    let blocks = build_content_blocks(&lines, &page.images, image_map);

    let mut body = String::new();
    body.push_str("  <div class=\"page\">\n");

    if blocks.is_empty() {
        body.push_str(&format!(
            "    <p class=\"empty-page\">[Page {}]</p>\n",
            page.number
        ));
    } else {
        for block in &blocks {
            match block {
                ContentBlock::Paragraph(html) => {
                    body.push_str("    <p>");
                    body.push_str(html);
                    body.push_str("</p>\n");
                }
                ContentBlock::Image { src, alt } => {
                    body.push_str(&format!(
                        "    <div class=\"page-image\"><img src=\"{}\" alt=\"{}\"/></div>\n",
                        convert_utils::xml::escape_xml_attr(src),
                        convert_utils::xml::escape_xml_attr(alt),
                    ));
                }
            }
        }
    }

    body.push_str("  </div>");

    convert_utils::xml::xhtml11_document(
        &format!("Page {}", page.number),
        "en",
        Some("style.css"),
        &body,
    )
}

/// Group text elements into lines based on vertical position.
/// Elements within `tolerance` pixels of each other vertically are on the same line.
fn group_into_lines(
    elements: &[crate::pdftohtml::TextElement],
    _fonts: &[FontSpec],
) -> Vec<TextLine> {
    if elements.is_empty() {
        return Vec::new();
    }

    // Sort by top position, then left
    let mut sorted: Vec<_> = elements.iter().collect();
    sorted.sort_by(|a, b| {
        a.top
            .partial_cmp(&b.top)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(
                a.left
                    .partial_cmp(&b.left)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
    });

    let tolerance = 3.0; // pixels
    let mut lines: Vec<TextLine> = Vec::new();

    for elem in &sorted {
        let content = elem.inner_text();
        if content.trim().is_empty() {
            continue;
        }

        // Find an existing line at this vertical position
        let found = lines
            .iter_mut()
            .find(|line| (line.top - elem.top).abs() < tolerance);

        match found {
            Some(line) => {
                line.fragments.push((elem.left, elem.inner_html.clone()));
            }
            None => {
                lines.push(TextLine {
                    top: elem.top,
                    height: elem.height,
                    fragments: vec![(elem.left, elem.inner_html.clone())],
                });
            }
        }
    }

    // Sort fragments within each line by left position
    for line in &mut lines {
        line.fragments
            .sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    }

    // Sort lines by top position
    lines.sort_by(|a, b| {
        a.top
            .partial_cmp(&b.top)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    lines
}

/// Build content blocks (paragraphs and images) from lines and images.
///
/// Paragraph detection: a gap between lines > 1.5x the average line gap
/// triggers a new paragraph.
fn build_content_blocks(
    lines: &[TextLine],
    images: &[ImageElement],
    image_map: &HashMap<String, String>,
) -> Vec<ContentBlock> {
    if lines.is_empty() && images.is_empty() {
        return Vec::new();
    }

    // Compute average line gap for paragraph detection
    let avg_gap = compute_avg_line_gap(lines);
    let para_threshold = avg_gap * 1.5;

    // Build a sorted list of content items by vertical position
    #[derive(Debug)]
    enum Item<'a> {
        Line(usize),       // index into lines
        Image(&'a ImageElement),
    }

    let mut items: Vec<(f64, Item)> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        items.push((line.top, Item::Line(i)));
    }
    for img in images {
        if let Some(epub_href) = image_map.get(&img.src) {
            items.push((img.top, Item::Image(img)));
            let _ = epub_href; // used below when we match
        }
    }
    items.sort_by(|a, b| {
        a.0.partial_cmp(&b.0)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut blocks: Vec<ContentBlock> = Vec::new();
    let mut current_para_lines: Vec<String> = Vec::new();
    let mut last_line_bottom: Option<f64> = None;

    for (_top, item) in &items {
        match item {
            Item::Line(idx) => {
                let line = &lines[*idx];

                // Check if we need a paragraph break
                if let Some(prev_bottom) = last_line_bottom {
                    let gap = line.top - prev_bottom;
                    if gap > para_threshold && !current_para_lines.is_empty() {
                        blocks.push(ContentBlock::Paragraph(
                            current_para_lines.join(" "),
                        ));
                        current_para_lines.clear();
                    }
                }

                // Join fragments of this line with spaces
                let line_html: String = line
                    .fragments
                    .iter()
                    .map(|(_, html)| html.as_str())
                    .collect::<Vec<_>>()
                    .join(" ");

                current_para_lines.push(line_html);
                last_line_bottom = Some(line.top + line.height);
            }
            Item::Image(img) => {
                // Flush current paragraph before image
                if !current_para_lines.is_empty() {
                    blocks.push(ContentBlock::Paragraph(
                        current_para_lines.join(" "),
                    ));
                    current_para_lines.clear();
                    last_line_bottom = None;
                }

                if let Some(epub_href) = image_map.get(&img.src) {
                    blocks.push(ContentBlock::Image {
                        src: epub_href.clone(),
                        alt: format!("Image at ({}, {})", img.left as u32, img.top as u32),
                    });
                }
            }
        }
    }

    // Flush remaining paragraph
    if !current_para_lines.is_empty() {
        blocks.push(ContentBlock::Paragraph(
            current_para_lines.join(" "),
        ));
    }

    blocks
}

/// Compute the average gap between consecutive lines.
fn compute_avg_line_gap(lines: &[TextLine]) -> f64 {
    if lines.len() < 2 {
        return 10.0; // default gap
    }

    let mut gaps: Vec<f64> = Vec::new();
    for i in 1..lines.len() {
        let gap = lines[i].top - (lines[i - 1].top + lines[i - 1].height);
        if gap > 0.0 {
            gaps.push(gap);
        }
    }

    if gaps.is_empty() {
        return 10.0;
    }

    gaps.iter().sum::<f64>() / gaps.len() as f64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdftohtml::TextElement;

    fn make_text(top: f64, left: f64, width: f64, height: f64, html: &str) -> TextElement {
        TextElement {
            top,
            left,
            width,
            height,
            font_id: 0,
            inner_html: html.to_string(),
        }
    }

    #[test]
    fn test_line_grouping() {
        let elements = vec![
            make_text(100.0, 50.0, 100.0, 14.0, "Hello"),
            make_text(100.5, 160.0, 100.0, 14.0, "world"),
            make_text(120.0, 50.0, 200.0, 14.0, "Next line"),
        ];
        let fonts = vec![];
        let lines = group_into_lines(&elements, &fonts);

        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].fragments.len(), 2); // "Hello" and "world" on same line
        assert_eq!(lines[1].fragments.len(), 1); // "Next line"
    }

    #[test]
    fn test_paragraph_detection() {
        // Lines with a big gap between line 2 and 3 should form two paragraphs
        let elements = vec![
            make_text(100.0, 50.0, 200.0, 14.0, "Line 1"),
            make_text(116.0, 50.0, 200.0, 14.0, "Line 2"),
            // Gap of ~30px (much more than the ~2px inter-line gap)
            make_text(160.0, 50.0, 200.0, 14.0, "Line 3"),
            make_text(176.0, 50.0, 200.0, 14.0, "Line 4"),
        ];
        let fonts = vec![];
        let lines = group_into_lines(&elements, &fonts);
        let blocks = build_content_blocks(&lines, &[], &HashMap::new());

        assert_eq!(blocks.len(), 2);
        match &blocks[0] {
            ContentBlock::Paragraph(text) => assert!(text.contains("Line 1")),
            _ => panic!("Expected paragraph"),
        }
        match &blocks[1] {
            ContentBlock::Paragraph(text) => assert!(text.contains("Line 3")),
            _ => panic!("Expected paragraph"),
        }
    }

    #[test]
    fn test_image_interleaving() {
        let elements = vec![
            make_text(100.0, 50.0, 200.0, 14.0, "Before image"),
            make_text(400.0, 50.0, 200.0, 14.0, "After image"),
        ];
        let images = vec![ImageElement {
            top: 200.0,
            left: 50.0,
            width: 300.0,
            height: 150.0,
            src: "output001.jpg".to_string(),
        }];
        let mut image_map = HashMap::new();
        image_map.insert(
            "output001.jpg".to_string(),
            "images/page1_img1.jpg".to_string(),
        );

        let fonts = vec![];
        let lines = group_into_lines(&elements, &fonts);
        let blocks = build_content_blocks(&lines, &images, &image_map);

        assert_eq!(blocks.len(), 3); // para, image, para
        assert!(matches!(&blocks[0], ContentBlock::Paragraph(_)));
        assert!(matches!(&blocks[1], ContentBlock::Image { .. }));
        assert!(matches!(&blocks[2], ContentBlock::Paragraph(_)));
    }

    #[test]
    fn test_build_text_page_xhtml() {
        let page = PdfPage {
            number: 1,
            width: 612.0,
            height: 792.0,
            text_elements: vec![make_text(100.0, 50.0, 200.0, 14.0, "Hello world")],
            images: vec![],
        };
        let fonts = vec![];
        let xhtml = build_text_page_xhtml(&page, &fonts, &HashMap::new());

        assert!(xhtml.contains("<title>Page 1</title>"));
        assert!(xhtml.contains("<p>Hello world</p>"));
        assert!(xhtml.contains("XHTML 1.1"));
    }

    #[test]
    fn test_empty_page_xhtml() {
        let page = PdfPage {
            number: 5,
            width: 612.0,
            height: 792.0,
            text_elements: vec![],
            images: vec![],
        };
        let fonts = vec![];
        let xhtml = build_text_page_xhtml(&page, &fonts, &HashMap::new());

        assert!(xhtml.contains("[Page 5]"));
    }
}
