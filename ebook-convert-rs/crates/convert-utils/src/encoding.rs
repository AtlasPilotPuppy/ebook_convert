//! Character encoding detection and conversion.

use encoding_rs::Encoding;

/// Detect encoding from a byte string and decode to UTF-8.
/// Tries BOM detection first, then falls back to encoding_rs sniffing.
pub fn decode_to_utf8(bytes: &[u8]) -> (String, &'static str) {
    // Check BOM
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return (String::from_utf8_lossy(&bytes[3..]).to_string(), "UTF-8");
    }
    if bytes.starts_with(&[0xFF, 0xFE]) {
        let (result, _, _) = encoding_rs::UTF_16LE.decode(bytes);
        return (result.to_string(), "UTF-16LE");
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        let (result, _, _) = encoding_rs::UTF_16BE.decode(bytes);
        return (result.to_string(), "UTF-16BE");
    }

    // Try UTF-8 first
    match std::str::from_utf8(bytes) {
        Ok(s) => (s.to_string(), "UTF-8"),
        Err(_) => {
            // Fall back to Windows-1252 (common for older documents)
            let (result, _, _) = encoding_rs::WINDOWS_1252.decode(bytes);
            (result.to_string(), "Windows-1252")
        }
    }
}

/// Decode bytes using a specific encoding name.
pub fn decode_with_encoding(bytes: &[u8], encoding_name: &str) -> Option<String> {
    let encoding = Encoding::for_label(encoding_name.as_bytes())?;
    let (result, _, _) = encoding.decode(bytes);
    Some(result.to_string())
}

/// Try to detect the encoding from an XML declaration.
/// Looks for `<?xml ... encoding="..." ?>`.
pub fn detect_xml_encoding(bytes: &[u8]) -> Option<String> {
    // Read enough bytes to find the XML declaration
    let head = &bytes[..bytes.len().min(512)];
    let head_str = String::from_utf8_lossy(head);

    if let Some(start) = head_str.find("encoding=") {
        let rest = &head_str[start + 9..];
        let quote = rest.chars().next()?;
        if quote == '"' || quote == '\'' {
            let rest = &rest[1..];
            if let Some(end) = rest.find(quote) {
                return Some(rest[..end].to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_utf8() {
        let (text, enc) = decode_to_utf8(b"Hello, World!");
        assert_eq!(text, "Hello, World!");
        assert_eq!(enc, "UTF-8");
    }

    #[test]
    fn test_decode_utf8_bom() {
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice(b"Hello");
        let (text, enc) = decode_to_utf8(&bytes);
        assert_eq!(text, "Hello");
        assert_eq!(enc, "UTF-8");
    }

    #[test]
    fn test_detect_xml_encoding() {
        let xml = b"<?xml version=\"1.0\" encoding=\"iso-8859-1\"?><root/>";
        assert_eq!(detect_xml_encoding(xml), Some("iso-8859-1".to_string()));
    }

    #[test]
    fn test_decode_with_encoding() {
        let result = decode_with_encoding(b"Hello", "utf-8");
        assert_eq!(result, Some("Hello".to_string()));

        let result = decode_with_encoding(b"Hello", "nonexistent");
        assert_eq!(result, None);
    }
}
