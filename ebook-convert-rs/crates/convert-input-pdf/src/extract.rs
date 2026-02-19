//! Hybrid PDF extraction orchestrator.
//!
//! Uses `pdftohtml -xml` as primary extraction for text-based pages,
//! falling back to `pdftoppm` for scanned/composited pages.

use std::collections::HashMap;
use std::path::Path;

use lopdf::Document;
use rayon::prelude::*;

use convert_core::book::{BookDocument, ManifestData, ManifestItem, Metadata, TocEntry};
use convert_core::error::{ConvertError, Result};
use convert_core::options::{ConversionOptions, PdfEngine};

use crate::classify::{self, PageType};
use crate::pdftohtml;
use crate::render;
use crate::text_builder;
use crate::toc;

/// Extract text and images from a PDF file into a BookDocument.
pub fn extract_pdf(path: &Path, options: &ConversionOptions) -> Result<BookDocument> {
    let doc = Document::load(path)
        .map_err(|e| ConvertError::Pdf(format!("Failed to load PDF: {}", e)))?;

    let mut book = BookDocument::new();
    book.base_path = path.parent().map(|p| p.to_path_buf());

    // Extract metadata from PDF info dictionary
    extract_metadata(&doc, &mut book.metadata);

    // Get page count
    let pages = doc.get_pages();
    let mut page_numbers: Vec<u32> = pages.keys().copied().collect();
    page_numbers.sort();
    let num_pages = page_numbers.len() as u32;

    log::info!("PDF has {} pages", num_pages);

    match options.pdf_engine {
        PdfEngine::ImageOnly => {
            extract_image_only(path, &page_numbers, num_pages, options, &mut book)?;
        }
        PdfEngine::Auto | PdfEngine::TextOnly => {
            extract_hybrid(path, &page_numbers, num_pages, options, &mut book)?;
        }
    }

    // Add a default stylesheet
    let css = generate_default_css();
    let css_item = ManifestItem::new(
        "stylesheet",
        "style.css",
        "text/css",
        ManifestData::Css(css),
    );
    book.manifest.add(css_item);

    Ok(book)
}

/// Image-only extraction: render all pages with pdftoppm (legacy behavior).
fn extract_image_only(
    pdf_path: &Path,
    _page_numbers: &[u32],
    num_pages: u32,
    options: &ConversionOptions,
    book: &mut BookDocument,
) -> Result<()> {
    let rendered = render::render_all_pages(pdf_path, num_pages, options)?;

    log::info!(
        "Rendered {} page images ({} bytes total)",
        rendered.len(),
        rendered.iter().map(|(_, d)| d.len()).sum::<usize>()
    );

    // Extract text per page using lopdf (for searchability in image-only mode)
    let doc = Document::load(pdf_path)
        .map_err(|e| ConvertError::Pdf(format!("Failed to load PDF: {}", e)))?;

    for (page_num, jpeg_data) in &rendered {
        let img_id = format!("img{}", page_num);
        let img_href = format!("images/page{}.jpg", page_num);

        if !jpeg_data.is_empty() {
            let img_item = ManifestItem::new(
                &img_id,
                &img_href,
                "image/jpeg",
                ManifestData::Binary(jpeg_data.clone()),
            );
            book.manifest.add(img_item);
        }

        let page_item_id = format!("page{}", page_num);
        let page_href = format!("page{}.xhtml", page_num);
        let text = doc.extract_text(&[*page_num]).unwrap_or_default();

        let image_hrefs = if jpeg_data.is_empty() {
            vec![]
        } else {
            vec![img_href]
        };
        let xhtml = build_image_page_xhtml(*page_num, &text, &image_hrefs);
        let item = ManifestItem::new(
            &page_item_id,
            &page_href,
            "application/xhtml+xml",
            ManifestData::Xhtml(xhtml),
        );
        book.manifest.add(item);
        book.spine.push(&page_item_id, true);

        book.toc
            .add(TocEntry::new(format!("Page {}", page_num), &page_href));
    }

    Ok(())
}

/// Hybrid extraction: use pdftohtml for text, pdftoppm for scanned pages.
fn extract_hybrid(
    pdf_path: &Path,
    page_numbers: &[u32],
    num_pages: u32,
    options: &ConversionOptions,
    book: &mut BookDocument,
) -> Result<()> {
    // Step 1: Run pdftohtml (parallel for large documents)
    let pdftohtml_result = match pdftohtml::run_pdftohtml_xml_parallel(pdf_path, num_pages) {
        Ok(result) => result,
        Err(e) => {
            log::warn!("pdftohtml failed: {}. Falling back to image-only mode.", e);
            return extract_image_only(pdf_path, page_numbers, num_pages, options, book);
        }
    };

    let fonts = &pdftohtml_result.fonts;
    let html_pages = &pdftohtml_result.pages;

    // Step 2: Classify each page (parallel)
    let classifications: Vec<(u32, PageType)> = html_pages
        .par_iter()
        .map(|page| {
            let page_type = classify::classify_page(page, fonts);
            log::debug!("Page {}: {:?}", page.number, page_type);
            (page.number, page_type)
        })
        .collect();

    let text_count = classifications
        .iter()
        .filter(|(_, t)| *t == PageType::Text)
        .count() as u32;
    let scanned_count = classifications
        .iter()
        .filter(|(_, t)| *t == PageType::Scanned)
        .count() as u32;

    log::info!(
        "Classification: {} text, {} scanned, {} total pages",
        text_count,
        scanned_count,
        html_pages.len()
    );

    // Step 3: If Auto mode and 0 text pages, fall back to image-only
    if options.pdf_engine == PdfEngine::Auto && text_count == 0 {
        log::info!("No text pages found, falling back to image-only mode.");
        return extract_image_only(pdf_path, page_numbers, num_pages, options, book);
    }

    // Step 4: Batch-render scanned pages with pdftoppm
    let scanned_pages: Vec<u32> = classifications
        .iter()
        .filter(|(_, t)| *t == PageType::Scanned)
        .map(|(n, _)| *n)
        .collect();

    let rendered_scanned = if !scanned_pages.is_empty() {
        render::render_page_ranges(pdf_path, &scanned_pages, num_pages, options)?
    } else {
        HashMap::new()
    };

    // Step 5: Load pdftohtml-extracted images into manifest
    // Collect image entries with their paths (search across all image dirs)
    let image_dirs = &pdftohtml_result.image_dirs;
    let image_entries: Vec<(u32, String, std::path::PathBuf)> = html_pages
        .iter()
        .flat_map(|page| {
            page.images.iter().filter_map(move |img_elem| {
                image_dirs
                    .iter()
                    .find_map(|dir| {
                        let src_path = dir.join(&img_elem.src);
                        if src_path.exists() {
                            Some(src_path)
                        } else {
                            None
                        }
                    })
                    .map(|src_path| (page.number, img_elem.src.clone(), src_path))
            })
        })
        .collect();

    // Read image files in parallel
    let loaded_images: Vec<(u32, String, Vec<u8>)> = image_entries
        .par_iter()
        .filter_map(|(page_num, src, path)| {
            std::fs::read(path).ok().and_then(|data| {
                if data.is_empty() {
                    None
                } else {
                    Some((*page_num, src.clone(), data))
                }
            })
        })
        .collect();

    // Add to manifest sequentially
    let mut image_map: HashMap<String, String> = HashMap::new();
    let mut img_counter = 0u32;

    for (page_num, src, data) in loaded_images {
        img_counter += 1;
        let epub_href = format!("images/page{}_{}.jpg", page_num, img_counter);
        let mime = if src.ends_with(".png") {
            "image/png"
        } else {
            "image/jpeg"
        };
        let img_id = format!("img_{}_{}", page_num, img_counter);
        let item = ManifestItem::new(&img_id, &epub_href, mime, ManifestData::Binary(data));
        book.manifest.add(item);
        image_map.insert(src, epub_href);
    }

    // Step 6: Build XHTML per page based on classification (parallel build, sequential apply)
    // First, collect scanned page images that need to be added to manifest
    let mut scanned_images: Vec<(u32, String, String, Vec<u8>)> = Vec::new();
    for (page_num, page_type) in &classifications {
        if *page_type == PageType::Scanned {
            if let Some(jpeg_data) = rendered_scanned.get(page_num) {
                if !jpeg_data.is_empty() {
                    let img_id = format!("img_scan{}", page_num);
                    let img_href = format!("images/scan_page{}.jpg", page_num);
                    scanned_images.push((*page_num, img_id, img_href, jpeg_data.clone()));
                }
            }
        }
    }

    // Add scanned images to manifest and build href lookup
    let mut scanned_href_map: HashMap<u32, String> = HashMap::new();
    for (page_num, img_id, img_href, jpeg_data) in scanned_images {
        let item = ManifestItem::new(
            &img_id,
            &img_href,
            "image/jpeg",
            ManifestData::Binary(jpeg_data),
        );
        book.manifest.add(item);
        scanned_href_map.insert(page_num, img_href);
    }

    // Build XHTML content in parallel
    let page_xhtmls: Vec<(u32, String, String, String)> = classifications
        .par_iter()
        .map(|(page_num, page_type)| {
            let page_item_id = format!("page{}", page_num);
            let page_href = format!("page{}.xhtml", page_num);

            let xhtml = match page_type {
                PageType::Text => {
                    let pdf_page = html_pages.iter().find(|p| p.number == *page_num);
                    match pdf_page {
                        Some(page) => text_builder::build_text_page_xhtml(page, fonts, &image_map),
                        None => build_placeholder_xhtml(*page_num),
                    }
                }
                PageType::Scanned => match scanned_href_map.get(page_num) {
                    Some(img_href) => build_scanned_page_xhtml(*page_num, img_href),
                    None => build_placeholder_xhtml(*page_num),
                },
                PageType::ImageOnly => {
                    let pdf_page = html_pages.iter().find(|p| p.number == *page_num);
                    let image_hrefs: Vec<String> = pdf_page
                        .map(|p| {
                            p.images
                                .iter()
                                .filter_map(|img| image_map.get(&img.src).cloned())
                                .collect()
                        })
                        .unwrap_or_default();
                    build_image_page_xhtml(*page_num, "", &image_hrefs)
                }
                PageType::Blank => build_placeholder_xhtml(*page_num),
            };

            (*page_num, page_item_id, page_href, xhtml)
        })
        .collect();

    // Apply results to manifest sequentially
    let mut page_href_map: HashMap<u32, String> = HashMap::new();
    for (page_num, page_item_id, page_href, xhtml) in page_xhtmls {
        page_href_map.insert(page_num, page_href.clone());
        let item = ManifestItem::new(
            &page_item_id,
            &page_href,
            "application/xhtml+xml",
            ManifestData::Xhtml(xhtml),
        );
        book.manifest.add(item);
        book.spine.push(&page_item_id, true);
    }

    // Step 7: Build TOC from outline
    let toc_entries = toc::build_toc(
        &pdftohtml_result.outline,
        &page_href_map,
        num_pages,
        3, // min_entries for outline-based TOC
    );
    for entry in toc_entries {
        book.toc.add(entry);
    }

    Ok(())
}

/// Build XHTML for a scanned page (full-page image).
fn build_scanned_page_xhtml(page_num: u32, img_href: &str) -> String {
    let body = format!(
        "  <div class=\"page\">\n    <div class=\"page-image\"><img src=\"{}\" alt=\"Page {}\"/></div>\n  </div>",
        convert_utils::xml::escape_xml_attr(img_href),
        page_num
    );
    convert_utils::xml::xhtml11_document(
        &format!("Page {}", page_num),
        "en",
        Some("style.css"),
        &body,
    )
}

/// Build XHTML for a placeholder (blank) page.
fn build_placeholder_xhtml(page_num: u32) -> String {
    let body = format!(
        "  <div class=\"page\">\n    <p class=\"empty-page\">[Page {}]</p>\n  </div>",
        page_num
    );
    convert_utils::xml::xhtml11_document(
        &format!("Page {}", page_num),
        "en",
        Some("style.css"),
        &body,
    )
}

/// Build an XHTML page from text and image references (for image-only mode).
fn build_image_page_xhtml(page_num: u32, text: &str, image_hrefs: &[String]) -> String {
    let mut body = String::new();
    body.push_str("  <div class=\"page\">\n");

    let has_text = !text.trim().is_empty();
    let has_images = !image_hrefs.is_empty();

    if has_text {
        for para in text.split("\n\n") {
            let para = para.trim();
            if !para.is_empty() {
                let escaped = para
                    .replace('&', "&amp;")
                    .replace('<', "&lt;")
                    .replace('>', "&gt;");
                body.push_str(&format!("    <p>{}</p>\n", escaped));
            }
        }
    }

    for href in image_hrefs {
        body.push_str(&format!(
            "    <div class=\"page-image\"><img src=\"{}\" alt=\"Page {} image\"/></div>\n",
            convert_utils::xml::escape_xml_attr(href),
            page_num
        ));
    }

    if !has_text && !has_images {
        body.push_str(&format!(
            "    <p class=\"empty-page\">[Page {}]</p>\n",
            page_num
        ));
    }

    body.push_str("  </div>");

    convert_utils::xml::xhtml11_document(
        &format!("Page {}", page_num),
        "en",
        Some("style.css"),
        &body,
    )
}

/// Decode a PDF string, handling UTF-16BE (BOM 0xFE 0xFF) and PDFDocEncoding.
fn decode_pdf_string(raw: &[u8]) -> String {
    // UTF-16BE: starts with BOM 0xFE 0xFF
    if raw.len() >= 2 && raw[0] == 0xFE && raw[1] == 0xFF {
        let chars: Vec<u16> = raw[2..]
            .chunks_exact(2)
            .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
            .collect();
        return String::from_utf16_lossy(&chars);
    }

    // UTF-8 BOM
    if raw.len() >= 3 && raw[0] == 0xEF && raw[1] == 0xBB && raw[2] == 0xBF {
        return String::from_utf8_lossy(&raw[3..]).to_string();
    }

    // PDFDocEncoding / Latin-1 fallback: bytes 0x80-0xFF map to Unicode
    // For most practical PDFs, treating as Latin-1 is sufficient
    if raw.is_ascii() {
        return String::from_utf8_lossy(raw).to_string();
    }

    // Try UTF-8 first, fall back to Latin-1
    match std::str::from_utf8(raw) {
        Ok(s) => s.to_string(),
        Err(_) => raw.iter().map(|&b| b as char).collect(),
    }
}

/// Extract metadata from the PDF info dictionary.
fn extract_metadata(doc: &Document, metadata: &mut Metadata) {
    if let Ok(info) = doc.trailer.get(b"Info") {
        if let Ok(info_ref) = info.as_reference() {
            if let Ok(info_obj) = doc.get_object(info_ref) {
                if let Ok(dict) = info_obj.as_dict() {
                    if let Ok(title) = dict.get(b"Title") {
                        if let Ok(s) = title.as_str() {
                            let s = decode_pdf_string(s);
                            let s = s.trim().to_string();
                            if !s.is_empty() {
                                metadata.set_title(s);
                            }
                        }
                    }
                    if let Ok(author) = dict.get(b"Author") {
                        if let Ok(s) = author.as_str() {
                            let s = decode_pdf_string(s);
                            let s = s.trim().to_string();
                            if !s.is_empty() {
                                metadata.add("creator", s);
                            }
                        }
                    }
                    if let Ok(subject) = dict.get(b"Subject") {
                        if let Ok(s) = subject.as_str() {
                            let s = decode_pdf_string(s);
                            let s = s.trim().to_string();
                            if !s.is_empty() {
                                metadata.add("description", s);
                            }
                        }
                    }
                }
            }
        }
    }

    if !metadata.contains("language") {
        metadata.set("language", "en");
    }

    if metadata.title().is_none() {
        metadata.set_title("Untitled PDF");
    }
}

/// Generate a default CSS stylesheet for PDF-extracted content.
fn generate_default_css() -> String {
    r#"body {
    margin: 0;
    padding: 0;
    font-family: serif;
    line-height: 1.6;
}

.page {
    margin: 0;
    padding: 0;
}

p {
    margin: 0.5em 1em;
    text-indent: 0;
}

.empty-page {
    color: #999;
    font-style: italic;
    text-align: center;
    padding: 2em 0;
}

.page-image {
    text-align: center;
    margin: 0;
    padding: 0;
}

img {
    max-width: 100%;
    height: auto;
    display: block;
}
"#
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_pdf_string_utf16be() {
        // UTF-16BE BOM + "The Art"
        let raw: Vec<u8> = vec![
            0xFE, 0xFF, // BOM
            0x00, 0x54, // T
            0x00, 0x68, // h
            0x00, 0x65, // e
            0x00, 0x20, // space
            0x00, 0x41, // A
            0x00, 0x72, // r
            0x00, 0x74, // t
        ];
        assert_eq!(decode_pdf_string(&raw), "The Art");
    }

    #[test]
    fn test_decode_pdf_string_ascii() {
        assert_eq!(decode_pdf_string(b"Hello World"), "Hello World");
    }

    #[test]
    fn test_decode_pdf_string_utf8() {
        assert_eq!(decode_pdf_string("Héllo".as_bytes()), "Héllo");
    }

    #[test]
    fn test_decode_pdf_string_latin1() {
        // Latin-1: 0xE9 = é (not valid UTF-8 by itself)
        let raw = vec![0x48, 0xE9, 0x6C, 0x6C, 0x6F]; // Héllo in Latin-1
        assert_eq!(decode_pdf_string(&raw), "Héllo");
    }

    #[test]
    fn test_build_image_page_xhtml_with_text() {
        let xhtml = build_image_page_xhtml(1, "Hello World\n\nSecond paragraph", &[]);
        assert!(xhtml.contains("<title>Page 1</title>"));
        assert!(xhtml.contains("<p>Hello World</p>"));
        assert!(xhtml.contains("<p>Second paragraph</p>"));
        assert!(xhtml.contains("XHTML 1.1"));
    }

    #[test]
    fn test_build_image_page_xhtml_with_images() {
        let hrefs = vec!["images/img1.jpg".to_string(), "images/img2.png".to_string()];
        let xhtml = build_image_page_xhtml(3, "Some text", &hrefs);
        assert!(xhtml.contains("<img src=\"images/img1.jpg\""));
        assert!(xhtml.contains("<img src=\"images/img2.png\""));
        assert!(xhtml.contains("Some text"));
    }

    #[test]
    fn test_build_image_page_xhtml_empty() {
        let xhtml = build_image_page_xhtml(1, "", &[]);
        assert!(xhtml.contains("[Page 1]"));
    }

    #[test]
    fn test_build_scanned_page_xhtml() {
        let xhtml = build_scanned_page_xhtml(5, "images/scan_page5.jpg");
        assert!(xhtml.contains("<img src=\"images/scan_page5.jpg\""));
        assert!(xhtml.contains("alt=\"Page 5\""));
        assert!(xhtml.contains("XHTML 1.1"));
    }

    #[test]
    fn test_build_placeholder_xhtml() {
        let xhtml = build_placeholder_xhtml(10);
        assert!(xhtml.contains("[Page 10]"));
        assert!(xhtml.contains("empty-page"));
    }

    #[test]
    fn test_generate_default_css() {
        let css = generate_default_css();
        assert!(css.contains("font-family: serif"));
        assert!(css.contains(".page-image"));
    }
}
