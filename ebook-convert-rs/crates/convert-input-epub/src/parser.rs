//! EPUB parsing — reads container.xml, OPF, NCX, and content files.

use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;

use quick_xml::events::Event;
use quick_xml::Reader;
use rayon::prelude::*;
use zip::read::ZipArchive;

use convert_core::book::{
    BookDocument, GuideRef, ManifestData, ManifestItem, TocEntry,
};
use convert_core::error::{ConvertError, Result};
use convert_utils::mime;

/// Parse an EPUB file into a BookDocument.
pub fn parse_epub(path: &Path) -> Result<BookDocument> {
    let file = File::open(path)
        .map_err(|e| ConvertError::Epub(format!("Cannot open {}: {}", path.display(), e)))?;
    let mut archive = ZipArchive::new(file)
        .map_err(|e| ConvertError::Epub(format!("Invalid ZIP: {}", e)))?;

    // 1. Find the OPF path from META-INF/container.xml
    let opf_path = read_container_xml(&mut archive)?;
    log::info!("OPF path: {}", opf_path);

    // Compute base directory for resolving relative hrefs in OPF
    let opf_dir = opf_path
        .rfind('/')
        .map(|i| &opf_path[..=i])
        .unwrap_or("");

    // 2. Parse the OPF file
    let opf_content = read_archive_entry(&mut archive, &opf_path)?;
    let opf_str = String::from_utf8_lossy(&opf_content).to_string();

    let mut book = BookDocument::new();

    // Parse metadata
    parse_opf_metadata(&opf_str, &mut book);

    // Parse manifest
    let manifest_map = parse_opf_manifest(&opf_str, opf_dir);

    // Parse spine
    let spine_idrefs = parse_opf_spine(&opf_str);

    // Parse guide
    parse_opf_guide(&opf_str, opf_dir, &mut book);

    // 3. Read all raw bytes from ZIP (sequential — ZIP isn't thread-safe)
    let raw_entries: Vec<(String, String, String, Vec<u8>)> = manifest_map
        .iter()
        .filter_map(|(id, (href, media_type))| {
            let full_path = if opf_dir.is_empty() {
                href.clone()
            } else {
                format!("{}{}", opf_dir, href)
            };

            match read_archive_entry(&mut archive, &full_path) {
                Ok(bytes) => Some((id.clone(), href.clone(), media_type.clone(), bytes)),
                Err(e) => {
                    log::warn!("Failed to read {}: {}", full_path, e);
                    Some((id.clone(), href.clone(), media_type.clone(), Vec::new()))
                }
            }
        })
        .collect();

    // 4. Classify content in parallel with rayon (text decoding, XHTML/CSS detection)
    let manifest_items: Vec<ManifestItem> = raw_entries
        .into_par_iter()
        .map(|(id, href, media_type, bytes)| {
            let data = if bytes.is_empty() {
                ManifestData::Empty
            } else if mime::is_text_mime(&media_type) {
                let text = String::from_utf8_lossy(&bytes).to_string();
                if media_type == "application/xhtml+xml"
                    || media_type == "text/html"
                    || media_type.contains("xml")
                {
                    ManifestData::Xhtml(text)
                } else if media_type == "text/css" {
                    ManifestData::Css(text)
                } else {
                    ManifestData::Xhtml(text)
                }
            } else {
                ManifestData::Binary(bytes)
            };

            ManifestItem::new(&id, &href, &media_type, data)
        })
        .collect();

    for item in manifest_items {
        book.manifest.add(item);
    }

    // 4. Build spine from idrefs
    for idref in &spine_idrefs {
        if manifest_map.contains_key(idref) {
            book.spine.push(idref, true);
        }
    }

    // 5. Try to parse NCX for TOC
    if let Some(ncx_id) = find_ncx_id(&opf_str) {
        if let Some((ncx_href, _)) = manifest_map.get(&ncx_id) {
            let ncx_path = if opf_dir.is_empty() {
                ncx_href.clone()
            } else {
                format!("{}{}", opf_dir, ncx_href)
            };
            if let Ok(ncx_data) = read_archive_entry(&mut archive, &ncx_path) {
                let ncx_str = String::from_utf8_lossy(&ncx_data).to_string();
                parse_ncx(&ncx_str, &mut book);
            }
        }
    }

    book.toc.rationalize_play_orders();

    log::info!(
        "EPUB loaded: {} manifest items, {} spine items, {} TOC entries",
        book.manifest.len(),
        book.spine.len(),
        book.toc.entries.len()
    );

    Ok(book)
}

/// Read META-INF/container.xml and return the OPF file path.
fn read_container_xml(archive: &mut ZipArchive<File>) -> Result<String> {
    let data = read_archive_entry(archive, "META-INF/container.xml")?;
    let xml = String::from_utf8_lossy(&data);

    let mut reader = Reader::from_str(&xml);
    loop {
        match reader.read_event() {
            Ok(Event::Empty(ref e)) | Ok(Event::Start(ref e)) => {
                let local = String::from_utf8_lossy(e.local_name().as_ref()).to_string();
                if local == "rootfile" {
                    for attr in e.attributes().flatten() {
                        let key = String::from_utf8_lossy(attr.key.local_name().as_ref()).to_string();
                        if key == "full-path" {
                            return Ok(String::from_utf8_lossy(&attr.value).to_string());
                        }
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(ConvertError::Epub(format!("XML error in container.xml: {}", e))),
            _ => {}
        }
    }

    Err(ConvertError::Epub("No rootfile found in container.xml".to_string()))
}

/// Parse OPF metadata section.
fn parse_opf_metadata(opf: &str, book: &mut BookDocument) {
    let mut reader = Reader::from_str(opf);
    let mut in_metadata = false;
    let mut current_tag = String::new();
    let mut current_attrs: HashMap<String, String> = HashMap::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) => {
                let local = String::from_utf8_lossy(e.local_name().as_ref()).to_string();
                if local == "metadata" {
                    in_metadata = true;
                } else if in_metadata {
                    current_tag = local.clone();
                    current_attrs.clear();
                    for attr in e.attributes().flatten() {
                        let key = String::from_utf8_lossy(attr.key.local_name().as_ref()).to_string();
                        let val = String::from_utf8_lossy(&attr.value).to_string();
                        current_attrs.insert(key, val);
                    }
                }
            }
            Ok(Event::Text(ref t)) => {
                if in_metadata && !current_tag.is_empty() {
                    if let Ok(text) = t.unescape() {
                        let text = text.trim().to_string();
                        if !text.is_empty() {
                            match current_tag.as_str() {
                                "title" => book.metadata.set_title(&text),
                                "creator" => book.metadata.add("creator", &text),
                                "language" => book.metadata.set("language", &text),
                                "identifier" => {
                                    book.metadata.add("identifier", &text);
                                    if current_attrs.get("id").map(|s| s.as_str()) == Some("bookid")
                                        || book.uid.is_none()
                                    {
                                        book.uid = Some(text.clone());
                                    }
                                }
                                "description" => book.metadata.set("description", &text),
                                "publisher" => book.metadata.set("publisher", &text),
                                "date" => book.metadata.set("date", &text),
                                "subject" => book.metadata.add("subject", &text),
                                "rights" => book.metadata.set("rights", &text),
                                _ => {
                                    book.metadata.add(&current_tag, &text);
                                }
                            }
                        }
                    }
                }
            }
            Ok(Event::End(ref e)) => {
                let local = String::from_utf8_lossy(e.local_name().as_ref()).to_string();
                if local == "metadata" {
                    in_metadata = false;
                }
                if in_metadata {
                    current_tag.clear();
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
    }
}

/// Parse OPF manifest section. Returns map of id -> (href, media-type).
fn parse_opf_manifest(opf: &str, _opf_dir: &str) -> HashMap<String, (String, String)> {
    let mut items = HashMap::new();
    let mut reader = Reader::from_str(opf);
    let mut in_manifest = false;

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                let local = String::from_utf8_lossy(e.local_name().as_ref()).to_string();
                if local == "manifest" {
                    in_manifest = true;
                } else if local == "item" && in_manifest {
                    let mut id = String::new();
                    let mut href = String::new();
                    let mut media_type = String::new();

                    for attr in e.attributes().flatten() {
                        let key = String::from_utf8_lossy(attr.key.local_name().as_ref()).to_string();
                        let val = String::from_utf8_lossy(&attr.value).to_string();
                        match key.as_str() {
                            "id" => id = val,
                            "href" => href = val,
                            "media-type" => media_type = val,
                            _ => {}
                        }
                    }

                    if !id.is_empty() && !href.is_empty() {
                        // URL-decode the href
                        let href = percent_decode(&href);
                        items.insert(id, (href, media_type));
                    }
                }
            }
            Ok(Event::End(ref e)) => {
                let local = String::from_utf8_lossy(e.local_name().as_ref()).to_string();
                if local == "manifest" {
                    in_manifest = false;
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
    }

    items
}

/// Parse OPF spine section. Returns ordered list of idrefs.
fn parse_opf_spine(opf: &str) -> Vec<String> {
    let mut idrefs = Vec::new();
    let mut reader = Reader::from_str(opf);
    let mut in_spine = false;

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                let local = String::from_utf8_lossy(e.local_name().as_ref()).to_string();
                if local == "spine" {
                    in_spine = true;
                } else if local == "itemref" && in_spine {
                    for attr in e.attributes().flatten() {
                        let key = String::from_utf8_lossy(attr.key.local_name().as_ref()).to_string();
                        if key == "idref" {
                            idrefs.push(String::from_utf8_lossy(&attr.value).to_string());
                        }
                    }
                }
            }
            Ok(Event::End(ref e)) => {
                let local = String::from_utf8_lossy(e.local_name().as_ref()).to_string();
                if local == "spine" {
                    in_spine = false;
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
    }

    idrefs
}

/// Parse OPF guide section.
fn parse_opf_guide(opf: &str, opf_dir: &str, book: &mut BookDocument) {
    let mut reader = Reader::from_str(opf);
    let mut in_guide = false;

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                let local = String::from_utf8_lossy(e.local_name().as_ref()).to_string();
                if local == "guide" {
                    in_guide = true;
                } else if local == "reference" && in_guide {
                    let mut ref_type = String::new();
                    let mut title = String::new();
                    let mut href = String::new();

                    for attr in e.attributes().flatten() {
                        let key = String::from_utf8_lossy(attr.key.local_name().as_ref()).to_string();
                        let val = String::from_utf8_lossy(&attr.value).to_string();
                        match key.as_str() {
                            "type" => ref_type = val,
                            "title" => title = val,
                            "href" => href = val,
                            _ => {}
                        }
                    }

                    if !ref_type.is_empty() && !href.is_empty() {
                        // Strip opf_dir prefix if present
                        let href = if !opf_dir.is_empty() && href.starts_with(opf_dir) {
                            href[opf_dir.len()..].to_string()
                        } else {
                            href
                        };
                        book.guide.add(GuideRef::new(ref_type, title, href));
                    }
                }
            }
            Ok(Event::End(ref e)) => {
                let local = String::from_utf8_lossy(e.local_name().as_ref()).to_string();
                if local == "guide" {
                    in_guide = false;
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
    }
}

/// Find the NCX manifest item id from the spine toc attribute.
fn find_ncx_id(opf: &str) -> Option<String> {
    let mut reader = Reader::from_str(opf);

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                let local = String::from_utf8_lossy(e.local_name().as_ref()).to_string();
                if local == "spine" {
                    for attr in e.attributes().flatten() {
                        let key = String::from_utf8_lossy(attr.key.local_name().as_ref()).to_string();
                        if key == "toc" {
                            return Some(String::from_utf8_lossy(&attr.value).to_string());
                        }
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
    }

    None
}

/// Parse NCX navMap into TOC entries.
fn parse_ncx(ncx: &str, book: &mut BookDocument) {
    let mut reader = Reader::from_str(ncx);
    let mut in_nav_point = false;
    let mut in_nav_label = false;
    let mut in_text = false;
    let mut current_title = String::new();
    let mut current_href = String::new();
    let mut depth = 0u32;

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) => {
                let local = String::from_utf8_lossy(e.local_name().as_ref()).to_string();
                match local.as_str() {
                    "navPoint" => {
                        in_nav_point = true;
                        depth += 1;
                        current_title.clear();
                        current_href.clear();
                    }
                    "navLabel" if in_nav_point => {
                        in_nav_label = true;
                    }
                    "text" if in_nav_label => {
                        in_text = true;
                    }
                    _ => {}
                }
            }
            Ok(Event::Empty(ref e)) => {
                let local = String::from_utf8_lossy(e.local_name().as_ref()).to_string();
                if local == "content" && in_nav_point {
                    for attr in e.attributes().flatten() {
                        let key = String::from_utf8_lossy(attr.key.local_name().as_ref()).to_string();
                        if key == "src" {
                            current_href = String::from_utf8_lossy(&attr.value).to_string();
                        }
                    }
                }
            }
            Ok(Event::Text(ref t)) => {
                if in_text {
                    if let Ok(text) = t.unescape() {
                        current_title.push_str(text.trim());
                    }
                }
            }
            Ok(Event::End(ref e)) => {
                let local = String::from_utf8_lossy(e.local_name().as_ref()).to_string();
                match local.as_str() {
                    "navPoint" => {
                        if !current_title.is_empty() && !current_href.is_empty() {
                            let entry = TocEntry::new(&current_title, &current_href);
                            book.toc.add(entry);
                        }
                        depth = depth.saturating_sub(1);
                        in_nav_point = depth > 0;
                    }
                    "navLabel" => {
                        in_nav_label = false;
                    }
                    "text" => {
                        in_text = false;
                    }
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
    }
}

fn read_archive_entry(archive: &mut ZipArchive<File>, name: &str) -> Result<Vec<u8>> {
    let mut entry = archive
        .by_name(name)
        .map_err(|e| ConvertError::Epub(format!("Entry '{}' not found: {}", name, e)))?;
    let mut buf = Vec::new();
    entry
        .read_to_end(&mut buf)
        .map_err(|e| ConvertError::Epub(format!("Failed to read '{}': {}", name, e)))?;
    Ok(buf)
}

/// Simple percent-decoding for EPUB hrefs.
fn percent_decode(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(byte) = u8::from_str_radix(
                &s[i + 1..i + 3],
                16,
            ) {
                result.push(byte as char);
                i += 3;
                continue;
            }
        }
        result.push(bytes[i] as char);
        i += 1;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_opf_metadata() {
        let opf = r#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="2.0">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:title>Test Book</dc:title>
    <dc:creator>Author One</dc:creator>
    <dc:language>en</dc:language>
    <dc:identifier id="bookid">urn:uuid:12345</dc:identifier>
  </metadata>
</package>"#;

        let mut book = BookDocument::new();
        parse_opf_metadata(opf, &mut book);

        assert_eq!(book.metadata.title(), Some("Test Book"));
        assert_eq!(book.metadata.authors(), vec!["Author One"]);
        assert_eq!(book.metadata.language(), Some("en"));
        assert_eq!(book.uid, Some("urn:uuid:12345".to_string()));
    }

    #[test]
    fn test_parse_opf_manifest() {
        let opf = r#"<?xml version="1.0"?>
<package xmlns="http://www.idpf.org/2007/opf">
  <manifest>
    <item id="ch1" href="chapter1.xhtml" media-type="application/xhtml+xml"/>
    <item id="style" href="style.css" media-type="text/css"/>
    <item id="img1" href="images/cover.jpg" media-type="image/jpeg"/>
    <item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/>
  </manifest>
</package>"#;

        let items = parse_opf_manifest(opf, "");
        assert_eq!(items.len(), 4);
        assert_eq!(items["ch1"].0, "chapter1.xhtml");
        assert_eq!(items["ch1"].1, "application/xhtml+xml");
        assert_eq!(items["img1"].0, "images/cover.jpg");
    }

    #[test]
    fn test_parse_opf_spine() {
        let opf = r#"<?xml version="1.0"?>
<package xmlns="http://www.idpf.org/2007/opf">
  <spine toc="ncx">
    <itemref idref="ch1"/>
    <itemref idref="ch2"/>
    <itemref idref="ch3"/>
  </spine>
</package>"#;

        let spine = parse_opf_spine(opf);
        assert_eq!(spine, vec!["ch1", "ch2", "ch3"]);
    }

    #[test]
    fn test_find_ncx_id() {
        let opf = r#"<package><spine toc="ncx"><itemref idref="ch1"/></spine></package>"#;
        assert_eq!(find_ncx_id(opf), Some("ncx".to_string()));
    }

    #[test]
    fn test_parse_ncx() {
        let ncx = r#"<?xml version="1.0"?>
<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/">
  <navMap>
    <navPoint id="np1" playOrder="1">
      <navLabel><text>Chapter 1</text></navLabel>
      <content src="chapter1.xhtml"/>
    </navPoint>
    <navPoint id="np2" playOrder="2">
      <navLabel><text>Chapter 2</text></navLabel>
      <content src="chapter2.xhtml"/>
    </navPoint>
  </navMap>
</ncx>"#;

        let mut book = BookDocument::new();
        parse_ncx(ncx, &mut book);

        assert_eq!(book.toc.entries.len(), 2);
        assert_eq!(book.toc.entries[0].title, "Chapter 1");
        assert_eq!(book.toc.entries[0].href, "chapter1.xhtml");
        assert_eq!(book.toc.entries[1].title, "Chapter 2");
    }

    #[test]
    fn test_percent_decode() {
        assert_eq!(percent_decode("hello%20world"), "hello world");
        assert_eq!(percent_decode("file%2Fname"), "file/name");
        assert_eq!(percent_decode("normal"), "normal");
    }
}
