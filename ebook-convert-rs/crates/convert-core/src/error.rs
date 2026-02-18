use thiserror::Error;

#[derive(Error, Debug)]
pub enum ConvertError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("XML parsing error: {0}")]
    Xml(String),

    #[error("HTML parsing error: {0}")]
    Html(String),

    #[error("CSS parsing error: {0}")]
    Css(String),

    #[error("PDF error: {0}")]
    Pdf(String),

    #[error("EPUB error: {0}")]
    Epub(String),

    #[error("MOBI error: {0}")]
    Mobi(String),

    #[error("DOCX error: {0}")]
    Docx(String),

    #[error("Invalid manifest: {0}")]
    Manifest(String),

    #[error("Invalid metadata: {0}")]
    Metadata(String),

    #[error("Unsupported format: {0}")]
    UnsupportedFormat(String),

    #[error("Image processing error: {0}")]
    Image(String),

    #[error("Encoding error: {0}")]
    Encoding(String),

    #[error("Pipeline error: {0}")]
    Pipeline(String),

    #[error("Plugin error: {0}")]
    Plugin(String),

    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, ConvertError>;
