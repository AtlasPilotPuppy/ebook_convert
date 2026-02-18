//! ZIP archive utilities for reading/writing EPUB and DOCX files.

use std::fs::File;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use zip::read::ZipArchive;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

/// Extract all files from a ZIP archive to a directory.
pub fn extract_zip(zip_path: &Path, output_dir: &Path) -> io::Result<Vec<PathBuf>> {
    let file = File::open(zip_path)?;
    let mut archive = ZipArchive::new(file)?;
    let mut extracted = Vec::new();

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let name = entry.name().to_string();

        // Skip directories
        if name.ends_with('/') {
            let dir_path = output_dir.join(&name);
            std::fs::create_dir_all(&dir_path)?;
            continue;
        }

        let out_path = output_dir.join(&name);
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mut outfile = File::create(&out_path)?;
        io::copy(&mut entry, &mut outfile)?;
        extracted.push(out_path);
    }

    Ok(extracted)
}

/// Read a single file from inside a ZIP archive.
pub fn read_zip_entry(zip_path: &Path, entry_name: &str) -> io::Result<Vec<u8>> {
    let file = File::open(zip_path)?;
    let mut archive = ZipArchive::new(file)?;
    let mut entry = archive.by_name(entry_name)?;
    let mut buf = Vec::new();
    entry.read_to_end(&mut buf)?;
    Ok(buf)
}

/// List all entries in a ZIP archive.
pub fn list_zip_entries(zip_path: &Path) -> io::Result<Vec<String>> {
    let file = File::open(zip_path)?;
    let archive = ZipArchive::new(file)?;
    let entries = (0..archive.len())
        .filter_map(|i| archive.name_for_index(i).map(|s| s.to_string()))
        .collect();
    Ok(entries)
}

/// Builder for creating ZIP archives (used for EPUB output).
pub struct ZipBuilder {
    writer: ZipWriter<File>,
}

impl ZipBuilder {
    /// Create a new ZIP file at the given path.
    pub fn new(path: &Path) -> io::Result<Self> {
        let file = File::create(path)?;
        Ok(Self {
            writer: ZipWriter::new(file),
        })
    }

    /// Add a file entry with the given content.
    pub fn add_file(&mut self, name: &str, content: &[u8]) -> io::Result<()> {
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        self.writer.start_file(name, options)?;
        self.writer.write_all(content)?;
        Ok(())
    }

    /// Add a file entry stored without compression (used for mimetype in EPUB).
    pub fn add_stored(&mut self, name: &str, content: &[u8]) -> io::Result<()> {
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        self.writer.start_file(name, options)?;
        self.writer.write_all(content)?;
        Ok(())
    }

    /// Add a directory entry.
    pub fn add_directory(&mut self, name: &str) -> io::Result<()> {
        let options = SimpleFileOptions::default();
        self.writer.add_directory(name, options)?;
        Ok(())
    }

    /// Finish writing the ZIP archive.
    pub fn finish(self) -> io::Result<()> {
        self.writer.finish()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zip_roundtrip() {
        let tmp = std::env::temp_dir().join("test_zip_roundtrip.zip");

        // Create
        {
            let mut builder = ZipBuilder::new(&tmp).unwrap();
            builder
                .add_stored("mimetype", b"application/epub+zip")
                .unwrap();
            builder.add_file("content.xml", b"<root/>").unwrap();
            builder.finish().unwrap();
        }

        // Read back
        let entries = list_zip_entries(&tmp).unwrap();
        assert_eq!(entries.len(), 2);
        assert!(entries.contains(&"mimetype".to_string()));

        let content = read_zip_entry(&tmp, "content.xml").unwrap();
        assert_eq!(content, b"<root/>");

        std::fs::remove_file(&tmp).ok();
    }
}
