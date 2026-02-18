//! Conversion options shared across the pipeline.

use std::path::PathBuf;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::book::EbookFormat;

/// All options controlling the conversion pipeline.
/// Equivalent to the merged options from Calibre's plumber.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
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
    #[serde(
        serialize_with = "serialize_image_size",
        deserialize_with = "deserialize_image_size"
    )]
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

    // -- Formats (CLI/extension only, not from config file) --
    #[serde(skip)]
    pub input_format: Option<EbookFormat>,
    #[serde(skip)]
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

/// Serialize `Option<(u32, u32)>` as `"WxH"` string.
fn serialize_image_size<S>(val: &Option<(u32, u32)>, s: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match val {
        Some((w, h)) => s.serialize_str(&format!("{}x{}", w, h)),
        None => s.serialize_none(),
    }
}

/// Deserialize `Option<(u32, u32)>` from `"WxH"` string.
fn deserialize_image_size<'de, D>(d: D) -> Result<Option<(u32, u32)>, D::Error>
where
    D: Deserializer<'de>,
{
    let opt: Option<String> = Option::deserialize(d)?;
    match opt {
        None => Ok(None),
        Some(s) => {
            let parts: Vec<&str> = s.split('x').collect();
            if parts.len() == 2 {
                let w = parts[0]
                    .parse::<u32>()
                    .map_err(serde::de::Error::custom)?;
                let h = parts[1]
                    .parse::<u32>()
                    .map_err(serde::de::Error::custom)?;
                Ok(Some((w, h)))
            } else {
                Err(serde::de::Error::custom(
                    "expected format 'WxH' (e.g. '1200x1600')",
                ))
            }
        }
    }
}

/// PDF extraction engine selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PdfEngine {
    /// Use pdftohtml for text pages, pdftoppm for scanned pages.
    #[default]
    Auto,
    /// Render all pages as images via pdftoppm (legacy behavior).
    ImageOnly,
    /// Use pdftohtml only; skip pdftoppm fallback for scanned pages.
    TextOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
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

impl Serialize for EpubVersion {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            EpubVersion::V2 => serializer.serialize_str("2"),
            EpubVersion::V3 => serializer.serialize_str("3"),
        }
    }
}

impl<'de> Deserialize<'de> for EpubVersion {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match s.as_str() {
            "2" => Ok(EpubVersion::V2),
            "3" => Ok(EpubVersion::V3),
            _ => Err(serde::de::Error::custom(
                "expected '2' or '3' for epub_version",
            )),
        }
    }
}

/// Output device profile (screen size, DPI, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_toml_round_trip_full() {
        let mut opts = ConversionOptions::default();
        opts.verbose = 2;
        opts.jpeg_quality = 95;
        opts.pdf_engine = PdfEngine::ImageOnly;
        opts.chapter_mark = ChapterMark::Both;
        opts.epub_version = EpubVersion::V3;
        opts.max_image_size = Some((1200, 1600));
        opts.extra_css = Some("body { font-size: 14px; }".to_string());
        opts.unsmarten_punctuation = true;
        opts.margin_top = 10.0;

        let toml_str = toml::to_string_pretty(&opts).unwrap();
        let parsed: ConversionOptions = toml::from_str(&toml_str).unwrap();

        assert_eq!(parsed.verbose, 2);
        assert_eq!(parsed.jpeg_quality, 95);
        assert_eq!(parsed.pdf_engine, PdfEngine::ImageOnly);
        assert_eq!(parsed.chapter_mark, ChapterMark::Both);
        assert_eq!(parsed.epub_version, EpubVersion::V3);
        assert_eq!(parsed.max_image_size, Some((1200, 1600)));
        assert_eq!(
            parsed.extra_css.as_deref(),
            Some("body { font-size: 14px; }")
        );
        assert!(parsed.unsmarten_punctuation);
        assert_eq!(parsed.margin_top, 10.0);
    }

    #[test]
    fn test_toml_partial_config() {
        let toml_str = r#"
verbose = 1
jpeg_quality = 90
"#;
        let opts: ConversionOptions = toml::from_str(toml_str).unwrap();
        assert_eq!(opts.verbose, 1);
        assert_eq!(opts.jpeg_quality, 90);
        // Defaults filled in
        assert_eq!(opts.pdf_engine, PdfEngine::Auto);
        assert_eq!(opts.margin_top, 5.0);
        assert_eq!(opts.epub_version, EpubVersion::V2);
    }

    #[test]
    fn test_pdf_engine_serde() {
        // Test via ConversionOptions (TOML requires table at top level)
        let toml_str = r#"pdf_engine = "image-only""#;
        let opts: ConversionOptions = toml::from_str(toml_str).unwrap();
        assert_eq!(opts.pdf_engine, PdfEngine::ImageOnly);

        let toml_str = r#"pdf_engine = "text-only""#;
        let opts: ConversionOptions = toml::from_str(toml_str).unwrap();
        assert_eq!(opts.pdf_engine, PdfEngine::TextOnly);

        let toml_str = r#"pdf_engine = "auto""#;
        let opts: ConversionOptions = toml::from_str(toml_str).unwrap();
        assert_eq!(opts.pdf_engine, PdfEngine::Auto);

        // Round-trip
        let mut opts = ConversionOptions::default();
        opts.pdf_engine = PdfEngine::ImageOnly;
        let serialized = toml::to_string_pretty(&opts).unwrap();
        let parsed: ConversionOptions = toml::from_str(&serialized).unwrap();
        assert_eq!(parsed.pdf_engine, PdfEngine::ImageOnly);
    }

    #[test]
    fn test_chapter_mark_serde() {
        let toml_str = r#"chapter_mark = "page-break""#;
        let opts: ConversionOptions = toml::from_str(toml_str).unwrap();
        assert_eq!(opts.chapter_mark, ChapterMark::PageBreak);

        let toml_str = r#"chapter_mark = "both""#;
        let opts: ConversionOptions = toml::from_str(toml_str).unwrap();
        assert_eq!(opts.chapter_mark, ChapterMark::Both);

        let toml_str = r#"chapter_mark = "none""#;
        let opts: ConversionOptions = toml::from_str(toml_str).unwrap();
        assert_eq!(opts.chapter_mark, ChapterMark::None);

        // Round-trip
        let mut opts = ConversionOptions::default();
        opts.chapter_mark = ChapterMark::Rule;
        let serialized = toml::to_string_pretty(&opts).unwrap();
        let parsed: ConversionOptions = toml::from_str(&serialized).unwrap();
        assert_eq!(parsed.chapter_mark, ChapterMark::Rule);
    }

    #[test]
    fn test_epub_version_serde() {
        let toml_str = r#"epub_version = "2""#;
        let opts: ConversionOptions = toml::from_str(toml_str).unwrap();
        assert_eq!(opts.epub_version, EpubVersion::V2);

        let toml_str = r#"epub_version = "3""#;
        let opts: ConversionOptions = toml::from_str(toml_str).unwrap();
        assert_eq!(opts.epub_version, EpubVersion::V3);

        // Round-trip
        let mut opts = ConversionOptions::default();
        opts.epub_version = EpubVersion::V3;
        let serialized = toml::to_string_pretty(&opts).unwrap();
        let parsed: ConversionOptions = toml::from_str(&serialized).unwrap();
        assert_eq!(parsed.epub_version, EpubVersion::V3);
    }

    #[test]
    fn test_image_size_serde() {
        let toml_str = r#"max_image_size = "1200x1600""#;
        let opts: ConversionOptions = toml::from_str(toml_str).unwrap();
        assert_eq!(opts.max_image_size, Some((1200, 1600)));

        // None case
        let toml_str = "";
        let opts: ConversionOptions = toml::from_str(toml_str).unwrap();
        assert_eq!(opts.max_image_size, None);
    }

    #[test]
    fn test_example_config() {
        let config = r##"
verbose = 1
jpeg_quality = 90
pdf_engine = "auto"
pdf_dpi = 300
extra_css = "body { font-size: 14px; }"
unsmarten_punctuation = true
max_image_size = "1200x1600"
margin_top = 10.0
epub_version = "2"
"##;
        let opts: ConversionOptions = toml::from_str(config).unwrap();
        assert_eq!(opts.verbose, 1);
        assert_eq!(opts.jpeg_quality, 90);
        assert_eq!(opts.pdf_engine, PdfEngine::Auto);
        assert_eq!(opts.pdf_dpi, 300);
        assert_eq!(
            opts.extra_css.as_deref(),
            Some("body { font-size: 14px; }")
        );
        assert!(opts.unsmarten_punctuation);
        assert_eq!(opts.max_image_size, Some((1200, 1600)));
        assert_eq!(opts.margin_top, 10.0);
        assert_eq!(opts.epub_version, EpubVersion::V2);
    }
}
