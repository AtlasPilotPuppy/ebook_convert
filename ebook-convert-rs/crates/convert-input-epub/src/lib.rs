//! EPUB input plugin — reads EPUB 2/3 files into BookDocument.

mod parser;

use std::path::Path;

use convert_core::book::{BookDocument, EbookFormat};
use convert_core::error::Result;
use convert_core::options::ConversionOptions;
use convert_core::plugin::InputPlugin;

pub struct EpubInputPlugin;

impl InputPlugin for EpubInputPlugin {
    fn name(&self) -> &str {
        "EPUB Input"
    }

    fn supported_formats(&self) -> &[EbookFormat] {
        &[EbookFormat::Epub]
    }

    fn convert(&self, input_path: &Path, _options: &ConversionOptions) -> Result<BookDocument> {
        log::info!("Reading EPUB: {}", input_path.display());
        parser::parse_epub(input_path)
    }
}
