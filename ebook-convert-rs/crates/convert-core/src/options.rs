//! Conversion options shared across the pipeline.

use std::path::PathBuf;

use crate::book::EbookFormat;

/// All options controlling the conversion pipeline.
/// Equivalent to the merged options from Calibre's plumber.
#[derive(Debug, Clone)]
pub struct ConversionOptions {
    // -- General --
    pub verbose: u8,
    pub debug_pipeline: Option<PathBuf>,

    // -- Input --
    pub input_encoding: Option<String>,

    // -- Look & Feel --
    pub base_font_size: f64,
    pub font_size_mapping: Option<Vec<f64>>,
    pub minimum_line_height: f64,
    pub line_height: Option<f64>,
    pub embed_font_family: Option<String>,
    pub embed_all_fonts: bool,
    pub subset_embedded_fonts: bool,
    pub extra_css: Option<String>,
    pub filter_css: Option<String>,
    pub smarten_punctuation: bool,
    pub unsmarten_punctuation: bool,

    // -- Page Setup --
    pub margin_top: f64,
    pub margin_bottom: f64,
    pub margin_left: f64,
    pub margin_right: f64,

    // -- Structure --
    pub chapter_mark: ChapterMark,
    pub chapter_regex: Option<String>,
    pub page_breaks_before: Option<String>,
    pub remove_first_image: bool,
    pub insert_metadata: bool,
    pub linearize_tables: bool,

    // -- Table of Contents --
    pub no_default_epub_cover: bool,
    pub max_toc_links: usize,
    pub toc_threshold: usize,
    pub toc_filter: Option<String>,
    pub level1_toc: Option<String>,
    pub level2_toc: Option<String>,
    pub level3_toc: Option<String>,

    // -- Image --
    pub max_image_size: Option<(u32, u32)>,
    pub no_images: bool,
    /// JPEG quality (1-100). Used when transcoding images (e.g., JP2→JPEG).
    pub jpeg_quality: u8,

    // -- Output format --
    pub output_profile: OutputProfile,
    pub input_profile: InputProfile,
    pub pretty_print: bool,

    // -- Format-specific --
    pub epub_version: EpubVersion,
    pub epub_flatten: bool,
    pub pdf_page_size: Option<String>,
    pub pdf_serif_family: Option<String>,
    pub pdf_engine: PdfEngine,
    pub pdf_dpi: u16,

    // -- Formats --
    pub input_format: Option<EbookFormat>,
    pub output_format: Option<EbookFormat>,
}

impl Default for ConversionOptions {
    fn default() -> Self {
        Self {
            verbose: 0,
            debug_pipeline: None,
            input_encoding: None,
            base_font_size: 0.0,
            font_size_mapping: None,
            minimum_line_height: 120.0,
            line_height: None,
            embed_font_family: None,
            embed_all_fonts: false,
            subset_embedded_fonts: true,
            extra_css: None,
            filter_css: None,
            smarten_punctuation: false,
            unsmarten_punctuation: false,
            margin_top: 5.0,
            margin_bottom: 5.0,
            margin_left: 5.0,
            margin_right: 5.0,
            chapter_mark: ChapterMark::PageBreak,
            chapter_regex: None,
            page_breaks_before: None,
            remove_first_image: false,
            insert_metadata: false,
            linearize_tables: false,
            no_default_epub_cover: false,
            max_toc_links: 50,
            toc_threshold: 6,
            toc_filter: None,
            level1_toc: None,
            level2_toc: None,
            level3_toc: None,
            max_image_size: None,
            no_images: false,
            jpeg_quality: 80,
            output_profile: OutputProfile::default(),
            input_profile: InputProfile::default(),
            pretty_print: false,
            epub_version: EpubVersion::V2,
            epub_flatten: false,
            pdf_page_size: None,
            pdf_serif_family: None,
            pdf_engine: PdfEngine::Auto,
            pdf_dpi: 200,
            input_format: None,
            output_format: None,
        }
    }
}

/// PDF extraction engine selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PdfEngine {
    /// Use pdftohtml for text pages, pdftoppm for scanned pages.
    #[default]
    Auto,
    /// Render all pages as images via pdftoppm (legacy behavior).
    ImageOnly,
    /// Use pdftohtml only; skip pdftoppm fallback for scanned pages.
    TextOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChapterMark {
    PageBreak,
    Rule,
    Both,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EpubVersion {
    V2,
    V3,
}

/// Output device profile (screen size, DPI, etc.).
#[derive(Debug, Clone)]
pub struct OutputProfile {
    pub name: String,
    pub screen_width: u32,
    pub screen_height: u32,
    pub dpi: f64,
    pub fbase: f64,
    pub fsizes: Vec<f64>,
}

impl Default for OutputProfile {
    fn default() -> Self {
        Self {
            name: "default".to_string(),
            screen_width: 600,
            screen_height: 800,
            dpi: 166.0,
            fbase: 12.0,
            fsizes: vec![7.5, 9.0, 10.0, 12.0, 15.5, 20.0, 22.0, 24.0],
        }
    }
}

/// Input device profile.
#[derive(Debug, Clone)]
pub struct InputProfile {
    pub name: String,
    pub screen_width: u32,
    pub screen_height: u32,
    pub dpi: f64,
    pub fbase: f64,
    pub fsizes: Vec<f64>,
}

impl Default for InputProfile {
    fn default() -> Self {
        Self {
            name: "default".to_string(),
            screen_width: 600,
            screen_height: 800,
            dpi: 166.0,
            fbase: 12.0,
            fsizes: vec![7.5, 9.0, 10.0, 12.0, 15.5, 20.0, 22.0, 24.0],
        }
    }
}
