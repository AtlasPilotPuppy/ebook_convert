//! PDF output plugin — serializes BookDocument to a PDF file.
//!
//! Uses printpdf 0.8 with builtin Helvetica fonts and Op-based page construction.
//! Text is extracted from XHTML spine items and rendered with word wrapping.
//! Images are embedded as XObjects.

use std::path::Path;

use rayon::prelude::*;

use convert_core::book::{BookDocument, EbookFormat, ManifestData};
use convert_core::error::{ConvertError, Result};
use convert_core::options::ConversionOptions;
use convert_core::plugin::OutputPlugin;

use printpdf::*;
use regex::Regex;

/// A4 page dimensions in mm.
const PAGE_W: Mm = Mm(210.0);
const PAGE_H: Mm = Mm(297.0);

/// Margins in mm.
const MARGIN: f32 = 25.0;

/// Font sizes in pt.
const FONT_SIZE_BODY: f32 = 11.0;
const FONT_SIZE_H1: f32 = 22.0;
const FONT_SIZE_H2: f32 = 18.0;
const FONT_SIZE_H3: f32 = 15.0;

/// Line height multiplier.
const LINE_HEIGHT: f32 = 1.4;

/// Approximate mm per pt.
const MM_PER_PT: f32 = 0.353;

pub struct PdfOutputPlugin;

impl OutputPlugin for PdfOutputPlugin {
    fn name(&self) -> &str {
        "PDF Output"
    }

    fn output_format(&self) -> EbookFormat {
        EbookFormat::Pdf
    }

    fn convert(
        &self,
        book: &BookDocument,
        output_path: &Path,
        _options: &ConversionOptions,
    ) -> Result<()> {
        log::info!("Writing PDF: {}", output_path.display());
        write_pdf(book, output_path)
    }
}

struct PageBuilder {
    pages: Vec<PdfPage>,
    current_ops: Vec<Op>,
    y_pos: f32, // mm from bottom
    chars_per_line: usize,
    in_text: bool,
}

impl PageBuilder {
    fn new() -> Self {
        let usable_w = 210.0 - 2.0 * MARGIN;
        let chars_per_line = (usable_w / (FONT_SIZE_BODY * 0.5 * MM_PER_PT)) as usize;
        let mut pb = PageBuilder {
            pages: Vec::new(),
            current_ops: Vec::new(),
            y_pos: 297.0 - MARGIN,
            chars_per_line,
            in_text: false,
        };
        pb.start_text();
        pb
    }

    fn start_text(&mut self) {
        if !self.in_text {
            self.current_ops.push(Op::StartTextSection);
            self.in_text = true;
        }
    }

    fn end_text(&mut self) {
        if self.in_text {
            self.current_ops.push(Op::EndTextSection);
            self.in_text = false;
        }
    }

    fn new_page(&mut self) {
        self.end_text();
        let ops = std::mem::take(&mut self.current_ops);
        self.pages.push(PdfPage::new(PAGE_W, PAGE_H, ops));
        self.y_pos = 297.0 - MARGIN;
        self.start_text();
    }

    fn ensure_space(&mut self, needed_mm: f32) {
        if self.y_pos - needed_mm < MARGIN {
            self.new_page();
        }
    }

    fn write_line(&mut self, text: &str, font_size: f32, font: BuiltinFont) {
        let line_h = font_size * LINE_HEIGHT * MM_PER_PT;
        self.ensure_space(line_h);

        self.current_ops.push(Op::SetTextCursor {
            pos: Point {
                x: Mm(MARGIN).into(),
                y: Mm(self.y_pos).into(),
            },
        });
        self.current_ops.push(Op::WriteTextBuiltinFont {
            items: vec![TextItem::Text(text.to_string())],
            font,
        });

        self.y_pos -= line_h;
    }

    fn write_wrapped(&mut self, text: &str, font_size: f32, font: BuiltinFont) {
        let words: Vec<&str> = text.split_whitespace().collect();
        let mut line = String::new();

        for word in words {
            if line.len() + word.len() + 1 > self.chars_per_line && !line.is_empty() {
                self.write_line(&line, font_size, font);
                line.clear();
            }
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(word);
        }
        if !line.is_empty() {
            self.write_line(&line, font_size, font);
        }
    }

    fn add_image(&mut self, doc: &mut PdfDocument, data: &[u8]) -> std::result::Result<(), String> {
        let mut warnings = Vec::new();
        let image = RawImage::decode_from_bytes(data, &mut warnings)
            .map_err(|e| format!("Image decode: {}", e))?;

        let (w, h) = (image.width, image.height);
        let image_id = doc.add_image(&image);

        // Scale to fit usable area
        let max_w = 210.0 - 2.0 * MARGIN;
        let max_h = 297.0 - 2.0 * MARGIN;
        let img_w_mm = w as f32 * 0.264583; // px to mm at 96 dpi
        let img_h_mm = h as f32 * 0.264583;
        let scale = (max_w / img_w_mm).min(max_h / img_h_mm).min(1.0);
        let final_h = img_h_mm * scale;

        self.ensure_space(final_h);
        self.end_text(); // images go outside text sections

        self.current_ops.push(Op::UseXobject {
            id: image_id,
            transform: XObjectTransform {
                translate_x: Some(Mm(MARGIN).into()),
                translate_y: Some(Mm(self.y_pos - final_h).into()),
                scale_x: Some(scale),
                scale_y: Some(scale),
                dpi: Some(96.0),
                ..Default::default()
            },
        });

        self.y_pos -= final_h + 5.0;
        self.start_text();
        Ok(())
    }

    fn finish(mut self) -> Vec<PdfPage> {
        self.end_text();
        let ops = std::mem::take(&mut self.current_ops);
        if !ops.is_empty() {
            self.pages.push(PdfPage::new(PAGE_W, PAGE_H, ops));
        }
        self.pages
    }
}

fn write_pdf(book: &BookDocument, output_path: &Path) -> Result<()> {
    let fallback_title = output_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Untitled")
        .to_string();
    let title = book.metadata.title().unwrap_or(&fallback_title);

    let mut doc = PdfDocument::new(title);
    let mut builder = PageBuilder::new();

    // Title page
    builder.write_line(title, FONT_SIZE_H1, BuiltinFont::HelveticaBold);
    builder.y_pos -= 5.0;

    for author in book.metadata.authors() {
        builder.write_line(author, FONT_SIZE_BODY, BuiltinFont::HelveticaOblique);
    }
    builder.y_pos -= 10.0;

    // Content — extract text from spine items in parallel, then render sequentially
    let tag_re = Regex::new(r"<[^>]+>").unwrap();
    let heading_re = Regex::new(r"(?i)<h([1-6])[^>]*>(.*?)</h[1-6]>").unwrap();
    let para_re = Regex::new(r"(?is)<p[^>]*>(.*?)</p>").unwrap();

    // Collect spine XHTMLs
    let spine_xhtmls: Vec<&str> = book
        .spine
        .iter()
        .filter_map(|si| book.manifest.by_id(&si.idref))
        .filter_map(|item| match &item.data {
            ManifestData::Xhtml(ref x) => Some(x.as_str()),
            _ => None,
        })
        .collect();

    // Extract headings and paragraphs in parallel
    #[allow(clippy::type_complexity)]
    let extracted: Vec<(Vec<(u32, String)>, Vec<String>)> = spine_xhtmls
        .par_iter()
        .map(|xhtml| {
            let body = extract_body(xhtml);
            let headings: Vec<(u32, String)> = heading_re
                .captures_iter(&body)
                .filter_map(|cap| {
                    let level: u32 = cap[1].parse().unwrap_or(3);
                    let text = tag_re.replace_all(&cap[2], "").to_string();
                    let text = decode_entities(&text);
                    if text.trim().is_empty() {
                        None
                    } else {
                        Some((level, text.trim().to_string()))
                    }
                })
                .collect();
            let paragraphs: Vec<String> = para_re
                .captures_iter(&body)
                .filter_map(|cap| {
                    let text = tag_re.replace_all(&cap[1], " ").to_string();
                    let text = decode_entities(&text);
                    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
                    if text.is_empty() {
                        None
                    } else {
                        Some(text)
                    }
                })
                .collect();
            (headings, paragraphs)
        })
        .collect();

    // Render sequentially
    for (headings, paragraphs) in &extracted {
        for (level, text) in headings {
            let font_size = match level {
                1 => FONT_SIZE_H1,
                2 => FONT_SIZE_H2,
                _ => FONT_SIZE_H3,
            };
            builder.y_pos -= font_size * MM_PER_PT * 0.5;
            builder.write_line(text, font_size, BuiltinFont::HelveticaBold);
            builder.y_pos -= 2.0;
        }

        for text in paragraphs {
            builder.write_wrapped(text, FONT_SIZE_BODY, BuiltinFont::Helvetica);
            builder.y_pos -= 2.0;
        }
    }

    // Embed images
    for item in book.manifest.iter() {
        if item.is_image() {
            if let ManifestData::Binary(ref data) = item.data {
                if let Err(e) = builder.add_image(&mut doc, data) {
                    log::warn!("Failed to embed image {}: {}", item.href, e);
                }
            }
        }
    }

    let pages = builder.finish();
    let mut warnings = Vec::new();
    let pdf_bytes = doc
        .with_pages(pages)
        .save(&PdfSaveOptions::default(), &mut warnings);

    std::fs::write(output_path, pdf_bytes)
        .map_err(|e| ConvertError::Other(format!("Failed to write PDF: {}", e)))?;

    Ok(())
}

fn extract_body(xhtml: &str) -> String {
    let lower = xhtml.to_lowercase();
    if let Some(start) = lower.find("<body") {
        let after = xhtml[start..].find('>').unwrap_or(0);
        let end = lower.rfind("</body>").unwrap_or(xhtml.len());
        xhtml[start + after + 1..end].to_string()
    } else {
        xhtml.to_string()
    }
}

fn decode_entities(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use convert_core::book::{ManifestItem, TocEntry};

    #[test]
    fn test_extract_body() {
        let xhtml = "<html><body><p>Hello</p></body></html>";
        assert_eq!(extract_body(xhtml), "<p>Hello</p>");
    }

    #[test]
    fn test_decode_entities() {
        assert_eq!(decode_entities("A &amp; B &lt; C"), "A & B < C");
    }

    #[test]
    fn test_pdf_output_basic() {
        let mut book = BookDocument::new();
        book.metadata.set_title("Test PDF");
        book.metadata.add("creator", "Author");

        let xhtml = "<html><body><h1>Chapter 1</h1><p>Hello world.</p></body></html>".to_string();
        let item = ManifestItem::new(
            "ch1",
            "ch1.xhtml",
            "application/xhtml+xml",
            ManifestData::Xhtml(xhtml),
        );
        book.manifest.add(item);
        book.spine.push("ch1", true);
        book.toc.add(TocEntry::new("Chapter 1", "ch1.xhtml"));

        let tmp = std::env::temp_dir().join("test_output_pdf.pdf");
        let opts = ConversionOptions::default();
        PdfOutputPlugin.convert(&book, &tmp, &opts).unwrap();

        let data = std::fs::read(&tmp).unwrap();
        assert!(data.len() > 100);
        assert_eq!(&data[..5], b"%PDF-");
        std::fs::remove_file(&tmp).ok();
    }
}
