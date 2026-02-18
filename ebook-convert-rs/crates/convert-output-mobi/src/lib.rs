//! MOBI output plugin — serializes BookDocument to MOBI/PRC format.
//!
//! Produces a PalmDOC-compatible PDB file with MOBI6 header + EXTH metadata.
//! Uses no compression for simplicity. Images are appended as PDB records
//! after text records.

use std::path::Path;

use convert_core::book::{BookDocument, EbookFormat, ManifestData};
use convert_core::error::{ConvertError, Result};
use convert_core::options::ConversionOptions;
use convert_core::plugin::OutputPlugin;

use regex::Regex;

/// Maximum size of a text record (4096 bytes, PalmDOC standard).
const TEXT_RECORD_SIZE: usize = 4096;

pub struct MobiOutputPlugin;

impl OutputPlugin for MobiOutputPlugin {
    fn name(&self) -> &str {
        "MOBI Output"
    }

    fn output_format(&self) -> EbookFormat {
        EbookFormat::Mobi
    }

    fn convert(
        &self,
        book: &BookDocument,
        output_path: &Path,
        _options: &ConversionOptions,
    ) -> Result<()> {
        log::info!("Writing MOBI: {}", output_path.display());
        write_mobi(book, output_path)
    }
}

fn write_mobi(book: &BookDocument, output_path: &Path) -> Result<()> {
    let fallback_title = output_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Untitled")
        .to_string();
    let title = book.metadata.title().unwrap_or(&fallback_title);

    // Build HTML content
    let html = build_mobi_html(book);
    let html_bytes = html.as_bytes();

    // Split text into records
    let text_records = split_into_records(html_bytes);
    let text_record_count = text_records.len();

    // Collect image records
    let mut image_records: Vec<Vec<u8>> = Vec::new();
    for item in book.manifest.iter() {
        if item.is_image() {
            if let ManifestData::Binary(ref data) = item.data {
                image_records.push(data.clone());
            }
        }
    }

    // +1 for FLIS, +1 for FCIS, +1 for EOF record
    let total_records =
        1 + text_record_count + image_records.len() + 3; // header + text + images + FLIS + FCIS + EOF

    // Build PDB file
    let mut pdb = Vec::new();

    // -- PDB Header (78 bytes) --
    write_pdb_header(&mut pdb, title, total_records);

    // Build all records first to compute offsets
    let mut all_records: Vec<Vec<u8>> = Vec::new();

    // Record 0: MOBI header record
    let mobi_header = build_mobi_header_record(
        html_bytes.len() as u32,
        text_record_count as u16,
        title,
        book,
        &image_records,
    );
    all_records.push(mobi_header);

    // Text records (1..text_record_count)
    for rec in &text_records {
        all_records.push(rec.clone());
    }

    // Image records
    for img in &image_records {
        all_records.push(img.clone());
    }

    // FLIS record
    all_records.push(build_flis_record());

    // FCIS record
    all_records.push(build_fcis_record(html_bytes.len() as u32));

    // EOF record (empty)
    all_records.push(vec![0xe9, 0x8e, 0x0d, 0x0a]);

    // Write record list (8 bytes per record)
    let record_list_size = total_records * 8;
    let gap = 2;
    let data_start = 78 + record_list_size + gap;

    let mut offset = data_start;
    for (i, rec) in all_records.iter().enumerate() {
        pdb.extend_from_slice(&(offset as u32).to_be_bytes());
        pdb.push(0u8); // attributes
        // unique ID (3 bytes)
        pdb.push(((i >> 16) & 0xFF) as u8);
        pdb.push(((i >> 8) & 0xFF) as u8);
        pdb.push((i & 0xFF) as u8);
        offset += rec.len();
    }

    // Gap bytes
    pdb.extend_from_slice(&[0u8; 2]);

    // Write all record data
    for rec in &all_records {
        pdb.extend_from_slice(rec);
    }

    std::fs::write(output_path, &pdb)
        .map_err(|e| ConvertError::Other(format!("Failed to write MOBI: {}", e)))?;

    Ok(())
}

fn write_pdb_header(pdb: &mut Vec<u8>, title: &str, total_records: usize) {
    let mut name_bytes = [0u8; 32];
    let name = title.as_bytes();
    let copy_len = name.len().min(31);
    name_bytes[..copy_len].copy_from_slice(&name[..copy_len]);
    pdb.extend_from_slice(&name_bytes); // 0-31: name

    pdb.extend_from_slice(&0u16.to_be_bytes()); // 32-33: attributes
    pdb.extend_from_slice(&0u16.to_be_bytes()); // 34-35: version
    pdb.extend_from_slice(&0u32.to_be_bytes()); // 36-39: creation date
    pdb.extend_from_slice(&0u32.to_be_bytes()); // 40-43: modification date
    pdb.extend_from_slice(&0u32.to_be_bytes()); // 44-47: last backup date
    pdb.extend_from_slice(&0u32.to_be_bytes()); // 48-51: modification number
    pdb.extend_from_slice(&0u32.to_be_bytes()); // 52-55: app info offset
    pdb.extend_from_slice(&0u32.to_be_bytes()); // 56-59: sort info offset
    pdb.extend_from_slice(b"BOOK"); // 60-63: type
    pdb.extend_from_slice(b"MOBI"); // 64-67: creator
    pdb.extend_from_slice(&0u32.to_be_bytes()); // 68-71: unique id seed
    pdb.extend_from_slice(&0u32.to_be_bytes()); // 72-75: next record list
    pdb.extend_from_slice(&(total_records as u16).to_be_bytes()); // 76-77: num records
}

fn build_mobi_html(book: &BookDocument) -> String {
    let tag_re = Regex::new(r"(?i)</?(!DOCTYPE|html|head|meta|link|title|xml)[^>]*>").unwrap();
    let mut html = String::new();

    html.push_str("<html><head><title>");
    html.push_str(
        &convert_utils::xml::escape_xml_text(book.metadata.title().unwrap_or("Untitled Document")),
    );
    html.push_str("</title></head><body>\n");

    for spine_item in book.spine.iter() {
        if let Some(item) = book.manifest.by_id(&spine_item.idref) {
            if let ManifestData::Xhtml(ref xhtml) = item.data {
                let body = extract_body(xhtml);
                let cleaned = tag_re.replace_all(&body, "");
                html.push_str(&cleaned);
                html.push('\n');
            }
        }
    }

    html.push_str("</body></html>");
    html
}

fn extract_body(xhtml: &str) -> String {
    let lower = xhtml.to_lowercase();
    if let Some(start) = lower.find("<body") {
        let after = xhtml[start..].find('>').unwrap_or(0);
        let end = lower.rfind("</body>").unwrap_or(xhtml.len());
        xhtml[start + after + 1..end].to_string()
    } else {
        xhtml.to_string()
    }
}

fn split_into_records(data: &[u8]) -> Vec<Vec<u8>> {
    data.chunks(TEXT_RECORD_SIZE)
        .map(|chunk| chunk.to_vec())
        .collect()
}

/// Build EXTH header with metadata.
fn build_exth(book: &BookDocument) -> Vec<u8> {
    let mut exth = Vec::new();
    let mut records: Vec<(u32, Vec<u8>)> = Vec::new();

    // EXTH record types (Calibre ordering):
    // 524 = language, 503 = updated title, 100 = author,
    // 108 = contributor/source, 101 = publisher, 104 = isbn,
    // 103 = description, 105 = subject, 106 = published date

    // Language (type 524)
    let lang = book.metadata.language().unwrap_or("en");
    records.push((524, lang.as_bytes().to_vec()));

    // Updated title (type 503)
    if let Some(title) = book.metadata.title() {
        records.push((503, title.as_bytes().to_vec()));
    }

    // Author (type 100)
    for author in book.metadata.authors() {
        records.push((100, author.as_bytes().to_vec()));
    }

    // Source/contributor (type 108)
    records.push((108, b"ebook-convert-rs".to_vec()));

    // Publisher (type 101)
    if let Some(publisher) = book.metadata.get_first_value("publisher") {
        records.push((101, publisher.as_bytes().to_vec()));
    }

    // ISBN/identifier (type 104)
    if let Some(isbn) = book.metadata.get_first_value("identifier") {
        records.push((104, isbn.as_bytes().to_vec()));
    }

    // Description (type 103)
    if let Some(desc) = book.metadata.get_first_value("description") {
        records.push((103, desc.as_bytes().to_vec()));
    }

    // Subject (type 105)
    if let Some(subject) = book.metadata.get_first_value("subject") {
        records.push((105, subject.as_bytes().to_vec()));
    }

    // Date (type 106)
    if let Some(date) = book.metadata.get_first_value("date") {
        records.push((106, date.as_bytes().to_vec()));
    }

    exth.extend_from_slice(b"EXTH"); // magic
    // Compute total length: 12 (header) + sum of (8 + data_len padded)
    let mut record_bytes = Vec::new();
    for (rec_type, data) in &records {
        let rec_len = 8 + data.len() as u32;
        record_bytes.extend_from_slice(&rec_type.to_be_bytes());
        record_bytes.extend_from_slice(&rec_len.to_be_bytes());
        record_bytes.extend_from_slice(data);
    }

    let total_len = 12 + record_bytes.len() as u32;
    exth.extend_from_slice(&total_len.to_be_bytes()); // header length
    exth.extend_from_slice(&(records.len() as u32).to_be_bytes()); // record count
    exth.extend_from_slice(&record_bytes);

    // Pad to 4-byte boundary
    while exth.len() % 4 != 0 {
        exth.push(0);
    }

    exth
}

fn build_mobi_header_record(
    text_length: u32,
    text_record_count: u16,
    title: &str,
    book: &BookDocument,
    image_records: &[Vec<u8>],
) -> Vec<u8> {
    let mut rec = Vec::new();

    // -- PalmDOC Header (16 bytes) --
    rec.extend_from_slice(&1u16.to_be_bytes()); // compression: 1 = none
    rec.extend_from_slice(&0u16.to_be_bytes()); // unused
    rec.extend_from_slice(&text_length.to_be_bytes()); // text length (4 bytes)
    rec.extend_from_slice(&text_record_count.to_be_bytes()); // record count (2 bytes)
    rec.extend_from_slice(&(TEXT_RECORD_SIZE as u16).to_be_bytes()); // record size (2 bytes)
    rec.extend_from_slice(&0u16.to_be_bytes()); // encryption type: 0 = none (2 bytes)
    rec.extend_from_slice(&0u16.to_be_bytes()); // unused (2 bytes)
    // Total PalmDOC header: 2+2+4+2+2+2+2 = 16 bytes ✓

    // -- MOBI Header (starts at offset 16) --
    let title_bytes = title.as_bytes();

    // Build EXTH first to know its size
    let exth_data = build_exth(book);
    let has_exth = !book.metadata.authors().is_empty()
        || book.metadata.title().is_some();

    // MOBI header is 232 bytes (from "MOBI" to end of header)
    let mobi_header_len: u32 = 232;

    // Full name comes after MOBI header + EXTH
    let full_name_offset = 16 + mobi_header_len + if has_exth { exth_data.len() as u32 } else { 0 };

    let first_image_record = if !image_records.is_empty() {
        (text_record_count as u32) + 1
    } else {
        0xFFFFFFFF
    };

    // FLIS record index = after text + images
    let flis_record = (text_record_count as u32) + (image_records.len() as u32) + 1;
    let fcis_record = flis_record + 1;
    let _last_record = fcis_record + 1; // EOF record

    rec.extend_from_slice(b"MOBI"); // 16-19: magic
    rec.extend_from_slice(&mobi_header_len.to_be_bytes()); // 20-23: header length
    rec.extend_from_slice(&2u32.to_be_bytes()); // 24-27: MOBI type: 2 = book
    rec.extend_from_slice(&65001u32.to_be_bytes()); // 28-31: text encoding: UTF-8
    rec.extend_from_slice(&0u32.to_be_bytes()); // 32-35: unique ID
    rec.extend_from_slice(&6u32.to_be_bytes()); // 36-39: file version: 6 (MOBI6)

    rec.extend_from_slice(&0xFFFFFFFFu32.to_be_bytes()); // 40-43: ortographic index
    rec.extend_from_slice(&0xFFFFFFFFu32.to_be_bytes()); // 44-47: inflection index
    rec.extend_from_slice(&0xFFFFFFFFu32.to_be_bytes()); // 48-51: index names
    rec.extend_from_slice(&0xFFFFFFFFu32.to_be_bytes()); // 52-55: index keys
    rec.extend_from_slice(&0xFFFFFFFFu32.to_be_bytes()); // 56-59: extra index 0
    rec.extend_from_slice(&0xFFFFFFFFu32.to_be_bytes()); // 60-63: extra index 1
    rec.extend_from_slice(&0xFFFFFFFFu32.to_be_bytes()); // 64-67: extra index 2
    rec.extend_from_slice(&0xFFFFFFFFu32.to_be_bytes()); // 68-71: extra index 3
    rec.extend_from_slice(&0xFFFFFFFFu32.to_be_bytes()); // 72-75: extra index 4
    rec.extend_from_slice(&0xFFFFFFFFu32.to_be_bytes()); // 76-79: extra index 5

    rec.extend_from_slice(&(text_record_count as u32 + 1).to_be_bytes()); // 80-83: first non-book index
    rec.extend_from_slice(&full_name_offset.to_be_bytes()); // 84-87: full name offset
    rec.extend_from_slice(&(title_bytes.len() as u32).to_be_bytes()); // 88-91: full name length

    rec.extend_from_slice(&9u32.to_be_bytes()); // 92-95: locale: English
    rec.extend_from_slice(&0u32.to_be_bytes()); // 96-99: input language
    rec.extend_from_slice(&0u32.to_be_bytes()); // 100-103: output language
    rec.extend_from_slice(&6u32.to_be_bytes()); // 104-107: min version: 6
    rec.extend_from_slice(&first_image_record.to_be_bytes()); // 108-111: first image index
    rec.extend_from_slice(&0u32.to_be_bytes()); // 112-115: HUFF record offset
    rec.extend_from_slice(&0u32.to_be_bytes()); // 116-119: HUFF record count
    rec.extend_from_slice(&0u32.to_be_bytes()); // 120-123: DATP record offset
    rec.extend_from_slice(&0u32.to_be_bytes()); // 124-127: DATP record count

    // EXTH flags: 0x50 = has EXTH header (matches Calibre output)
    let exth_flags: u32 = if has_exth { 0x50 } else { 0 };
    rec.extend_from_slice(&exth_flags.to_be_bytes()); // 128-131: EXTH flags

    // 132-163: 32 bytes of unknown/unused
    rec.extend_from_slice(&[0u8; 32]);

    rec.extend_from_slice(&0xFFFFFFFFu32.to_be_bytes()); // 164-167: DRM offset (-1 = none)
    rec.extend_from_slice(&0u32.to_be_bytes()); // 168-171: DRM count
    rec.extend_from_slice(&0u32.to_be_bytes()); // 172-175: DRM size
    rec.extend_from_slice(&0u32.to_be_bytes()); // 176-179: DRM flags

    // 180-191: 12 bytes unused
    rec.extend_from_slice(&[0u8; 12]);

    rec.extend_from_slice(&0xFFFFu16.to_be_bytes()); // 192-193: first content record (0xFFFF = use default)
    rec.extend_from_slice(&(text_record_count + 1).to_be_bytes()); // 194-195: last content record

    rec.extend_from_slice(&1u32.to_be_bytes()); // 196-199: unknown (1)

    rec.extend_from_slice(&flis_record.to_be_bytes()); // 200-203: FLIS record number
    rec.extend_from_slice(&fcis_record.to_be_bytes()); // 204-207: FCIS record number
    rec.extend_from_slice(&1u32.to_be_bytes()); // 208-211: FLIS count
    rec.extend_from_slice(&1u32.to_be_bytes()); // 212-215: FCIS count

    // 216-219: unknown
    rec.extend_from_slice(&0xFFFFFFFFu32.to_be_bytes());
    // 220-223: unknown (0)
    rec.extend_from_slice(&0u32.to_be_bytes());
    // 224-227: unknown
    rec.extend_from_slice(&0xFFFFFFFFu32.to_be_bytes());
    // 228-231: unknown (0)
    rec.extend_from_slice(&0u32.to_be_bytes());

    // 232-235: extra record data flags
    rec.extend_from_slice(&0u32.to_be_bytes());
    // 236-239: INDX record offset (-1 = none)
    rec.extend_from_slice(&0xFFFFFFFFu32.to_be_bytes());

    // Verify MOBI header size (from byte 16 to here should be mobi_header_len)
    let mobi_written = rec.len() - 16;
    if mobi_written < mobi_header_len as usize {
        rec.resize(16 + mobi_header_len as usize, 0);
    }

    // EXTH header (if applicable)
    if has_exth {
        rec.extend_from_slice(&exth_data);
    }

    // Full name (after MOBI header + EXTH)
    rec.extend_from_slice(title_bytes);
    // Pad to 4-byte boundary
    while rec.len() % 4 != 0 {
        rec.push(0);
    }

    rec
}

/// Build FLIS record (Fixed Layout Information Structure).
fn build_flis_record() -> Vec<u8> {
    let mut flis = Vec::new();
    flis.extend_from_slice(b"FLIS"); // magic
    flis.extend_from_slice(&8u32.to_be_bytes()); // fixed length
    flis.extend_from_slice(&65u16.to_be_bytes()); // unknown
    flis.extend_from_slice(&0u16.to_be_bytes()); // unknown
    flis.extend_from_slice(&0u32.to_be_bytes()); // unknown
    flis.extend_from_slice(&0xFFFFFFFFu32.to_be_bytes()); // unknown
    flis.extend_from_slice(&1u16.to_be_bytes()); // unknown
    flis.extend_from_slice(&3u16.to_be_bytes()); // unknown
    flis.extend_from_slice(&3u32.to_be_bytes()); // unknown
    flis.extend_from_slice(&1u32.to_be_bytes()); // unknown
    flis.extend_from_slice(&0xFFFFFFFFu32.to_be_bytes()); // unknown
    flis
}

/// Build FCIS record (Fixed Content Information Structure).
fn build_fcis_record(text_length: u32) -> Vec<u8> {
    let mut fcis = Vec::new();
    fcis.extend_from_slice(b"FCIS"); // magic
    fcis.extend_from_slice(&20u32.to_be_bytes()); // fixed length
    fcis.extend_from_slice(&16u32.to_be_bytes()); // unknown
    fcis.extend_from_slice(&1u32.to_be_bytes()); // unknown
    fcis.extend_from_slice(&0u32.to_be_bytes()); // unknown
    fcis.extend_from_slice(&text_length.to_be_bytes()); // text length
    fcis.extend_from_slice(&0u32.to_be_bytes()); // unknown
    fcis.extend_from_slice(&32u32.to_be_bytes()); // unknown
    fcis.extend_from_slice(&8u32.to_be_bytes()); // unknown
    fcis.extend_from_slice(&1u16.to_be_bytes()); // unknown
    fcis.extend_from_slice(&1u16.to_be_bytes()); // unknown
    fcis.extend_from_slice(&0u32.to_be_bytes()); // unknown
    fcis
}

#[cfg(test)]
mod tests {
    use super::*;
    use convert_core::book::{ManifestItem, TocEntry};

    #[test]
    fn test_extract_body() {
        let xhtml = "<html><body><p>Hello</p></body></html>";
        assert_eq!(extract_body(xhtml), "<p>Hello</p>");
    }

    #[test]
    fn test_split_records() {
        let data = vec![0u8; 10000];
        let records = split_into_records(&data);
        assert_eq!(records.len(), 3);
        assert_eq!(records[0].len(), 4096);
        assert_eq!(records[1].len(), 4096);
        assert_eq!(records[2].len(), 10000 - 2 * 4096);
    }

    #[test]
    fn test_mobi_header_record_structure() {
        let mut book = BookDocument::new();
        book.metadata.set_title("Test Book");
        book.metadata.add("creator", "Author");

        let rec = build_mobi_header_record(5000, 2, "Test Book", &book, &[]);
        // PalmDOC compression = 1 (no compression)
        assert_eq!(u16::from_be_bytes([rec[0], rec[1]]), 1);
        // Text length at bytes 4-7
        assert_eq!(u32::from_be_bytes([rec[4], rec[5], rec[6], rec[7]]), 5000);
        // Record count at bytes 8-9
        assert_eq!(u16::from_be_bytes([rec[8], rec[9]]), 2);
        // MOBI magic at bytes 16-19
        assert_eq!(&rec[16..20], b"MOBI");
        // File version at bytes 36-39 should be 6 (MOBI6)
        assert_eq!(
            u32::from_be_bytes([rec[36], rec[37], rec[38], rec[39]]),
            6
        );
        // Min version at bytes 104-107 should be 6
        assert_eq!(
            u32::from_be_bytes([rec[104], rec[105], rec[106], rec[107]]),
            6
        );
    }

    #[test]
    fn test_mobi_has_exth() {
        let mut book = BookDocument::new();
        book.metadata.set_title("Test Book");
        book.metadata.add("creator", "Test Author");

        let rec = build_mobi_header_record(100, 1, "Test Book", &book, &[]);
        // EXTH flags at bytes 128-131 should be 0x50
        let exth_flags = u32::from_be_bytes([rec[128], rec[129], rec[130], rec[131]]);
        assert_eq!(exth_flags, 0x50, "EXTH flags should be 0x50");
        // EXTH magic should appear after MOBI header (byte 16 + 232 = 248)
        assert_eq!(&rec[248..252], b"EXTH");
    }

    #[test]
    fn test_mobi_output_basic() {
        let mut book = BookDocument::new();
        book.metadata.set_title("Test MOBI");
        book.metadata.add("creator", "Author");

        let xhtml =
            "<html><body><h1>Chapter 1</h1><p>Hello world.</p></body></html>".to_string();
        let item = ManifestItem::new(
            "ch1",
            "ch1.xhtml",
            "application/xhtml+xml",
            ManifestData::Xhtml(xhtml),
        );
        book.manifest.add(item);
        book.spine.push("ch1", true);
        book.toc.add(TocEntry::new("Chapter 1", "ch1.xhtml"));

        let tmp = std::env::temp_dir().join("test_output_mobi.mobi");
        let opts = ConversionOptions::default();
        MobiOutputPlugin.convert(&book, &tmp, &opts).unwrap();

        let data = std::fs::read(&tmp).unwrap();
        assert!(data.len() > 78);
        // Check PDB type/creator
        assert_eq!(&data[60..64], b"BOOK");
        assert_eq!(&data[64..68], b"MOBI");
        std::fs::remove_file(&tmp).ok();
    }

    #[test]
    fn test_build_mobi_html() {
        let mut book = BookDocument::new();
        book.metadata.set_title("Test");
        let xhtml = "<html><body><p>Content here</p></body></html>".to_string();
        let item = ManifestItem::new(
            "ch1",
            "ch1.xhtml",
            "application/xhtml+xml",
            ManifestData::Xhtml(xhtml),
        );
        book.manifest.add(item);
        book.spine.push("ch1", true);

        let html = build_mobi_html(&book);
        assert!(html.contains("<title>Test</title>"));
        assert!(html.contains("<p>Content here</p>"));
        assert!(html.starts_with("<html>"));
        assert!(html.ends_with("</html>"));
    }

    #[test]
    fn test_flis_record() {
        let flis = build_flis_record();
        assert_eq!(&flis[..4], b"FLIS");
    }

    #[test]
    fn test_fcis_record() {
        let fcis = build_fcis_record(5000);
        assert_eq!(&fcis[..4], b"FCIS");
        // Text length at bytes 20-23
        assert_eq!(
            u32::from_be_bytes([fcis[20], fcis[21], fcis[22], fcis[23]]),
            5000
        );
    }

    #[test]
    fn test_exth_building() {
        let mut book = BookDocument::new();
        book.metadata.set_title("My Book");
        book.metadata.add("creator", "Jane Doe");
        book.metadata.add("publisher", "Test Press");

        let exth = build_exth(&book);
        assert_eq!(&exth[..4], b"EXTH");
        // Should have: language(524), title(503), author(100), source(108), publisher(101)
        let count = u32::from_be_bytes([exth[8], exth[9], exth[10], exth[11]]);
        assert_eq!(count, 5);
    }
}
