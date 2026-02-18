//! EPUB output plugin — serializes BookDocument to EPUB 2/3.

mod writer;

use std::path::Path;

use convert_core::book::{BookDocument, EbookFormat};
use convert_core::error::Result;
use convert_core::options::ConversionOptions;
use convert_core::plugin::OutputPlugin;

pub struct EpubOutputPlugin;

impl OutputPlugin for EpubOutputPlugin {
    fn name(&self) -> &str {
        "EPUB Output"
    }

    fn output_format(&self) -> EbookFormat {
        EbookFormat::Epub
    }

    fn convert(
        &self,
        book: &BookDocument,
        output_path: &Path,
        options: &ConversionOptions,
    ) -> Result<()> {
        log::info!("Writing EPUB: {}", output_path.display());
        writer::write_epub(book, output_path, options)
    }
}
