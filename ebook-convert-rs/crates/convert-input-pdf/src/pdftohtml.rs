//! Run `pdftohtml -xml` and parse its XML output.
//!
//! pdftohtml (poppler) extracts structured text, fonts, and images from PDF files.
//! We parse the XML output to get per-page text elements and image references.

use std::path::{Path, PathBuf};
use std::process::Command;

use quick_xml::events::Event;
use quick_xml::Reader;

use convert_core::error::{ConvertError, Result};

/// A font specification from the pdftohtml XML output.
#[derive(Debug, Clone)]
pub struct FontSpec {
    pub id: u32,
    pub size: f64,
    pub family: String,
    pub color: String,
}

/// A text element on a page.
#[derive(Debug, Clone)]
pub struct TextElement {
    pub top: f64,
    pub left: f64,
    pub width: f64,
    pub height: f64,
    pub font_id: u32,
    /// Raw inner HTML (may contain `<b>`, `<i>`, `<a>` tags).
    pub inner_html: String,
}

impl TextElement {
    /// Extract plain text (strip HTML tags).
    pub fn inner_text(&self) -> String {
        let mut result = String::with_capacity(self.inner_html.len());
        let mut in_tag = false;
        for ch in self.inner_html.chars() {
            match ch {
                '<' => in_tag = true,
                '>' => in_tag = false,
                _ if !in_tag => result.push(ch),
                _ => {}
            }
        }
        result
    }
}

/// An image element on a page.
#[derive(Debug, Clone)]
pub struct ImageElement {
    pub top: f64,
    pub left: f64,
    pub width: f64,
    pub height: f64,
    pub src: String,
}

/// A single page from the pdftohtml XML output.
#[derive(Debug, Clone)]
pub struct PdfPage {
    pub number: u32,
    pub width: f64,
    pub height: f64,
    pub text_elements: Vec<TextElement>,
    pub images: Vec<ImageElement>,
}

/// An outline/bookmark item from the PDF.
#[derive(Debug, Clone)]
pub struct OutlineItem {
    pub title: String,
    pub page: u32,
    pub children: Vec<OutlineItem>,
}

/// Result of running pdftohtml and parsing its XML output.
pub struct PdfToHtmlResult {
    pub fonts: Vec<FontSpec>,
    pub pages: Vec<PdfPage>,
    pub outline: Vec<OutlineItem>,
    /// Directories containing extracted images (one per chunk in parallel mode).
    pub image_dirs: Vec<PathBuf>,
    /// Keep temp dirs alive until we're done with them.
    pub _tmp_dirs: Vec<tempfile::TempDir>,
}

/// Run `pdftohtml -xml` on a PDF and parse the resulting XML.
pub fn run_pdftohtml_xml(pdf_path: &Path) -> Result<PdfToHtmlResult> {
    // Check that pdftohtml is available
    let which = Command::new("which")
        .arg("pdftohtml")
        .output()
        .map_err(|e| ConvertError::Pdf(format!("Failed to check for pdftohtml: {}", e)))?;

    if !which.status.success() {
        return Err(ConvertError::Pdf(
            "pdftohtml (poppler-utils) is required for PDF conversion. \
             Install with: brew install poppler (macOS) or apt install poppler-utils (Linux)"
                .to_string(),
        ));
    }

    let tmp_dir = tempfile::TempDir::new()
        .map_err(|e| ConvertError::Pdf(format!("Failed to create temp dir: {}", e)))?;

    let output_base = tmp_dir.path().join("output");
    let output_base_str = output_base
        .to_str()
        .ok_or_else(|| ConvertError::Pdf("Invalid temp path".to_string()))?;

    log::info!("Running pdftohtml -xml on {}...", pdf_path.display());

    let output = Command::new("pdftohtml")
        .arg("-xml")
        .arg("-enc")
        .arg("UTF-8")
        .arg("-noframes")
        .arg("-p")
        .arg("-nomerge")
        .arg("-nodrm")
        .arg("-fmt")
        .arg("jpg")
        .arg(pdf_path.as_os_str())
        .arg(output_base_str)
        .output()
        .map_err(|e| ConvertError::Pdf(format!("Failed to run pdftohtml: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(ConvertError::Pdf(format!("pdftohtml failed: {}", stderr)));
    }

    // The XML file is at output_base.xml
    let xml_path = tmp_dir.path().join("output.xml");
    let xml_content = std::fs::read_to_string(&xml_path).map_err(|e| {
        ConvertError::Pdf(format!(
            "Failed to read pdftohtml XML output at {}: {}",
            xml_path.display(),
            e
        ))
    })?;

    let (fonts, pages, outline) = parse_pdftohtml_xml(&xml_content)?;

    log::info!(
        "pdftohtml: {} fonts, {} pages, {} outline items",
        fonts.len(),
        pages.len(),
        outline.len()
    );

    Ok(PdfToHtmlResult {
        fonts,
        pages,
        outline,
        image_dirs: vec![tmp_dir.path().to_path_buf()],
        _tmp_dirs: vec![tmp_dir],
    })
}

/// Minimum page count to trigger parallel extraction.
const PARALLEL_MIN_PAGES: u32 = 50;

/// Maximum number of parallel pdftohtml worker processes.
const PARALLEL_MAX_WORKERS: usize = 8;

/// Run `pdftohtml -xml` in parallel by splitting into page-range chunks.
///
/// For documents with more than [`PARALLEL_MIN_PAGES`] pages, spawns multiple
/// `pdftohtml` processes (one per chunk) using `std::thread::scope`, then merges
/// their results. The outline is extracted separately from the full document.
///
/// For small documents (≤50 pages), delegates to [`run_pdftohtml_xml`].
pub fn run_pdftohtml_xml_parallel(pdf_path: &Path, num_pages: u32) -> Result<PdfToHtmlResult> {
    if num_pages <= PARALLEL_MIN_PAGES {
        return run_pdftohtml_xml(pdf_path);
    }

    // Check that pdftohtml is available
    let which = Command::new("which")
        .arg("pdftohtml")
        .output()
        .map_err(|e| ConvertError::Pdf(format!("Failed to check for pdftohtml: {}", e)))?;

    if !which.status.success() {
        return Err(ConvertError::Pdf(
            "pdftohtml (poppler-utils) is required for PDF conversion. \
             Install with: brew install poppler (macOS) or apt install poppler-utils (Linux)"
                .to_string(),
        ));
    }

    let num_workers = std::thread::available_parallelism()
        .map(|n| n.get().min(PARALLEL_MAX_WORKERS))
        .unwrap_or(4);
    let chunk_size = (num_pages as usize).div_ceil(num_workers);

    log::info!(
        "Running pdftohtml -xml in parallel: {} pages across {} workers (chunk size {})...",
        num_pages,
        num_workers,
        chunk_size
    );

    // Build chunk ranges: (first_page, last_page)
    let mut chunks: Vec<(u32, u32)> = Vec::new();
    let mut start = 1u32;
    while start <= num_pages {
        let end = (start + chunk_size as u32 - 1).min(num_pages);
        chunks.push((start, end));
        start = end + 1;
    }

    // Result type for each parallel chunk
    type ChunkResult = Result<(Vec<FontSpec>, Vec<PdfPage>, PathBuf, tempfile::TempDir)>;

    // Spawn parallel pdftohtml processes using scoped threads
    let chunk_results: Vec<ChunkResult> = std::thread::scope(|s| {
        let handles: Vec<_> = chunks
            .iter()
            .map(|&(first, last)| {
                s.spawn(move || -> ChunkResult {
                    log::info!(
                        "[pdftohtml] Processing pages {}-{} of {}...",
                        first,
                        last,
                        num_pages
                    );

                    let tmp_dir = tempfile::TempDir::new().map_err(|e| {
                        ConvertError::Pdf(format!("Failed to create temp dir: {}", e))
                    })?;

                    let output_base = tmp_dir.path().join("output");
                    let output_base_str = output_base
                        .to_str()
                        .ok_or_else(|| ConvertError::Pdf("Invalid temp path".to_string()))?;

                    let output = Command::new("pdftohtml")
                        .arg("-xml")
                        .arg("-enc")
                        .arg("UTF-8")
                        .arg("-noframes")
                        .arg("-p")
                        .arg("-nomerge")
                        .arg("-nodrm")
                        .arg("-fmt")
                        .arg("jpg")
                        .arg("-f")
                        .arg(first.to_string())
                        .arg("-l")
                        .arg(last.to_string())
                        .arg(pdf_path.as_os_str())
                        .arg(output_base_str)
                        .output()
                        .map_err(|e| {
                            ConvertError::Pdf(format!("Failed to run pdftohtml: {}", e))
                        })?;

                    if !output.status.success() {
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        return Err(ConvertError::Pdf(format!(
                            "pdftohtml failed for pages {}-{}: {}",
                            first, last, stderr
                        )));
                    }

                    let xml_path = tmp_dir.path().join("output.xml");
                    let xml_content = std::fs::read_to_string(&xml_path).map_err(|e| {
                        ConvertError::Pdf(format!(
                            "Failed to read pdftohtml XML output at {}: {}",
                            xml_path.display(),
                            e
                        ))
                    })?;

                    let (fonts, pages, _outline) = parse_pdftohtml_xml(&xml_content)?;
                    let image_dir = tmp_dir.path().to_path_buf();
                    Ok((fonts, pages, image_dir, tmp_dir))
                })
            })
            .collect();

        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });

    // Extract outline separately from the full document (fast with -i to skip images)
    let outline = extract_outline_only(pdf_path)?;

    // Merge results from all chunks
    let mut all_fonts: Vec<FontSpec> = Vec::new();
    let mut all_pages: Vec<PdfPage> = Vec::new();
    let mut image_dirs: Vec<PathBuf> = Vec::new();
    let mut tmp_dirs: Vec<tempfile::TempDir> = Vec::new();
    let mut seen_font_ids = std::collections::HashSet::new();

    for chunk_result in chunk_results {
        let (fonts, pages, image_dir, tmp_dir) = chunk_result?;

        // Deduplicate fonts by ID (each chunk may re-declare the same fonts)
        for font in fonts {
            if seen_font_ids.insert(font.id) {
                all_fonts.push(font);
            }
        }

        all_pages.extend(pages);
        image_dirs.push(image_dir);
        tmp_dirs.push(tmp_dir);
    }

    // Sort pages by page number to ensure correct order
    all_pages.sort_by_key(|p| p.number);

    log::info!(
        "pdftohtml parallel: {} fonts, {} pages, {} outline items",
        all_fonts.len(),
        all_pages.len(),
        outline.len()
    );

    Ok(PdfToHtmlResult {
        fonts: all_fonts,
        pages: all_pages,
        outline,
        image_dirs,
        _tmp_dirs: tmp_dirs,
    })
}

/// Extract only the outline/TOC from a PDF using `pdftohtml -xml -i` (skip images for speed).
fn extract_outline_only(pdf_path: &Path) -> Result<Vec<OutlineItem>> {
    let tmp_dir = tempfile::TempDir::new()
        .map_err(|e| ConvertError::Pdf(format!("Failed to create temp dir: {}", e)))?;

    let output_base = tmp_dir.path().join("output");
    let output_base_str = output_base
        .to_str()
        .ok_or_else(|| ConvertError::Pdf("Invalid temp path".to_string()))?;

    let output = Command::new("pdftohtml")
        .arg("-xml")
        .arg("-enc")
        .arg("UTF-8")
        .arg("-i") // ignore images — faster for outline extraction
        .arg("-noframes")
        .arg("-nodrm")
        .arg(pdf_path.as_os_str())
        .arg(output_base_str)
        .output()
        .map_err(|e| ConvertError::Pdf(format!("Failed to run pdftohtml for outline: {}", e)))?;

    if !output.status.success() {
        // Non-fatal: return empty outline
        log::warn!("pdftohtml outline extraction failed, continuing without outline");
        return Ok(Vec::new());
    }

    let xml_path = tmp_dir.path().join("output.xml");
    let xml_content = std::fs::read_to_string(&xml_path).unwrap_or_default();

    if xml_content.is_empty() {
        return Ok(Vec::new());
    }

    let (_fonts, _pages, outline) = parse_pdftohtml_xml(&xml_content)?;
    Ok(outline)
}

/// Parse the pdftohtml XML output.
fn parse_pdftohtml_xml(xml: &str) -> Result<(Vec<FontSpec>, Vec<PdfPage>, Vec<OutlineItem>)> {
    let mut reader = Reader::from_str(xml);
    let mut fonts: Vec<FontSpec> = Vec::new();
    let mut pages: Vec<PdfPage> = Vec::new();
    let mut outline: Vec<OutlineItem> = Vec::new();

    // State tracking
    let mut current_page: Option<PdfPage> = None;
    let mut in_text = false;
    let mut current_text: Option<TextElement> = None;
    let mut text_html = String::new();
    let mut outline_stack: Vec<Vec<OutlineItem>> = vec![Vec::new()];

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) => {
                let tag = e.local_name();
                let tag_str = std::str::from_utf8(tag.as_ref()).unwrap_or("");

                match tag_str {
                    "page" => {
                        let attrs = parse_attrs(e);
                        let page = PdfPage {
                            number: attrs
                                .get("number")
                                .and_then(|v| v.parse().ok())
                                .unwrap_or(0),
                            width: attrs
                                .get("width")
                                .and_then(|v| v.parse().ok())
                                .unwrap_or(0.0),
                            height: attrs
                                .get("height")
                                .and_then(|v| v.parse().ok())
                                .unwrap_or(0.0),
                            text_elements: Vec::new(),
                            images: Vec::new(),
                        };
                        current_page = Some(page);
                    }
                    "text" => {
                        let attrs = parse_attrs(e);
                        let te = TextElement {
                            top: attrs.get("top").and_then(|v| v.parse().ok()).unwrap_or(0.0),
                            left: attrs
                                .get("left")
                                .and_then(|v| v.parse().ok())
                                .unwrap_or(0.0),
                            width: attrs
                                .get("width")
                                .and_then(|v| v.parse().ok())
                                .unwrap_or(0.0),
                            height: attrs
                                .get("height")
                                .and_then(|v| v.parse().ok())
                                .unwrap_or(0.0),
                            font_id: attrs.get("font").and_then(|v| v.parse().ok()).unwrap_or(0),
                            inner_html: String::new(),
                        };
                        current_text = Some(te);
                        text_html.clear();
                        in_text = true;
                    }
                    "outline" => {
                        // outline_stack already has the root level
                    }
                    "item" if !outline_stack.is_empty() => {
                        // Push a new child level
                        outline_stack.push(Vec::new());
                    }
                    // Inline formatting tags inside <text>: preserve them
                    tag if in_text && matches!(tag, "b" | "i" | "a" | "sup" | "sub") => {
                        text_html.push('<');
                        text_html.push_str(tag);
                        // Copy attributes for <a> tags
                        if tag == "a" {
                            let attrs = parse_attrs(e);
                            if let Some(href) = attrs.get("href") {
                                text_html.push_str(" href=\"");
                                text_html.push_str(&convert_utils::xml::escape_xml_attr(href));
                                text_html.push('"');
                            }
                        }
                        text_html.push('>');
                    }
                    _ => {}
                }
            }
            Ok(Event::Empty(ref e)) => {
                let tag = e.local_name();
                let tag_str = std::str::from_utf8(tag.as_ref()).unwrap_or("");

                match tag_str {
                    "fontspec" => {
                        let attrs = parse_attrs(e);
                        let font = FontSpec {
                            id: attrs.get("id").and_then(|v| v.parse().ok()).unwrap_or(0),
                            size: attrs
                                .get("size")
                                .and_then(|v| v.parse().ok())
                                .unwrap_or(0.0),
                            family: attrs.get("family").cloned().unwrap_or_default(),
                            color: attrs
                                .get("color")
                                .cloned()
                                .unwrap_or_else(|| "#000000".to_string()),
                        };
                        fonts.push(font);
                    }
                    "image" => {
                        let attrs = parse_attrs(e);
                        let img = ImageElement {
                            top: attrs.get("top").and_then(|v| v.parse().ok()).unwrap_or(0.0),
                            left: attrs
                                .get("left")
                                .and_then(|v| v.parse().ok())
                                .unwrap_or(0.0),
                            width: attrs
                                .get("width")
                                .and_then(|v| v.parse().ok())
                                .unwrap_or(0.0),
                            height: attrs
                                .get("height")
                                .and_then(|v| v.parse().ok())
                                .unwrap_or(0.0),
                            src: attrs.get("src").cloned().unwrap_or_default(),
                        };
                        if let Some(ref mut page) = current_page {
                            page.images.push(img);
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(ref e)) => {
                if in_text {
                    if let Ok(text) = e.unescape() {
                        // Escape for embedding in our inner_html
                        text_html.push_str(&text);
                    }
                }
                // Handle outline item text
                if outline_stack.len() > 1 {
                    // We're inside an <item> tag, but text is handled at End
                }
            }
            Ok(Event::End(ref e)) => {
                let tag = e.local_name();
                let tag_str = std::str::from_utf8(tag.as_ref()).unwrap_or("");

                match tag_str {
                    "page" => {
                        if let Some(page) = current_page.take() {
                            pages.push(page);
                        }
                    }
                    "text" => {
                        if let Some(mut te) = current_text.take() {
                            te.inner_html = text_html.clone();
                            if let Some(ref mut page) = current_page {
                                page.text_elements.push(te);
                            }
                        }
                        in_text = false;
                    }
                    "outline" => {
                        if let Some(items) = outline_stack.pop() {
                            outline = items;
                        }
                    }
                    "item" if outline_stack.len() > 1 => {
                        // Pop child level and attach to parent
                        if let Some(children) = outline_stack.pop() {
                            // The item text was captured during parsing;
                            // for pdftohtml, <item page="N">Title</item>
                            // We handle this in the item Start/Text/End cycle
                            // Actually, pdftohtml outline items are self-contained
                            let _ = children; // nested outlines handled below
                        }
                    }
                    tag if in_text && matches!(tag, "b" | "i" | "a" | "sup" | "sub") => {
                        text_html.push_str("</");
                        text_html.push_str(tag);
                        text_html.push('>');
                    }
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                log::warn!("XML parse warning: {}", e);
                break;
            }
            _ => {}
        }
    }

    // Parse outline from XML more carefully — pdftohtml uses a simple structure:
    // <outline>
    //   <item page="1">Chapter 1</item>
    //   <item page="5">Chapter 2</item>
    // </outline>
    // Let's re-parse just the outline section if we haven't captured it
    if outline.is_empty() {
        outline = parse_outline_section(xml);
    }

    Ok((fonts, pages, outline))
}

/// Parse the outline section specifically.
fn parse_outline_section(xml: &str) -> Vec<OutlineItem> {
    let mut reader = Reader::from_str(xml);
    let mut items = Vec::new();
    let mut in_outline = false;
    let mut current_item_page: Option<u32> = None;
    let mut item_text = String::new();
    let mut depth: usize = 0;
    let mut stack: Vec<Vec<OutlineItem>> = Vec::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) => {
                let local = e.local_name();
                let tag = std::str::from_utf8(local.as_ref()).unwrap_or("");
                match tag {
                    "outline" => {
                        in_outline = true;
                        stack.push(Vec::new());
                    }
                    "item" if in_outline => {
                        let attrs = parse_attrs(e);
                        current_item_page = attrs.get("page").and_then(|v| v.parse().ok());
                        item_text.clear();
                        depth += 1;
                        stack.push(Vec::new());
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(ref e)) => {
                if current_item_page.is_some() {
                    if let Ok(text) = e.unescape() {
                        item_text.push_str(&text);
                    }
                }
            }
            Ok(Event::End(ref e)) => {
                let local = e.local_name();
                let tag = std::str::from_utf8(local.as_ref()).unwrap_or("");
                match tag {
                    "item" if in_outline && depth > 0 => {
                        let children = stack.pop().unwrap_or_default();
                        let item = OutlineItem {
                            title: item_text.trim().to_string(),
                            page: current_item_page.unwrap_or(1),
                            children,
                        };
                        current_item_page = None;
                        item_text.clear();
                        depth -= 1;
                        if let Some(parent) = stack.last_mut() {
                            parent.push(item);
                        }
                    }
                    "outline" => {
                        in_outline = false;
                        items = stack.pop().unwrap_or_default();
                    }
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
    }

    items
}

/// Helper to parse attributes from a quick-xml event.
fn parse_attrs(e: &quick_xml::events::BytesStart) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    for attr in e.attributes().flatten() {
        let key = String::from_utf8_lossy(attr.key.local_name().as_ref()).to_string();
        let value = String::from_utf8_lossy(&attr.value).to_string();
        map.insert(key, value);
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_xml() {
        let xml = r##"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE pdf2xml SYSTEM "pdf2xml.dtd">
<pdf2xml>
<page number="1" width="612.000000" height="792.000000">
<fontspec id="0" size="12" family="TimesNewRomanPSMT" color="#000000"/>
<text top="100" left="50" width="200" height="14" font="0">Hello <b>world</b></text>
<image top="300" left="50" width="500" height="400" src="output001.jpg"/>
</page>
</pdf2xml>"##;

        let (fonts, pages, _outline) = parse_pdftohtml_xml(xml).unwrap();

        assert_eq!(fonts.len(), 1);
        assert_eq!(fonts[0].family, "TimesNewRomanPSMT");
        assert_eq!(fonts[0].size, 12.0);

        assert_eq!(pages.len(), 1);
        let page = &pages[0];
        assert_eq!(page.number, 1);
        assert_eq!(page.width, 612.0);
        assert_eq!(page.text_elements.len(), 1);
        assert_eq!(page.text_elements[0].inner_html, "Hello <b>world</b>");
        assert_eq!(page.images.len(), 1);
        assert_eq!(page.images[0].src, "output001.jpg");
    }

    #[test]
    fn test_parse_multiple_pages() {
        let xml = r##"<?xml version="1.0" encoding="UTF-8"?>
<pdf2xml>
<page number="1" width="612" height="792">
<fontspec id="0" size="14" family="Arial" color="#000000"/>
<text top="100" left="50" width="200" height="16" font="0">Page one</text>
</page>
<page number="2" width="612" height="792">
<text top="100" left="50" width="200" height="16" font="0">Page two</text>
</page>
</pdf2xml>"##;

        let (fonts, pages, _) = parse_pdftohtml_xml(xml).unwrap();

        assert_eq!(fonts.len(), 1);
        assert_eq!(pages.len(), 2);
        assert_eq!(pages[0].number, 1);
        assert_eq!(pages[1].number, 2);
        assert_eq!(pages[0].text_elements[0].inner_text(), "Page one");
        assert_eq!(pages[1].text_elements[0].inner_text(), "Page two");
    }

    #[test]
    fn test_parse_outline() {
        let xml = r##"<?xml version="1.0" encoding="UTF-8"?>
<pdf2xml>
<outline>
<item page="1">Introduction</item>
<item page="5">Chapter 1</item>
<item page="20">Chapter 2</item>
</outline>
<page number="1" width="612" height="792">
</page>
</pdf2xml>"##;

        let (_, _, outline) = parse_pdftohtml_xml(xml).unwrap();

        assert_eq!(outline.len(), 3);
        assert_eq!(outline[0].title, "Introduction");
        assert_eq!(outline[0].page, 1);
        assert_eq!(outline[1].title, "Chapter 1");
        assert_eq!(outline[1].page, 5);
    }

    #[test]
    fn test_text_element_inner_text() {
        let te = TextElement {
            top: 0.0,
            left: 0.0,
            width: 100.0,
            height: 14.0,
            font_id: 0,
            inner_html: "Hello <b>bold</b> and <i>italic</i>".to_string(),
        };
        assert_eq!(te.inner_text(), "Hello bold and italic");
    }

    #[test]
    fn test_parse_scanned_page() {
        let xml = r##"<?xml version="1.0" encoding="UTF-8"?>
<pdf2xml>
<page number="1" width="612" height="792">
<fontspec id="0" size="11" family="GlyphLessFont" color="#000000"/>
<text top="100" left="50" width="200" height="14" font="0">invisible text</text>
<image top="0" left="0" width="612" height="792" src="output001.jpg"/>
<image top="0" left="0" width="612" height="792" src="output002.jpg"/>
</page>
</pdf2xml>"##;

        let (fonts, pages, _) = parse_pdftohtml_xml(xml).unwrap();

        assert_eq!(fonts[0].family, "GlyphLessFont");
        assert_eq!(pages[0].images.len(), 2);
        assert_eq!(pages[0].text_elements.len(), 1);
    }
}
