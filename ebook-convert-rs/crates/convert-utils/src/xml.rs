//! XML parsing helpers using quick-xml.

use quick_xml::events::Event;
use quick_xml::Reader;
use std::collections::HashMap;

/// Parse an XML string and extract text content of a specific element.
pub fn extract_text(xml: &str, tag_name: &str) -> Vec<String> {
    let mut reader = Reader::from_str(xml);
    let mut results = Vec::new();
    let mut in_target = false;
    let mut buf = String::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) => {
                let local_name = String::from_utf8_lossy(e.local_name().as_ref()).to_string();
                if local_name == tag_name {
                    in_target = true;
                    buf.clear();
                }
            }
            Ok(Event::Text(ref e)) => {
                if in_target {
                    if let Ok(text) = e.unescape() {
                        buf.push_str(&text);
                    }
                }
            }
            Ok(Event::End(ref e)) => {
                let local_name = String::from_utf8_lossy(e.local_name().as_ref()).to_string();
                if local_name == tag_name && in_target {
                    results.push(buf.clone());
                    in_target = false;
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
    }
    results
}

/// Extract attributes from the first occurrence of a tag.
pub fn extract_attributes(xml: &str, tag_name: &str) -> Option<HashMap<String, String>> {
    let mut reader = Reader::from_str(xml);

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                let local_name = String::from_utf8_lossy(e.local_name().as_ref()).to_string();
                if local_name == tag_name {
                    let mut attrs = HashMap::new();
                    for attr in e.attributes().flatten() {
                        let key =
                            String::from_utf8_lossy(attr.key.local_name().as_ref()).to_string();
                        let value = String::from_utf8_lossy(&attr.value).to_string();
                        attrs.insert(key, value);
                    }
                    return Some(attrs);
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
    }
    None
}

/// Simple XML builder for generating OPF, NCX, and container.xml files.
pub struct XmlBuilder {
    content: String,
    indent_level: usize,
}

impl XmlBuilder {
    pub fn new() -> Self {
        Self {
            content: String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n"),
            indent_level: 0,
        }
    }

    pub fn open_tag(&mut self, name: &str, attrs: &[(&str, &str)]) -> &mut Self {
        self.indent();
        self.content.push('<');
        self.content.push_str(name);
        for (key, value) in attrs {
            self.content.push(' ');
            self.content.push_str(key);
            self.content.push_str("=\"");
            self.content.push_str(&escape_xml_attr(value));
            self.content.push('"');
        }
        self.content.push_str(">\n");
        self.indent_level += 1;
        self
    }

    pub fn close_tag(&mut self, name: &str) -> &mut Self {
        self.indent_level = self.indent_level.saturating_sub(1);
        self.indent();
        self.content.push_str("</");
        self.content.push_str(name);
        self.content.push_str(">\n");
        self
    }

    pub fn empty_tag(&mut self, name: &str, attrs: &[(&str, &str)]) -> &mut Self {
        self.indent();
        self.content.push('<');
        self.content.push_str(name);
        for (key, value) in attrs {
            self.content.push(' ');
            self.content.push_str(key);
            self.content.push_str("=\"");
            self.content.push_str(&escape_xml_attr(value));
            self.content.push('"');
        }
        self.content.push_str("/>\n");
        self
    }

    pub fn text_element(&mut self, name: &str, text: &str, attrs: &[(&str, &str)]) -> &mut Self {
        self.indent();
        self.content.push('<');
        self.content.push_str(name);
        for (key, value) in attrs {
            self.content.push(' ');
            self.content.push_str(key);
            self.content.push_str("=\"");
            self.content.push_str(&escape_xml_attr(value));
            self.content.push('"');
        }
        self.content.push('>');
        self.content.push_str(&escape_xml_text(text));
        self.content.push_str("</");
        self.content.push_str(name);
        self.content.push_str(">\n");
        self
    }

    pub fn raw(&mut self, text: &str) -> &mut Self {
        self.content.push_str(text);
        self
    }

    pub fn build(self) -> String {
        self.content
    }

    fn indent(&mut self) {
        for _ in 0..self.indent_level {
            self.content.push_str("  ");
        }
    }
}

impl Default for XmlBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// XHTML 1.1 DOCTYPE for EPUB 2 compliance.
pub const XHTML11_DOCTYPE: &str =
    "<!DOCTYPE html PUBLIC \"-//W3C//DTD XHTML 1.1//EN\" \"http://www.w3.org/TR/xhtml11/DTD/xhtml11.dtd\">";

/// XML declaration for XHTML files.
pub const XML_DECLARATION: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>";

/// Build a valid XHTML 1.1 document for EPUB 2.
///
/// Returns the complete preamble up to and including `<body>`:
/// ```text
/// <?xml version="1.0" encoding="UTF-8"?>
/// <!DOCTYPE html PUBLIC "-//W3C//DTD XHTML 1.1//EN" "...">
/// <html xmlns="http://www.w3.org/1999/xhtml" xml:lang="en">
/// <head>
///   <title>...</title>
///   <link rel="stylesheet" type="text/css" href="style.css"/>
/// </head>
/// <body>
/// ```
pub fn xhtml11_document(title: &str, lang: &str, css_href: Option<&str>, body: &str) -> String {
    let mut s = String::with_capacity(512 + body.len());
    s.push_str(XML_DECLARATION);
    s.push('\n');
    s.push_str(XHTML11_DOCTYPE);
    s.push('\n');
    s.push_str("<html xmlns=\"http://www.w3.org/1999/xhtml\" xml:lang=\"");
    s.push_str(&escape_xml_attr(lang));
    s.push_str("\">\n<head>\n  <title>");
    s.push_str(&escape_xml_text(title));
    s.push_str("</title>\n");
    if let Some(href) = css_href {
        s.push_str("  <link rel=\"stylesheet\" type=\"text/css\" href=\"");
        s.push_str(&escape_xml_attr(href));
        s.push_str("\"/>\n");
    }
    s.push_str("</head>\n<body>\n");
    s.push_str(body);
    s.push_str("\n</body>\n</html>");
    s
}

/// Escape special characters in XML text content.
pub fn escape_xml_text(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Escape special characters in XML attribute values.
pub fn escape_xml_attr(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_text() {
        let xml = r#"<root><title>Hello World</title><title>Second</title></root>"#;
        let titles = extract_text(xml, "title");
        assert_eq!(titles, vec!["Hello World", "Second"]);
    }

    #[test]
    fn test_extract_attributes() {
        let xml = r#"<root><item id="ch1" href="chapter1.xhtml" media-type="application/xhtml+xml"/></root>"#;
        let attrs = extract_attributes(xml, "item").unwrap();
        assert_eq!(attrs.get("id").unwrap(), "ch1");
        assert_eq!(attrs.get("href").unwrap(), "chapter1.xhtml");
    }

    #[test]
    fn test_xml_builder() {
        let mut builder = XmlBuilder::new();
        builder
            .open_tag("root", &[("xmlns", "http://example.com")])
            .text_element("title", "Test", &[])
            .empty_tag("meta", &[("name", "author"), ("content", "Alice")])
            .close_tag("root");

        let xml = builder.build();
        assert!(xml.contains("<title>Test</title>"));
        assert!(xml.contains("xmlns=\"http://example.com\""));
        assert!(xml.contains("<meta name=\"author\" content=\"Alice\"/>"));
    }

    #[test]
    fn test_escape() {
        assert_eq!(escape_xml_text("a < b & c"), "a &lt; b &amp; c");
        assert_eq!(escape_xml_attr("say \"hello\""), "say &quot;hello&quot;");
    }
}
