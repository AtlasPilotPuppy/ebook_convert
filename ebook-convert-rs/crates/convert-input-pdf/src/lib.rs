//! PDF input plugin — extracts text and images from PDF files.

mod classify;
mod extract;
pub mod pdftohtml;
mod render;
mod text_builder;
mod toc;

use std::path::Path;

use convert_core::book::{BookDocument, EbookFormat};
use convert_core::error::Result;
use convert_core::options::ConversionOptions;
use convert_core::plugin::InputPlugin;

pub struct PdfInputPlugin;

impl InputPlugin for PdfInputPlugin {
    fn name(&self) -> &str {
        "PDF Input"
    }

    fn supported_formats(&self) -> &[EbookFormat] {
        &[EbookFormat::Pdf]
    }

    fn convert(&self, input_path: &Path, options: &ConversionOptions) -> Result<BookDocument> {
        log::info!("Reading PDF: {}", input_path.display());
        extract::extract_pdf(input_path, options)
    }
}
