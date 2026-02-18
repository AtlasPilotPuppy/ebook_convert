//! FB2 (FictionBook) input plugin — reads FictionBook XML files into BookDocument.
//!
//! FB2 is an XML-based ebook format popular in Russia. It stores text content,
//! metadata, and base64-encoded images in a single XML file.

use std::path::Path;

use convert_core::book::{
    BookDocument, EbookFormat, ManifestData, ManifestItem, TocEntry,
};
use convert_core::error::{ConvertError, Result};
use convert_core::options::ConversionOptions;
use convert_core::plugin::InputPlugin;
use quick_xml::events::Event;
use quick_xml::Reader;

pub struct Fb2InputPlugin;

impl InputPlugin for Fb2InputPlugin {
    fn name(&self) -> &str {
        "FB2 Input"
    }

    fn supported_formats(&self) -> &[EbookFormat] {
        &[EbookFormat::Fb2]
    }

    fn convert(&self, input_path: &Path, _options: &ConversionOptions) -> Result<BookDocument> {
        log::info!("Reading FB2: {}", input_path.display());
        parse_fb2(input_path)
    }
}

fn parse_fb2(path: &Path) -> Result<BookDocument> {
    let data = std::fs::read_to_string(path)
        .map_err(|e| ConvertError::Fb2(format!("Cannot read {}: {}", path.display(), e)))?;

    let mut book = BookDocument::new();
    book.base_path = path.parent().map(|p| p.to_path_buf());

    let mut reader = Reader::from_str(&data);
    reader.config_mut().trim_text(true);

    let mut state = ParseState::default();
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                state.path.push(name.clone());

                match name.as_str() {
                    "section" => {
                        state.section_depth += 1;
                        state.in_section = true;
                    }
                    "binary" => {
                        // Extract id and content-type attributes
                        for attr in e.attributes().flatten() {
                            let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                            let val = String::from_utf8_lossy(&attr.value).to_string();
                            match key.as_str() {
                                "id" => state.binary_id = Some(val),
                                "content-type" => state.binary_mime = Some(val),
                                _ => {}
                            }
                        }
                        state.in_binary = true;
                        state.text_buf.clear();
                    }
                    "image" => {
                        // Extract l:href or href attribute
                        for attr in e.attributes().flatten() {
                            let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                            if key == "l:href" || key == "href" {
                                let href = String::from_utf8_lossy(&attr.value).to_string();
                                let id = href.trim_start_matches('#');
                                state.html.push_str(&format!(
                                    r#"<img src="images/{}" alt=""/>"#,
                                    id
                                ));
                            }
                        }
                    }
                    "a" => {
                        for attr in e.attributes().flatten() {
                            let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                            if key == "l:href" || key == "href" {
                                let href = String::from_utf8_lossy(&attr.value).to_string();
                                state.html.push_str(&format!(r#"<a href="{}">"#, convert_utils::xml::escape_xml_attr(&href)));
                                state.in_link = true;
                            }
                        }
                    }
                    "title" if state.in_section => {
                        state.in_title = true;
                        state.title_buf.clear();
                    }
                    "p" if state.in_binary => {}
                    "p" if state.in_title => {
                        // Title paragraphs
                    }
                    "p" => {
                        state.html.push_str("<p>");
                        state.in_para = true;
                    }
                    "empty-line" => {
                        state.html.push_str("<br/>");
                    }
                    "strong" => state.html.push_str("<strong>"),
                    "emphasis" => state.html.push_str("<em>"),
                    "strikethrough" => state.html.push_str("<del>"),
                    "code" => state.html.push_str("<code>"),
                    "sub" => state.html.push_str("<sub>"),
                    "sup" => state.html.push_str("<sup>"),
                    "subtitle" => {
                        state.html.push_str("<h3>");
                        state.in_para = true;
                    }
                    "poem" => state.html.push_str(r#"<div class="poem">"#),
                    "stanza" => state.html.push_str(r#"<div class="stanza">"#),
                    "v" => {
                        state.html.push_str(r#"<p class="verse">"#);
                        state.in_para = true;
                    }
                    "cite" => state.html.push_str("<blockquote>"),
                    "epigraph" => state.html.push_str(r#"<div class="epigraph">"#),
                    "text-author" => {
                        state.html.push_str(r#"<p class="text-author">"#);
                        state.in_para = true;
                    }
                    _ => {}
                }
            }
            Ok(Event::End(ref e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();

                match name.as_str() {
                    "section" => {
                        state.section_depth -= 1;
                        if state.section_depth == 0 {
                            state.in_section = false;
                        }
                    }
                    "binary" => {
                        // Decode base64 image
                        if let (Some(id), Some(mime)) = (state.binary_id.take(), state.binary_mime.take()) {
                            let b64 = state.text_buf.trim().replace(['\n', '\r', ' '], "");
                            if let Ok(data) = base64::Engine::decode(
                                &base64::engine::general_purpose::STANDARD,
                                &b64,
                            ) {
                                    let href = format!("images/{}", id);
                                let item = ManifestItem::new(
                                    &id,
                                    &href,
                                    &mime,
                                    ManifestData::Binary(data),
                                );
                                book.manifest.add(item);
                            }
                        }
                        state.in_binary = false;
                        state.text_buf.clear();
                    }
                    "title" if state.in_title => {
                        state.in_title = false;
                        let title_text = state.title_buf.trim().to_string();
                        if !title_text.is_empty() {
                            let level = state.section_depth.min(6).max(1);
                            state.html.push_str(&format!("<h{}>{}</h{}>", level, convert_utils::xml::escape_xml_text(&title_text), level));
                            state.section_titles.push(title_text);
                        }
                    }
                    "p" if state.in_title => {
                        // Space between title paragraphs
                        if !state.title_buf.is_empty() {
                            state.title_buf.push(' ');
                        }
                    }
                    "p" if state.in_para => {
                        state.html.push_str("</p>\n");
                        state.in_para = false;
                    }
                    "a" if state.in_link => {
                        state.html.push_str("</a>");
                        state.in_link = false;
                    }
                    "strong" => state.html.push_str("</strong>"),
                    "emphasis" => state.html.push_str("</em>"),
                    "strikethrough" => state.html.push_str("</del>"),
                    "code" => state.html.push_str("</code>"),
                    "sub" => state.html.push_str("</sub>"),
                    "sup" => state.html.push_str("</sup>"),
                    "subtitle" => {
                        state.html.push_str("</h3>\n");
                        state.in_para = false;
                    }
                    "poem" => state.html.push_str("</div>\n"),
                    "stanza" => state.html.push_str("</div>\n"),
                    "v" => {
                        state.html.push_str("</p>\n");
                        state.in_para = false;
                    }
                    "cite" => state.html.push_str("</blockquote>\n"),
                    "epigraph" => state.html.push_str("</div>\n"),
                    "text-author" => {
                        state.html.push_str("</p>\n");
                        state.in_para = false;
                    }
                    // Metadata extraction
                    "book-title" => {
                        if is_in_path(&state.path, "title-info") {
                            book.metadata.set_title(state.text_buf.trim());
                        }
                        state.text_buf.clear();
                    }
                    "first-name" | "middle-name" | "last-name" | "nickname" => {
                        if is_in_path(&state.path, "title-info/author") {
                            let part = state.text_buf.trim().to_string();
                            if !part.is_empty() {
                                state.author_parts.push(part);
                            }
                        }
                        state.text_buf.clear();
                    }
                    "author" => {
                        if is_in_path(&state.path, "title-info") && !state.author_parts.is_empty() {
                            let author = state.author_parts.join(" ");
                            book.metadata.add("creator", &author);
                            state.author_parts.clear();
                        }
                    }
                    "genre" => {
                        if is_in_path(&state.path, "title-info") {
                            book.metadata.add("subject", state.text_buf.trim());
                        }
                        state.text_buf.clear();
                    }
                    "lang" | "language" => {
                        if is_in_path(&state.path, "title-info") {
                            book.metadata.set("language", state.text_buf.trim());
                        }
                        state.text_buf.clear();
                    }
                    "date" => {
                        if is_in_path(&state.path, "title-info") {
                            book.metadata.set("date", state.text_buf.trim());
                        }
                        state.text_buf.clear();
                    }
                    "publisher" => {
                        if is_in_path(&state.path, "publish-info") {
                            book.metadata.set("publisher", state.text_buf.trim());
                        }
                        state.text_buf.clear();
                    }
                    "isbn" => {
                        if is_in_path(&state.path, "publish-info") {
                            book.metadata.set("identifier", state.text_buf.trim());
                        }
                        state.text_buf.clear();
                    }
                    "id" => {
                        if is_in_path(&state.path, "document-info") && book.metadata.get_first_value("identifier").is_none() {
                            book.metadata.set("identifier", state.text_buf.trim());
                        }
                        state.text_buf.clear();
                    }
                    _ => {}
                }

                state.path.pop();
            }
            Ok(Event::Text(ref e)) => {
                let text = e.unescape().unwrap_or_default().to_string();
                if state.in_binary {
                    state.text_buf.push_str(&text);
                } else if state.in_title {
                    state.title_buf.push_str(&text);
                } else if state.in_para || state.in_link {
                    state.html.push_str(&convert_utils::xml::escape_xml_text(&text));
                } else if is_in_path(&state.path, "description") {
                    // Metadata text content
                    state.text_buf.push_str(&text);
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                return Err(ConvertError::Fb2(format!("XML parse error: {}", e)));
            }
            _ => {}
        }
        buf.clear();
    }

    // Build the XHTML document
    let title = book.metadata.title().unwrap_or("Untitled").to_string();
    let lang = book.metadata.get_first_value("language").unwrap_or("en").to_string();
    let xhtml = convert_utils::xml::xhtml11_document(&title, &lang, Some("style.css"), &state.html);

    let content_item = ManifestItem::new(
        "content",
        "content.xhtml",
        "application/xhtml+xml",
        ManifestData::Xhtml(xhtml),
    );
    book.manifest.add(content_item);
    book.spine.push("content", true);

    // Default stylesheet
    let css = r#"body { font-family: serif; line-height: 1.6; margin: 1em; }
p { margin: 0.5em 0; text-indent: 1.5em; }
p:first-child { text-indent: 0; }
h1, h2, h3, h4, h5, h6 { text-indent: 0; margin: 1em 0 0.5em; }
img { max-width: 100%; height: auto; }
.poem { margin: 1em 2em; font-style: italic; }
.stanza { margin: 0.5em 0; }
.verse { margin: 0; text-indent: 0; }
.epigraph { margin: 1em 2em; font-style: italic; color: #555; }
.text-author { text-align: right; font-style: italic; }
blockquote { margin: 1em 2em; }
code { font-family: monospace; }"#;
    let css_item = ManifestItem::new("style", "style.css", "text/css", ManifestData::Css(css.to_string()));
    book.manifest.add(css_item);

    // Build TOC from section titles
    for title_text in &state.section_titles {
        book.toc.add(TocEntry::new(title_text, "content.xhtml"));
    }
    if book.toc.entries.is_empty() {
        book.toc.add(TocEntry::new(&title, "content.xhtml"));
    }

    let image_count = book.manifest.iter().filter(|i| i.media_type.starts_with("image/")).count();
    log::info!(
        "Parsed FB2: \"{}\" with {} images, {} sections",
        title,
        image_count,
        state.section_titles.len()
    );

    Ok(book)
}

#[derive(Default)]
struct ParseState {
    path: Vec<String>,
    html: String,
    text_buf: String,
    title_buf: String,
    in_section: bool,
    section_depth: usize,
    in_title: bool,
    in_para: bool,
    in_binary: bool,
    in_link: bool,
    binary_id: Option<String>,
    binary_mime: Option<String>,
    author_parts: Vec<String>,
    section_titles: Vec<String>,
}

fn is_in_path(path: &[String], target: &str) -> bool {
    let parts: Vec<&str> = target.split('/').collect();
    if parts.len() > path.len() {
        return false;
    }
    // Check if any suffix of path matches the target parts
    for window in path.windows(parts.len()) {
        if window.iter().zip(parts.iter()).all(|(a, b)| a == b) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_in_path() {
        let path = vec!["FictionBook".into(), "description".into(), "title-info".into(), "author".into(), "first-name".into()];
        assert!(is_in_path(&path, "title-info/author"));
        assert!(is_in_path(&path, "description"));
        assert!(!is_in_path(&path, "publish-info"));
    }

    #[test]
    fn test_parse_fb2_minimal() {
        let fb2 = r#"<?xml version="1.0" encoding="UTF-8"?>
<FictionBook xmlns="http://www.gribuser.ru/xml/fictionbook/2.0" xmlns:l="http://www.w3.org/1999/xlink">
  <description>
    <title-info>
      <genre>fiction</genre>
      <author>
        <first-name>Leo</first-name>
        <last-name>Tolstoy</last-name>
      </author>
      <book-title>War and Peace</book-title>
      <language>en</language>
    </title-info>
  </description>
  <body>
    <section>
      <title><p>Part One</p></title>
      <p>Well, Prince, so Genoa and Lucca are now just family estates of the Buonapartes.</p>
      <p>But I warn you, if you don't tell me that this means war...</p>
    </section>
  </body>
</FictionBook>"#;

        let dir = std::env::temp_dir().join("test_fb2");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.fb2");
        std::fs::write(&path, fb2).unwrap();

        let result = parse_fb2(&path).unwrap();
        assert_eq!(result.metadata.title().unwrap(), "War and Peace");
        assert!(result.metadata.get_first_value("language").unwrap().contains("en"));

        let xhtml = result.manifest.by_id("content").unwrap().data.as_xhtml().unwrap();
        assert!(xhtml.contains("Genoa and Lucca"));
        assert!(xhtml.contains("<h1>Part One</h1>"));

        assert_eq!(result.toc.entries.len(), 1);
        assert_eq!(result.toc.entries[0].title, "Part One");

        // Cleanup
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_parse_fb2_with_formatting() {
        let fb2 = r#"<?xml version="1.0" encoding="UTF-8"?>
<FictionBook xmlns="http://www.gribuser.ru/xml/fictionbook/2.0">
  <description>
    <title-info>
      <book-title>Test</book-title>
      <language>en</language>
    </title-info>
  </description>
  <body>
    <section>
      <p>Normal <strong>bold</strong> <emphasis>italic</emphasis> text.</p>
    </section>
  </body>
</FictionBook>"#;

        let dir = std::env::temp_dir().join("test_fb2_fmt");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.fb2");
        std::fs::write(&path, fb2).unwrap();

        let result = parse_fb2(&path).unwrap();
        let xhtml = result.manifest.by_id("content").unwrap().data.as_xhtml().unwrap();
        assert!(xhtml.contains("<strong>bold</strong>"));
        assert!(xhtml.contains("<em>italic</em>"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_parse_fb2_with_binary() {
        // Tiny 1x1 PNG as base64
        let png_b64 = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";
        let fb2 = format!(r##"<?xml version="1.0" encoding="UTF-8"?>
<FictionBook xmlns="http://www.gribuser.ru/xml/fictionbook/2.0" xmlns:l="http://www.w3.org/1999/xlink">
  <description>
    <title-info>
      <book-title>Test Images</book-title>
      <language>en</language>
    </title-info>
  </description>
  <body>
    <section>
      <p>Text with image:</p>
      <image l:href="#cover"/>
    </section>
  </body>
  <binary id="cover" content-type="image/png">{}</binary>
</FictionBook>"##, png_b64);

        let dir = std::env::temp_dir().join("test_fb2_img");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.fb2");
        std::fs::write(&path, fb2).unwrap();

        let result = parse_fb2(&path).unwrap();
        let cover = result.manifest.by_id("cover").unwrap();
        assert_eq!(cover.media_type, "image/png");
        assert!(!cover.data.as_binary().unwrap().is_empty());

        let xhtml = result.manifest.by_id("content").unwrap().data.as_xhtml().unwrap();
        assert!(xhtml.contains(r#"<img src="images/cover""#));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_parse_fb2_metadata() {
        let fb2 = r#"<?xml version="1.0" encoding="UTF-8"?>
<FictionBook xmlns="http://www.gribuser.ru/xml/fictionbook/2.0">
  <description>
    <title-info>
      <genre>sf</genre>
      <genre>adventure</genre>
      <author>
        <first-name>Isaac</first-name>
        <last-name>Asimov</last-name>
      </author>
      <book-title>Foundation</book-title>
      <date>1951</date>
      <language>en</language>
    </title-info>
    <publish-info>
      <publisher>Gnome Press</publisher>
      <isbn>978-0553293357</isbn>
    </publish-info>
  </description>
  <body><section><p>Hello</p></section></body>
</FictionBook>"#;

        let dir = std::env::temp_dir().join("test_fb2_meta");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.fb2");
        std::fs::write(&path, fb2).unwrap();

        let result = parse_fb2(&path).unwrap();
        assert_eq!(result.metadata.title().unwrap(), "Foundation");
        assert_eq!(result.metadata.get_first_value("publisher").unwrap(), "Gnome Press");
        assert_eq!(result.metadata.get_first_value("identifier").unwrap(), "978-0553293357");
        assert_eq!(result.metadata.get_first_value("date").unwrap(), "1951");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
