//! MIME type detection and mapping utilities.

use std::path::Path;

/// Detect MIME type from a file extension.
pub fn mime_from_extension(ext: &str) -> &'static str {
    match ext.to_lowercase().as_str() {
        // XHTML/HTML
        "xhtml" | "xhtm" => "application/xhtml+xml",
        "html" | "htm" => "text/html",

        // CSS
        "css" => "text/css",

        // Images
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "webp" => "image/webp",
        "avif" => "image/avif",

        // Fonts
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "woff" => "font/woff",
        "woff2" => "font/woff2",

        // XML
        "xml" => "application/xml",
        "opf" => "application/oebps-package+xml",
        "ncx" => "application/x-dtbncx+xml",

        // Other
        "js" => "application/javascript",
        "json" => "application/json",
        "txt" => "text/plain",
        "pdf" => "application/pdf",

        _ => "application/octet-stream",
    }
}

/// Detect MIME type from a file path.
pub fn mime_from_path(path: &Path) -> &'static str {
    path.extension()
        .and_then(|e| e.to_str())
        .map(mime_from_extension)
        .unwrap_or("application/octet-stream")
}

/// Check if a MIME type represents a text-based format.
pub fn is_text_mime(mime: &str) -> bool {
    mime.starts_with("text/")
        || mime == "application/xhtml+xml"
        || mime == "application/xml"
        || mime == "application/javascript"
        || mime == "application/json"
        || mime == "application/oebps-package+xml"
        || mime == "application/x-dtbncx+xml"
}

/// Get the standard file extension for a MIME type.
pub fn extension_from_mime(mime: &str) -> &'static str {
    match mime {
        "application/xhtml+xml" => "xhtml",
        "text/html" => "html",
        "text/css" => "css",
        "image/jpeg" => "jpg",
        "image/png" => "png",
        "image/gif" => "gif",
        "image/svg+xml" => "svg",
        "image/webp" => "webp",
        "font/ttf" | "application/x-font-ttf" => "ttf",
        "font/otf" | "application/x-font-opentype" => "otf",
        "font/woff" | "application/font-woff" => "woff",
        "font/woff2" | "application/font-woff2" => "woff2",
        "application/xml" => "xml",
        "text/plain" => "txt",
        "application/pdf" => "pdf",
        _ => "bin",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_mime_from_extension() {
        assert_eq!(mime_from_extension("xhtml"), "application/xhtml+xml");
        assert_eq!(mime_from_extension("jpg"), "image/jpeg");
        assert_eq!(mime_from_extension("CSS"), "text/css");
        assert_eq!(mime_from_extension("unknown"), "application/octet-stream");
    }

    #[test]
    fn test_mime_from_path() {
        assert_eq!(
            mime_from_path(Path::new("chapter1.xhtml")),
            "application/xhtml+xml"
        );
        assert_eq!(mime_from_path(Path::new("style.css")), "text/css");
    }

    #[test]
    fn test_is_text_mime() {
        assert!(is_text_mime("text/html"));
        assert!(is_text_mime("application/xhtml+xml"));
        assert!(!is_text_mime("image/jpeg"));
    }
}
