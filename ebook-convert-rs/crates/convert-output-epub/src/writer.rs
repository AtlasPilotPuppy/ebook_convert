//! EPUB writer — assembles a valid EPUB file from BookDocument.

use std::path::Path;

use rayon::prelude::*;

use convert_core::book::{BookDocument, ManifestData};
use convert_core::error::{ConvertError, Result};
use convert_core::options::ConversionOptions;
use convert_utils::archive::ZipBuilder;
use convert_utils::xml::XmlBuilder;

/// Write a BookDocument as an EPUB file.
pub fn write_epub(
    book: &BookDocument,
    output_path: &Path,
    options: &ConversionOptions,
) -> Result<()> {
    let mut zip = ZipBuilder::new(output_path)
        .map_err(|e| ConvertError::Epub(format!("Failed to create EPUB: {}", e)))?;

    // 1. mimetype (must be first, stored uncompressed)
    zip.add_stored("mimetype", b"application/epub+zip")
        .map_err(|e| ConvertError::Epub(format!("Failed to write mimetype: {}", e)))?;

    // 2. META-INF/container.xml
    let container_xml = generate_container_xml();
    zip.add_file("META-INF/container.xml", container_xml.as_bytes())
        .map_err(|e| ConvertError::Epub(format!("Failed to write container.xml: {}", e)))?;

    // 3. Pre-resolve Lazy items in parallel, then write all content to zip sequentially
    // Collect items that need lazy loading
    let lazy_items: Vec<(String, std::path::PathBuf)> = book
        .manifest
        .iter()
        .filter_map(|item| {
            if let ManifestData::Lazy(ref file_path) = item.data {
                Some((item.href.clone(), file_path.clone()))
            } else {
                None
            }
        })
        .collect();

    // Read lazy files in parallel
    let lazy_data: Vec<(String, std::result::Result<Vec<u8>, ConvertError>)> = lazy_items
        .into_par_iter()
        .map(|(href, file_path)| {
            let result = std::fs::read(&file_path).map_err(|e| {
                ConvertError::Epub(format!(
                    "Failed to read lazy content {}: {}",
                    file_path.display(),
                    e
                ))
            });
            (href, result)
        })
        .collect();

    // Build href→data lookup for lazy items
    let mut lazy_map: std::collections::HashMap<String, Vec<u8>> = std::collections::HashMap::new();
    for (href, result) in lazy_data {
        lazy_map.insert(href, result?);
    }

    // Write all content to zip sequentially
    for item in book.manifest.iter() {
        let path = format!("OEBPS/{}", item.href);
        let is_precompressed = is_precompressed_media(&item.media_type);
        match &item.data {
            ManifestData::Xhtml(s) => {
                zip.add_file(&path, s.as_bytes())
                    .map_err(|e| ConvertError::Epub(format!("Failed to write {}: {}", path, e)))?;
            }
            ManifestData::Css(s) => {
                zip.add_file(&path, s.as_bytes())
                    .map_err(|e| ConvertError::Epub(format!("Failed to write {}: {}", path, e)))?;
            }
            ManifestData::Binary(b) => {
                if is_precompressed {
                    zip.add_stored(&path, b).map_err(|e| {
                        ConvertError::Epub(format!("Failed to write {}: {}", path, e))
                    })?;
                } else {
                    zip.add_file(&path, b).map_err(|e| {
                        ConvertError::Epub(format!("Failed to write {}: {}", path, e))
                    })?;
                }
            }
            ManifestData::Lazy(_) => {
                if let Some(data) = lazy_map.get(&item.href) {
                    if is_precompressed {
                        zip.add_stored(&path, data).map_err(|e| {
                            ConvertError::Epub(format!("Failed to write {}: {}", path, e))
                        })?;
                    } else {
                        zip.add_file(&path, data).map_err(|e| {
                            ConvertError::Epub(format!("Failed to write {}: {}", path, e))
                        })?;
                    }
                }
            }
            ManifestData::Empty => continue,
        }
    }

    // 4. OPF package document
    let opf = generate_opf(book, options);
    zip.add_file("OEBPS/content.opf", opf.as_bytes())
        .map_err(|e| ConvertError::Epub(format!("Failed to write content.opf: {}", e)))?;

    // 5. NCX navigation document (EPUB 2)
    let ncx = generate_ncx(book);
    zip.add_file("OEBPS/toc.ncx", ncx.as_bytes())
        .map_err(|e| ConvertError::Epub(format!("Failed to write toc.ncx: {}", e)))?;

    zip.finish()
        .map_err(|e| ConvertError::Epub(format!("Failed to finalize EPUB: {}", e)))?;

    log::info!("EPUB written successfully: {}", output_path.display());
    Ok(())
}

/// Check if a media type is already compressed (deflating would waste CPU).
fn is_precompressed_media(media_type: &str) -> bool {
    matches!(
        media_type,
        "image/png"
            | "image/jpeg"
            | "image/gif"
            | "image/webp"
            | "image/avif"
            | "audio/mpeg"
            | "audio/ogg"
            | "video/mp4"
            | "application/x-font-opentype"
            | "application/x-font-truetype"
    )
}

fn generate_container_xml() -> String {
    let mut xml = XmlBuilder::new();
    xml.open_tag(
        "container",
        &[
            ("version", "1.0"),
            ("xmlns", "urn:oasis:names:tc:opendocument:xmlns:container"),
        ],
    )
    .open_tag("rootfiles", &[])
    .empty_tag(
        "rootfile",
        &[
            ("full-path", "OEBPS/content.opf"),
            ("media-type", "application/oebps-package+xml"),
        ],
    )
    .close_tag("rootfiles")
    .close_tag("container");
    xml.build()
}

fn generate_opf(book: &BookDocument, _options: &ConversionOptions) -> String {
    let uid = book
        .uid
        .as_deref()
        .unwrap_or("urn:uuid:00000000-0000-0000-0000-000000000000");
    let title = book.metadata.title().unwrap_or("Untitled");
    let language = book.metadata.language().unwrap_or("en");

    let mut xml = XmlBuilder::new();
    xml.open_tag(
        "package",
        &[
            ("xmlns", "http://www.idpf.org/2007/opf"),
            ("unique-identifier", "bookid"),
            ("version", "2.0"),
        ],
    );

    // Metadata
    xml.open_tag(
        "metadata",
        &[
            ("xmlns:dc", "http://purl.org/dc/elements/1.1/"),
            ("xmlns:opf", "http://www.idpf.org/2007/opf"),
        ],
    );
    xml.text_element("dc:title", title, &[]);
    xml.text_element("dc:language", language, &[]);
    xml.text_element("dc:identifier", uid, &[("id", "bookid")]);

    for author in book.metadata.authors() {
        xml.text_element("dc:creator", author, &[("opf:role", "aut")]);
    }

    if let Some(desc) = book.metadata.description() {
        xml.text_element("dc:description", desc, &[]);
    }
    if let Some(publisher) = book.metadata.publisher() {
        xml.text_element("dc:publisher", publisher, &[]);
    }
    if let Some(date) = book.metadata.date() {
        xml.text_element("dc:date", date, &[]);
    }

    xml.close_tag("metadata");

    // Manifest
    xml.open_tag("manifest", &[]);
    xml.empty_tag(
        "item",
        &[
            ("id", "ncx"),
            ("href", "toc.ncx"),
            ("media-type", "application/x-dtbncx+xml"),
        ],
    );

    for item in book.manifest.iter() {
        xml.empty_tag(
            "item",
            &[
                ("id", &item.id),
                ("href", &item.href),
                ("media-type", &item.media_type),
            ],
        );
    }
    xml.close_tag("manifest");

    // Spine
    xml.open_tag("spine", &[("toc", "ncx")]);
    for spine_item in book.spine.iter() {
        if spine_item.linear {
            xml.empty_tag("itemref", &[("idref", &spine_item.idref)]);
        } else {
            xml.empty_tag("itemref", &[("idref", &spine_item.idref), ("linear", "no")]);
        }
    }
    xml.close_tag("spine");

    // Guide
    if !book.guide.is_empty() {
        xml.open_tag("guide", &[]);
        for guide_ref in book.guide.iter() {
            xml.empty_tag(
                "reference",
                &[
                    ("type", &guide_ref.ref_type),
                    ("title", &guide_ref.title),
                    ("href", &guide_ref.href),
                ],
            );
        }
        xml.close_tag("guide");
    }

    xml.close_tag("package");
    xml.build()
}

fn generate_ncx(book: &BookDocument) -> String {
    let uid = book
        .uid
        .as_deref()
        .unwrap_or("urn:uuid:00000000-0000-0000-0000-000000000000");
    let title = book.metadata.title().unwrap_or("Untitled");

    let mut xml = XmlBuilder::new();
    xml.raw("<!DOCTYPE ncx PUBLIC \"-//NISO//DTD ncx 2005-1//EN\" \"http://www.daisy.org/z3986/2005/ncx-2005-1.dtd\">\n");
    xml.open_tag(
        "ncx",
        &[
            ("xmlns", "http://www.daisy.org/z3986/2005/ncx/"),
            ("version", "2005-1"),
        ],
    );

    // Head
    xml.open_tag("head", &[]);
    xml.empty_tag("meta", &[("name", "dtb:uid"), ("content", uid)]);
    xml.empty_tag("meta", &[("name", "dtb:depth"), ("content", "1")]);
    xml.empty_tag("meta", &[("name", "dtb:totalPageCount"), ("content", "0")]);
    xml.empty_tag("meta", &[("name", "dtb:maxPageNumber"), ("content", "0")]);
    xml.close_tag("head");

    // Doc title
    xml.open_tag("docTitle", &[]);
    xml.text_element("text", title, &[]);
    xml.close_tag("docTitle");

    // Nav map
    xml.open_tag("navMap", &[]);
    let mut play_order = 1;
    for entry in book.toc.iter_depth_first() {
        write_ncx_nav_point(&mut xml, entry, &mut play_order);
    }
    xml.close_tag("navMap");

    xml.close_tag("ncx");
    xml.build()
}

fn write_ncx_nav_point(
    xml: &mut XmlBuilder,
    entry: &convert_core::book::TocEntry,
    play_order: &mut u32,
) {
    let id = format!("navPoint-{}", play_order);
    let po = play_order.to_string();
    xml.open_tag("navPoint", &[("id", &id), ("playOrder", &po)]);
    xml.open_tag("navLabel", &[]);
    xml.text_element("text", &entry.title, &[]);
    xml.close_tag("navLabel");
    xml.empty_tag("content", &[("src", &entry.href)]);
    // Note: children are handled by the depth-first iterator already flattening them
    xml.close_tag("navPoint");
    *play_order += 1;
}

#[cfg(test)]
mod tests {
    use super::*;
    use convert_core::book::{BookDocument, ManifestData, ManifestItem, TocEntry};

    fn make_test_book() -> BookDocument {
        let mut book = BookDocument::new();
        book.uid = Some("test-uid-123".to_string());
        book.metadata.set_title("Test Book");
        book.metadata.add("creator", "Test Author");
        book.metadata.set("language", "en");

        let xhtml = "<html><body><p>Hello</p></body></html>".to_string();
        let item = ManifestItem::new(
            "ch1",
            "chapter1.xhtml",
            "application/xhtml+xml",
            ManifestData::Xhtml(xhtml),
        );
        book.manifest.add(item);
        book.spine.push("ch1", true);
        book.toc.add(TocEntry::new("Chapter 1", "chapter1.xhtml"));

        book
    }

    #[test]
    fn test_generate_container_xml() {
        let xml = generate_container_xml();
        assert!(xml.contains("OEBPS/content.opf"));
        assert!(xml.contains("urn:oasis:names:tc:opendocument:xmlns:container"));
    }

    #[test]
    fn test_generate_opf() {
        let book = make_test_book();
        let opts = ConversionOptions::default();
        let opf = generate_opf(&book, &opts);
        assert!(opf.contains("<dc:title>Test Book</dc:title>"));
        assert!(opf.contains("chapter1.xhtml"));
        assert!(opf.contains("idref=\"ch1\""));
    }

    #[test]
    fn test_generate_ncx() {
        let book = make_test_book();
        let ncx = generate_ncx(&book);
        assert!(ncx.contains("Chapter 1"));
        assert!(ncx.contains("chapter1.xhtml"));
        assert!(ncx.contains("navPoint"));
    }

    #[test]
    fn test_write_epub() {
        let book = make_test_book();
        let opts = ConversionOptions::default();
        let tmp = std::env::temp_dir().join("test_output.epub");
        write_epub(&book, &tmp, &opts).unwrap();
        assert!(tmp.exists());
        assert!(std::fs::metadata(&tmp).unwrap().len() > 0);
        std::fs::remove_file(&tmp).ok();
    }
}
