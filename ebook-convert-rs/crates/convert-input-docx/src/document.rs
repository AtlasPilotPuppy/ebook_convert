//! Convert `word/document.xml` content to HTML.
//!
//! Handles Word Open XML elements:
//! - `w:p` (paragraphs) → `<p>`, `<h1>`-`<h6>`, `<li>`
//! - `w:r` (runs) → `<span>`, `<b>`, `<i>`, `<u>`, `<sub>`, `<sup>`
//! - `w:t` (text)
//! - `w:tbl` (tables) → `<table>`
//! - `w:drawing` / `w:pict` (images) → `<img>`
//! - `w:hyperlink` → `<a>`
//! - `w:br` (breaks) → `<br/>`
//! - `w:tab` → tab space

use std::collections::HashMap;

use quick_xml::events::Event;
use quick_xml::Reader;

use crate::styles::{self, NumberingInfo, StyleInfo};

/// Parse `word/_rels/document.xml.rels` into a relationship map (rId → target).
pub fn parse_relationships(xml: &str) -> HashMap<String, String> {
    let mut rels = HashMap::new();
    let mut reader = Reader::from_str(xml);

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                let local = String::from_utf8_lossy(e.local_name().as_ref()).to_string();
                if local == "Relationship" {
                    let mut id = String::new();
                    let mut target = String::new();
                    for attr in e.attributes().flatten() {
                        let key =
                            String::from_utf8_lossy(attr.key.local_name().as_ref()).to_string();
                        match key.as_str() {
                            "Id" => id = String::from_utf8_lossy(&attr.value).to_string(),
                            "Target" => target = String::from_utf8_lossy(&attr.value).to_string(),
                            _ => {}
                        }
                    }
                    if !id.is_empty() && !target.is_empty() {
                        rels.insert(id, target);
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
    }

    rels
}

/// Convert the main document XML into HTML body content.
pub fn convert_document(
    xml: &str,
    rels: &HashMap<String, String>,
    styles: &HashMap<String, StyleInfo>,
    numbering: &HashMap<String, NumberingInfo>,
) -> String {
    let mut html = String::new();
    let mut reader = Reader::from_str(xml);

    let mut state = ConvertState::new(rels, styles, numbering);

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) => {
                let local = String::from_utf8_lossy(e.local_name().as_ref()).to_string();
                state.handle_start(&local, e, &mut html);
            }
            Ok(Event::Empty(ref e)) => {
                let local = String::from_utf8_lossy(e.local_name().as_ref()).to_string();
                state.handle_empty(&local, e, &mut html);
            }
            Ok(Event::Text(ref e)) => {
                if state.in_text {
                    if let Ok(text) = e.unescape() {
                        state.para_buffer.push_str(&escape_html(&text));
                        state.para_has_content = true;
                    }
                }
            }
            Ok(Event::End(ref e)) => {
                let local = String::from_utf8_lossy(e.local_name().as_ref()).to_string();
                state.handle_end(&local, &mut html);
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
    }

    // Close any remaining open list
    state.close_list(&mut html);

    html
}

struct ConvertState<'a> {
    rels: &'a HashMap<String, String>,
    styles: &'a HashMap<String, StyleInfo>,
    numbering: &'a HashMap<String, NumberingInfo>,

    // Current paragraph state
    in_paragraph: bool,
    in_run: bool,
    in_text: bool,
    in_table: bool,
    in_row: bool,
    in_cell: bool,

    // Run formatting
    is_bold: bool,
    is_italic: bool,
    is_underline: bool,
    is_strike: bool,
    is_superscript: bool,
    is_subscript: bool,

    // Current paragraph style
    para_style_id: String,
    para_alignment: String,
    para_num_id: String,

    // Hyperlink state
    in_hyperlink: bool,
    hyperlink_href: String,

    // List state tracking
    current_list_type: Option<String>, // "ul" or "ol"

    // Buffering paragraph content to wrap in correct tag
    para_buffer: String,
    para_has_content: bool,
}

impl<'a> ConvertState<'a> {
    fn new(
        rels: &'a HashMap<String, String>,
        styles: &'a HashMap<String, StyleInfo>,
        numbering: &'a HashMap<String, NumberingInfo>,
    ) -> Self {
        Self {
            rels,
            styles,
            numbering,
            in_paragraph: false,
            in_run: false,
            in_text: false,
            in_table: false,
            in_row: false,
            in_cell: false,
            is_bold: false,
            is_italic: false,
            is_underline: false,
            is_strike: false,
            is_superscript: false,
            is_subscript: false,
            para_style_id: String::new(),
            para_alignment: String::new(),
            para_num_id: String::new(),
            in_hyperlink: false,
            hyperlink_href: String::new(),
            current_list_type: None,
            para_buffer: String::new(),
            para_has_content: false,
        }
    }

    fn handle_start(&mut self, local: &str, e: &quick_xml::events::BytesStart, html: &mut String) {
        match local {
            "p" => {
                self.in_paragraph = true;
                self.para_style_id.clear();
                self.para_alignment.clear();
                self.para_num_id.clear();
                self.para_buffer.clear();
                self.para_has_content = false;
            }
            "pPr" => {}
            "pStyle" => {
                for attr in e.attributes().flatten() {
                    let key = String::from_utf8_lossy(attr.key.local_name().as_ref()).to_string();
                    if key == "val" {
                        self.para_style_id = String::from_utf8_lossy(&attr.value).to_string();
                    }
                }
            }
            "jc" if self.in_paragraph => {
                for attr in e.attributes().flatten() {
                    let key = String::from_utf8_lossy(attr.key.local_name().as_ref()).to_string();
                    if key == "val" {
                        self.para_alignment = String::from_utf8_lossy(&attr.value).to_string();
                    }
                }
            }
            "numPr" => {}
            "numId" => {
                for attr in e.attributes().flatten() {
                    let key = String::from_utf8_lossy(attr.key.local_name().as_ref()).to_string();
                    if key == "val" {
                        self.para_num_id = String::from_utf8_lossy(&attr.value).to_string();
                    }
                }
            }
            "r" => {
                self.in_run = true;
                self.is_bold = false;
                self.is_italic = false;
                self.is_underline = false;
                self.is_strike = false;
                self.is_superscript = false;
                self.is_subscript = false;
            }
            "rPr" => {}
            "t" => {
                self.in_text = true;
                // Open formatting tags
                self.open_run_formatting();
            }
            "tbl" => {
                self.close_list(html);
                self.in_table = true;
                html.push_str("<table>\n");
            }
            "tr" => {
                self.in_row = true;
                html.push_str("<tr>");
            }
            "tc" => {
                self.in_cell = true;
                html.push_str("<td>");
            }
            "hyperlink" => {
                self.in_hyperlink = true;
                self.hyperlink_href.clear();
                for attr in e.attributes().flatten() {
                    let key = String::from_utf8_lossy(attr.key.local_name().as_ref()).to_string();
                    if key == "id" {
                        let rid = String::from_utf8_lossy(&attr.value).to_string();
                        if let Some(target) = self.rels.get(&rid) {
                            self.hyperlink_href = target.clone();
                        }
                    }
                }
                if !self.hyperlink_href.is_empty() {
                    self.para_buffer.push_str(&format!(
                        "<a href=\"{}\">",
                        escape_attr(&self.hyperlink_href)
                    ));
                }
            }
            "drawing" | "pict" => {
                // Image — look for relationship ID in child elements
                // We handle this in empty elements (blip)
            }
            _ => {}
        }
    }

    fn handle_empty(&mut self, local: &str, e: &quick_xml::events::BytesStart, html: &mut String) {
        match local {
            "b" | "bCs" if self.in_run => self.is_bold = true,
            "i" | "iCs" if self.in_run => self.is_italic = true,
            "u" if self.in_run => self.is_underline = true,
            "strike" if self.in_run => self.is_strike = true,
            "vertAlign" if self.in_run => {
                for attr in e.attributes().flatten() {
                    let key = String::from_utf8_lossy(attr.key.local_name().as_ref()).to_string();
                    if key == "val" {
                        let val = String::from_utf8_lossy(&attr.value).to_string();
                        match val.as_str() {
                            "superscript" => self.is_superscript = true,
                            "subscript" => self.is_subscript = true,
                            _ => {}
                        }
                    }
                }
            }
            "br" if self.in_run => {
                self.para_buffer.push_str("<br/>");
                self.para_has_content = true;
            }
            "tab" if self.in_run => {
                self.para_buffer.push_str("&#160;&#160;&#160;&#160;");
                self.para_has_content = true;
            }
            "pStyle" => {
                for attr in e.attributes().flatten() {
                    let key = String::from_utf8_lossy(attr.key.local_name().as_ref()).to_string();
                    if key == "val" {
                        self.para_style_id = String::from_utf8_lossy(&attr.value).to_string();
                    }
                }
            }
            "jc" if self.in_paragraph => {
                for attr in e.attributes().flatten() {
                    let key = String::from_utf8_lossy(attr.key.local_name().as_ref()).to_string();
                    if key == "val" {
                        self.para_alignment = String::from_utf8_lossy(&attr.value).to_string();
                    }
                }
            }
            "numId" => {
                for attr in e.attributes().flatten() {
                    let key = String::from_utf8_lossy(attr.key.local_name().as_ref()).to_string();
                    if key == "val" {
                        self.para_num_id = String::from_utf8_lossy(&attr.value).to_string();
                    }
                }
            }
            "blip" => {
                // Image embed reference
                for attr in e.attributes().flatten() {
                    let key = String::from_utf8_lossy(attr.key.local_name().as_ref()).to_string();
                    if key == "embed" {
                        let rid = String::from_utf8_lossy(&attr.value).to_string();
                        if let Some(target) = self.rels.get(&rid) {
                            // target is relative to word/ dir (e.g., "media/image1.png")
                            self.para_buffer.push_str(&format!(
                                "<img src=\"{}\" alt=\"\"/>",
                                escape_attr(target)
                            ));
                            self.para_has_content = true;
                        }
                    }
                }
            }
            _ => {
                // Handle b, i, etc. that appear as start elements with val attribute
                match local {
                    "b" | "bCs" if self.in_run => self.is_bold = true,
                    "i" | "iCs" if self.in_run => self.is_italic = true,
                    _ => {}
                }
            }
        }

        // Also handle table-related empty elements if needed
        if local == "tbl" {
            self.close_list(html);
            self.in_table = true;
            html.push_str("<table>\n");
        }
    }

    fn handle_end(&mut self, local: &str, html: &mut String) {
        match local {
            "t" => {
                self.close_run_formatting();
                self.in_text = false;
            }
            "r" => {
                self.in_run = false;
            }
            "p" => {
                self.flush_paragraph(html);
                self.in_paragraph = false;
            }
            "tbl" => {
                html.push_str("</table>\n");
                self.in_table = false;
            }
            "tr" => {
                html.push_str("</tr>\n");
                self.in_row = false;
            }
            "tc" => {
                html.push_str("</td>");
                self.in_cell = false;
            }
            "hyperlink" => {
                if self.in_hyperlink && !self.hyperlink_href.is_empty() {
                    self.para_buffer.push_str("</a>");
                }
                self.in_hyperlink = false;
                self.hyperlink_href.clear();
            }
            _ => {}
        }
    }

    fn open_run_formatting(&mut self) {
        if self.is_superscript {
            self.para_buffer.push_str("<sup>");
        }
        if self.is_subscript {
            self.para_buffer.push_str("<sub>");
        }
        if self.is_bold {
            self.para_buffer.push_str("<b>");
        }
        if self.is_italic {
            self.para_buffer.push_str("<i>");
        }
        if self.is_underline {
            self.para_buffer.push_str("<u>");
        }
        if self.is_strike {
            self.para_buffer.push_str("<s>");
        }
    }

    fn close_run_formatting(&mut self) {
        if self.is_strike {
            self.para_buffer.push_str("</s>");
        }
        if self.is_underline {
            self.para_buffer.push_str("</u>");
        }
        if self.is_italic {
            self.para_buffer.push_str("</i>");
        }
        if self.is_bold {
            self.para_buffer.push_str("</b>");
        }
        if self.is_subscript {
            self.para_buffer.push_str("</sub>");
        }
        if self.is_superscript {
            self.para_buffer.push_str("</sup>");
        }
    }

    fn flush_paragraph(&mut self, html: &mut String) {
        // Determine tag type: heading, list item, or regular paragraph
        let heading_level = if !self.para_style_id.is_empty() {
            styles::heading_level(&self.para_style_id, self.styles)
        } else {
            None
        };

        let is_list = !self.para_num_id.is_empty() && self.para_num_id != "0";

        // Alignment class
        let class = match self.para_alignment.as_str() {
            "center" => " class=\"docx-center\"",
            "right" | "end" => " class=\"docx-right\"",
            "both" | "distribute" => " class=\"docx-justify\"",
            _ => "",
        };

        if let Some(level) = heading_level {
            // Close any open list before a heading
            self.close_list(html);
            let tag = format!("h{}", level);
            html.push_str(&format!("<{}{}>", tag, class));
            html.push_str(&self.para_buffer);
            html.push_str(&format!("</{}>\n", tag));
        } else if is_list {
            // Determine list type
            let list_type = if let Some(info) = self.numbering.get(&self.para_num_id) {
                if info.format == "bullet" {
                    "ul"
                } else {
                    "ol"
                }
            } else {
                "ul" // default to unordered
            };

            // Open list if needed or switch type
            match &self.current_list_type {
                Some(current) if current == list_type => {} // already in correct list
                Some(_) => {
                    // Wrong list type, close and reopen
                    self.close_list(html);
                    html.push_str(&format!("<{}>\n", list_type));
                    self.current_list_type = Some(list_type.to_string());
                }
                None => {
                    html.push_str(&format!("<{}>\n", list_type));
                    self.current_list_type = Some(list_type.to_string());
                }
            }

            html.push_str("<li>");
            html.push_str(&self.para_buffer);
            html.push_str("</li>\n");
        } else {
            // Close any open list
            self.close_list(html);

            // Skip truly empty paragraphs (no content, no formatting)
            if !self.para_has_content && self.para_buffer.trim().is_empty() {
                return;
            }

            html.push_str(&format!("<p{}>", class));
            html.push_str(&self.para_buffer);
            html.push_str("</p>\n");
        }
    }

    fn close_list(&mut self, html: &mut String) {
        if let Some(ref list_type) = self.current_list_type.take() {
            html.push_str(&format!("</{}>\n", list_type));
        }
    }
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn escape_attr(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_relationships() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/image1.png"/>
  <Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink" Target="https://example.com" TargetMode="External"/>
</Relationships>"#;

        let rels = parse_relationships(xml);
        assert_eq!(rels.get("rId1").unwrap(), "media/image1.png");
        assert_eq!(rels.get("rId2").unwrap(), "https://example.com");
    }

    #[test]
    fn test_convert_simple_paragraph() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:body>
  <w:p>
    <w:r>
      <w:t>Hello World</w:t>
    </w:r>
  </w:p>
</w:body>
</w:document>"#;

        let html = convert_document(xml, &HashMap::new(), &HashMap::new(), &HashMap::new());
        assert!(html.contains("<p>Hello World</p>"));
    }

    #[test]
    fn test_convert_bold_italic() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:body>
  <w:p>
    <w:r>
      <w:rPr><w:b/><w:i/></w:rPr>
      <w:t>Bold Italic</w:t>
    </w:r>
  </w:p>
</w:body>
</w:document>"#;

        let html = convert_document(xml, &HashMap::new(), &HashMap::new(), &HashMap::new());
        assert!(html.contains("<b><i>Bold Italic</i></b>"));
    }

    #[test]
    fn test_convert_heading() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:body>
  <w:p>
    <w:pPr><w:pStyle w:val="Heading1"/></w:pPr>
    <w:r><w:t>Chapter Title</w:t></w:r>
  </w:p>
</w:body>
</w:document>"#;

        let mut styles = HashMap::new();
        styles.insert(
            "Heading1".to_string(),
            StyleInfo {
                name: "heading 1".to_string(),
                based_on: None,
                outline_level: Some(0),
            },
        );

        let html = convert_document(xml, &HashMap::new(), &styles, &HashMap::new());
        assert!(html.contains("<h1>Chapter Title</h1>"));
    }

    #[test]
    fn test_convert_table() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:body>
  <w:tbl>
    <w:tr>
      <w:tc><w:p><w:r><w:t>A</w:t></w:r></w:p></w:tc>
      <w:tc><w:p><w:r><w:t>B</w:t></w:r></w:p></w:tc>
    </w:tr>
  </w:tbl>
</w:body>
</w:document>"#;

        let html = convert_document(xml, &HashMap::new(), &HashMap::new(), &HashMap::new());
        assert!(html.contains("<table>"));
        assert!(html.contains("<td>"));
        assert!(html.contains("A"));
        assert!(html.contains("B"));
        assert!(html.contains("</table>"));
    }

    #[test]
    fn test_convert_image_reference() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
            xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
            xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
<w:body>
  <w:p>
    <w:r>
      <w:drawing>
        <a:blip r:embed="rId1"/>
      </w:drawing>
    </w:r>
  </w:p>
</w:body>
</w:document>"#;

        let mut rels = HashMap::new();
        rels.insert("rId1".to_string(), "media/image1.png".to_string());

        let html = convert_document(xml, &rels, &HashMap::new(), &HashMap::new());
        assert!(html.contains(r#"<img src="media/image1.png""#));
    }

    #[test]
    fn test_convert_alignment() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:body>
  <w:p>
    <w:pPr><w:jc w:val="center"/></w:pPr>
    <w:r><w:t>Centered text</w:t></w:r>
  </w:p>
</w:body>
</w:document>"#;

        let html = convert_document(xml, &HashMap::new(), &HashMap::new(), &HashMap::new());
        assert!(html.contains("docx-center"));
        assert!(html.contains("Centered text"));
    }
}
