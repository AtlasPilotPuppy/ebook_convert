//! Page classification for hybrid PDF extraction.
//!
//! Classifies each page from pdftohtml XML output as text, scanned, image-only, or blank.

use crate::pdftohtml::{FontSpec, PdfPage};

/// Classification of a PDF page for extraction strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageType {
    /// Has real text elements with proper fonts — use text extraction.
    Text,
    /// Scanned page: no real text, or only OCR invisible text (GlyphLessFont).
    Scanned,
    /// Only images, no text at all — use extracted images directly.
    ImageOnly,
    /// No content whatsoever.
    Blank,
}

/// Known OCR invisible font families (used by Tesseract and other OCR engines).
const OCR_FONT_FAMILIES: &[&str] = &["GlyphLessFont", "Invisible"];

/// Returns true if the font family is an OCR invisible-text font.
fn is_ocr_font(family: &str) -> bool {
    OCR_FONT_FAMILIES
        .iter()
        .any(|ocr| family.eq_ignore_ascii_case(ocr))
}

/// Classify a page based on its text elements, images, and font information.
pub fn classify_page(page: &PdfPage, fonts: &[FontSpec]) -> PageType {
    let has_images = !page.images.is_empty();
    let has_text_elements = !page.text_elements.is_empty();

    if !has_text_elements && !has_images {
        return PageType::Blank;
    }

    if !has_text_elements && has_images {
        return PageType::ImageOnly;
    }

    // Check if all text uses OCR fonts
    let all_ocr = page.text_elements.iter().all(|te| {
        let content = te.inner_text();
        if content.trim().is_empty() {
            return true; // Empty text elements don't count
        }
        fonts
            .iter()
            .find(|f| f.id == te.font_id)
            .map(|f| is_ocr_font(&f.family))
            .unwrap_or(false)
    });

    // Check if there's any non-empty real text
    let has_real_text = page.text_elements.iter().any(|te| {
        let content = te.inner_text();
        if content.trim().is_empty() {
            return false;
        }
        fonts
            .iter()
            .find(|f| f.id == te.font_id)
            .map(|f| !is_ocr_font(&f.family))
            .unwrap_or(true) // Unknown font — assume real
    });

    if has_real_text {
        return PageType::Text;
    }

    if all_ocr && has_images {
        // Has full-page image(s) with only OCR invisible text overlay
        return PageType::Scanned;
    }

    if all_ocr {
        // Only OCR text, no images — treat as blank
        return PageType::Blank;
    }

    // Fallback
    PageType::Text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdftohtml::{ImageElement, TextElement};

    fn make_font(id: u32, family: &str) -> FontSpec {
        FontSpec {
            id,
            size: 12.0,
            family: family.to_string(),
            color: "#000000".to_string(),
        }
    }

    fn make_text(font_id: u32, content: &str) -> TextElement {
        TextElement {
            top: 100.0,
            left: 50.0,
            width: 200.0,
            height: 14.0,
            font_id,
            inner_html: content.to_string(),
        }
    }

    fn make_image(width: f64, height: f64) -> ImageElement {
        ImageElement {
            top: 0.0,
            left: 0.0,
            width,
            height,
            src: "page001.jpg".to_string(),
        }
    }

    #[test]
    fn test_classify_text_page() {
        let fonts = vec![make_font(0, "TimesNewRomanPSMT")];
        let page = PdfPage {
            number: 1,
            width: 612.0,
            height: 792.0,
            text_elements: vec![make_text(0, "Hello world")],
            images: vec![],
        };
        assert_eq!(classify_page(&page, &fonts), PageType::Text);
    }

    #[test]
    fn test_classify_scanned_page() {
        let fonts = vec![make_font(0, "GlyphLessFont")];
        let page = PdfPage {
            number: 1,
            width: 612.0,
            height: 792.0,
            text_elements: vec![make_text(0, "OCR text")],
            images: vec![make_image(612.0, 792.0)],
        };
        assert_eq!(classify_page(&page, &fonts), PageType::Scanned);
    }

    #[test]
    fn test_classify_blank_page() {
        let fonts = vec![];
        let page = PdfPage {
            number: 1,
            width: 612.0,
            height: 792.0,
            text_elements: vec![],
            images: vec![],
        };
        assert_eq!(classify_page(&page, &fonts), PageType::Blank);
    }

    #[test]
    fn test_classify_image_only_page() {
        let fonts = vec![];
        let page = PdfPage {
            number: 1,
            width: 612.0,
            height: 792.0,
            text_elements: vec![],
            images: vec![make_image(612.0, 792.0)],
        };
        assert_eq!(classify_page(&page, &fonts), PageType::ImageOnly);
    }

    #[test]
    fn test_ocr_font_detection() {
        assert!(is_ocr_font("GlyphLessFont"));
        assert!(is_ocr_font("glyphlessfont"));
        assert!(is_ocr_font("Invisible"));
        assert!(!is_ocr_font("TimesNewRomanPSMT"));
        assert!(!is_ocr_font("Arial"));
    }

    #[test]
    fn test_mixed_text_and_images() {
        let fonts = vec![make_font(0, "Arial")];
        let page = PdfPage {
            number: 1,
            width: 612.0,
            height: 792.0,
            text_elements: vec![make_text(0, "Caption text")],
            images: vec![make_image(400.0, 300.0)],
        };
        assert_eq!(classify_page(&page, &fonts), PageType::Text);
    }
}
