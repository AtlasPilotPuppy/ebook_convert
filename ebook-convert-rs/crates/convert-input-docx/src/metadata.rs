//! Parse Dublin Core metadata from `docProps/core.xml`.

use convert_core::book::BookDocument;
use quick_xml::events::Event;
use quick_xml::Reader;

/// Parse `docProps/core.xml` and populate book metadata.
///
/// Core properties use Dublin Core namespace:
/// - `dc:title`, `dc:creator`, `dc:description`, `dc:subject`
/// - `dc:language`, `dcterms:created`, `dcterms:modified`
/// - `cp:keywords`
pub fn parse_core_metadata(xml: &str, book: &mut BookDocument) {
    let mut reader = Reader::from_str(xml);

    let mut current_tag = String::new();
    let mut in_element = false;

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) => {
                let local = String::from_utf8_lossy(e.local_name().as_ref()).to_string();
                current_tag = local;
                in_element = true;
            }
            Ok(Event::Text(ref e)) => {
                if in_element {
                    if let Ok(text) = e.unescape() {
                        let text = text.trim().to_string();
                        if text.is_empty() {
                            continue;
                        }
                        match current_tag.as_str() {
                            "title" => book.metadata.set_title(&text),
                            "creator" => book.metadata.add("creator", &text),
                            "description" => book.metadata.set("description", &text),
                            "subject" => book.metadata.add("subject", &text),
                            "language" => book.metadata.set("language", &text),
                            "created" | "modified" => {
                                if !book.metadata.contains("date") {
                                    book.metadata.set("date", &text);
                                }
                            }
                            "keywords" => {
                                // Keywords may be comma-separated
                                for kw in text.split(',') {
                                    let kw = kw.trim();
                                    if !kw.is_empty() {
                                        book.metadata.add("subject", kw);
                                    }
                                }
                            }
                            "lastModifiedBy" => {
                                book.metadata.add("contributor", &text);
                            }
                            _ => {}
                        }
                    }
                }
            }
            Ok(Event::End(_)) => {
                in_element = false;
                current_tag.clear();
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_core_metadata() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties"
                   xmlns:dc="http://purl.org/dc/elements/1.1/"
                   xmlns:dcterms="http://purl.org/dc/terms/">
  <dc:title>My Document</dc:title>
  <dc:creator>John Doe</dc:creator>
  <dc:description>A test document</dc:description>
  <dc:language>en-US</dc:language>
  <dcterms:created>2024-01-15T10:30:00Z</dcterms:created>
  <cp:keywords>test, document, sample</cp:keywords>
</cp:coreProperties>"#;

        let mut book = BookDocument::new();
        parse_core_metadata(xml, &mut book);

        assert_eq!(book.metadata.title(), Some("My Document"));
        assert_eq!(book.metadata.authors(), vec!["John Doe"]);
        assert_eq!(book.metadata.description(), Some("A test document"));
        assert_eq!(book.metadata.language(), Some("en-US"));
        assert!(book.metadata.date().is_some());
    }

    #[test]
    fn test_parse_minimal_metadata() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties">
  <dc:title xmlns:dc="http://purl.org/dc/elements/1.1/">Minimal</dc:title>
</cp:coreProperties>"#;

        let mut book = BookDocument::new();
        parse_core_metadata(xml, &mut book);
        assert_eq!(book.metadata.title(), Some("Minimal"));
    }
}
