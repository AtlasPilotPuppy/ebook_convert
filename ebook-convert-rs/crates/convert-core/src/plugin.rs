//! Plugin traits for input, output, and transform plugins.

use std::path::Path;

use crate::book::{BookDocument, EbookFormat};
use crate::error::Result;
use crate::options::ConversionOptions;

/// Progress reporter callback type.
pub type ProgressReporter = Box<dyn Fn(f64, &str) + Send + Sync>;

/// Input format plugin: converts a file into a BookDocument.
pub trait InputPlugin: Send + Sync {
    /// Human-readable name of this plugin.
    fn name(&self) -> &str;

    /// File formats this plugin handles.
    fn supported_formats(&self) -> &[EbookFormat];

    /// Convert an input file to a BookDocument.
    fn convert(
        &self,
        input_path: &Path,
        options: &ConversionOptions,
    ) -> Result<BookDocument>;

    /// Called after the book has been parsed to allow format-specific postprocessing.
    fn postprocess(&self, _book: &mut BookDocument, _options: &ConversionOptions) -> Result<()> {
        Ok(())
    }

    /// Called after postprocess to specialize the book for a particular output format.
    fn specialize(
        &self,
        _book: &mut BookDocument,
        _options: &ConversionOptions,
        _output_format: EbookFormat,
    ) -> Result<()> {
        Ok(())
    }
}

/// Output format plugin: converts a BookDocument to a target file.
pub trait OutputPlugin: Send + Sync {
    /// Human-readable name of this plugin.
    fn name(&self) -> &str;

    /// The output format this plugin produces.
    fn output_format(&self) -> EbookFormat;

    /// Convert a BookDocument to the target format.
    fn convert(
        &self,
        book: &BookDocument,
        output_path: &Path,
        options: &ConversionOptions,
    ) -> Result<()>;
}

/// A transform that mutates the BookDocument IR.
/// Transforms run between input and output in a fixed order.
pub trait Transform: Send + Sync {
    /// Human-readable name of this transform.
    fn name(&self) -> &str;

    /// Apply this transform to the book document.
    fn apply(
        &self,
        book: &mut BookDocument,
        options: &ConversionOptions,
    ) -> Result<()>;

    /// Whether this transform should run given the current options.
    /// Default: always run.
    fn should_run(&self, _options: &ConversionOptions) -> bool {
        true
    }
}
