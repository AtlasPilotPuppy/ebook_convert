//! Book document intermediate representation.
//!
//! This is the Rust equivalent of Calibre's OEBBook. All conversions pass through
//! this IR: Input Plugin → BookDocument → Transforms → Output Plugin.

use std::collections::HashMap;
use std::path::PathBuf;

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

/// The central intermediate representation for an ebook.
/// Equivalent to Python's `OEBBook`.
#[derive(Debug, Clone)]
pub struct BookDocument {
    /// Dublin Core and extended metadata
    pub metadata: Metadata,
    /// All content items (HTML, CSS, images, fonts, etc.)
    pub manifest: Manifest,
    /// Reading order of content documents
    pub spine: Spine,
    /// Hierarchical table of contents
    pub toc: Toc,
    /// Standard section references (cover, toc page, etc.)
    pub guide: Guide,
    /// Unique book identifier
    pub uid: Option<String>,
    /// OPF version (typically "2.0" or "3.0")
    pub version: String,
    /// Base directory for resolving relative paths
    pub base_path: Option<PathBuf>,
}

impl BookDocument {
    pub fn new() -> Self {
        Self {
            metadata: Metadata::new(),
            manifest: Manifest::new(),
            spine: Spine::new(),
            toc: Toc::new(),
            guide: Guide::new(),
            uid: None,
            version: "2.0".to_string(),
            base_path: None,
        }
    }
}

impl Default for BookDocument {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Metadata
// ---------------------------------------------------------------------------

/// Dublin Core metadata plus Calibre extensions.
/// Equivalent to Python's `Metadata` class.
///
/// Terms are stored as a multimap: one term can have multiple values
/// (e.g., multiple `creator` entries for co-authors).
#[derive(Debug, Clone, Default)]
pub struct Metadata {
    items: HashMap<String, Vec<MetadataItem>>,
}

impl Metadata {
    pub fn new() -> Self {
        Self {
            items: HashMap::new(),
        }
    }

    /// Add a metadata item for the given term.
    pub fn add(&mut self, term: impl Into<String>, value: impl Into<String>) {
        let item = MetadataItem {
            value: value.into(),
            attributes: HashMap::new(),
        };
        self.items.entry(term.into()).or_default().push(item);
    }

    /// Add a metadata item with attributes.
    pub fn add_with_attrs(
        &mut self,
        term: impl Into<String>,
        value: impl Into<String>,
        attrs: HashMap<String, String>,
    ) {
        let item = MetadataItem {
            value: value.into(),
            attributes: attrs,
        };
        self.items.entry(term.into()).or_default().push(item);
    }

    /// Get all values for a term.
    pub fn get(&self, term: &str) -> Option<&[MetadataItem]> {
        self.items.get(term).map(|v| v.as_slice())
    }

    /// Get the first value for a term (convenience).
    pub fn get_first(&self, term: &str) -> Option<&MetadataItem> {
        self.items.get(term).and_then(|v| v.first())
    }

    /// Get the first value as a string, or None.
    pub fn get_first_value(&self, term: &str) -> Option<&str> {
        self.get_first(term).map(|item| item.value.as_str())
    }

    /// Set a term to a single value (replacing any existing).
    pub fn set(&mut self, term: impl Into<String>, value: impl Into<String>) {
        let item = MetadataItem {
            value: value.into(),
            attributes: HashMap::new(),
        };
        self.items.insert(term.into(), vec![item]);
    }

    /// Remove all values for a term.
    pub fn remove(&mut self, term: &str) {
        self.items.remove(term);
    }

    /// Iterate over all (term, items) pairs.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &[MetadataItem])> {
        self.items
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_slice()))
    }

    /// Check if a term exists.
    pub fn contains(&self, term: &str) -> bool {
        self.items.contains_key(term)
    }

    // -- Convenience accessors for common DC terms --

    pub fn title(&self) -> Option<&str> {
        self.get_first_value("title")
    }

    pub fn set_title(&mut self, title: impl Into<String>) {
        let t = title.into();
        let trimmed = t.trim();
        if !trimmed.is_empty() {
            self.set("title", trimmed.to_string());
        }
    }

    pub fn authors(&self) -> Vec<&str> {
        self.get("creator")
            .map(|items| items.iter().map(|i| i.value.as_str()).collect())
            .unwrap_or_default()
    }

    pub fn language(&self) -> Option<&str> {
        self.get_first_value("language")
    }

    pub fn description(&self) -> Option<&str> {
        self.get_first_value("description")
    }

    pub fn publisher(&self) -> Option<&str> {
        self.get_first_value("publisher")
    }

    pub fn identifier(&self) -> Option<&str> {
        self.get_first_value("identifier")
    }

    pub fn date(&self) -> Option<&str> {
        self.get_first_value("date")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetadataItem {
    pub value: String,
    pub attributes: HashMap<String, String>,
}

// ---------------------------------------------------------------------------
// Manifest
// ---------------------------------------------------------------------------

/// Collection of all content items in the book.
/// Provides two-way indexing: by id and by href.
#[derive(Debug, Clone, Default)]
pub struct Manifest {
    items: Vec<ManifestItem>,
    /// Map from item id to index in items vec
    id_index: HashMap<String, usize>,
    /// Map from item href to index in items vec
    href_index: HashMap<String, usize>,
    /// Counter for generating unique IDs
    next_id: usize,
}

impl Manifest {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            id_index: HashMap::new(),
            href_index: HashMap::new(),
            next_id: 1,
        }
    }

    /// Add a manifest item. Returns the index.
    pub fn add(&mut self, item: ManifestItem) -> usize {
        let idx = self.items.len();
        self.id_index.insert(item.id.clone(), idx);
        self.href_index.insert(item.href.clone(), idx);
        self.items.push(item);
        idx
    }

    /// Remove an item by id. Returns the removed item if found.
    pub fn remove_by_id(&mut self, id: &str) -> Option<ManifestItem> {
        let idx = self.id_index.remove(id)?;
        let item = self.items.remove(idx);
        self.href_index.remove(&item.href);
        self.rebuild_indices();
        Some(item)
    }

    /// Get item by id.
    pub fn by_id(&self, id: &str) -> Option<&ManifestItem> {
        self.id_index.get(id).map(|&idx| &self.items[idx])
    }

    /// Get mutable item by id.
    pub fn by_id_mut(&mut self, id: &str) -> Option<&mut ManifestItem> {
        self.id_index
            .get(id)
            .copied()
            .map(move |idx| &mut self.items[idx])
    }

    /// Get item by href.
    pub fn by_href(&self, href: &str) -> Option<&ManifestItem> {
        self.href_index.get(href).map(|&idx| &self.items[idx])
    }

    /// Get mutable item by href.
    pub fn by_href_mut(&mut self, href: &str) -> Option<&mut ManifestItem> {
        self.href_index
            .get(href)
            .copied()
            .map(move |idx| &mut self.items[idx])
    }

    /// Iterate over all items.
    pub fn iter(&self) -> impl Iterator<Item = &ManifestItem> {
        self.items.iter()
    }

    /// Iterate over all items mutably.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut ManifestItem> {
        self.items.iter_mut()
    }

    /// Parallel iterate over all items.
    pub fn par_iter(&self) -> rayon::slice::Iter<'_, ManifestItem> {
        self.items.par_iter()
    }

    /// Number of items.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Generate a unique id based on a prefix.
    pub fn generate_id(&mut self, prefix: &str) -> String {
        loop {
            let id = format!("{}{}", prefix, self.next_id);
            self.next_id += 1;
            if !self.id_index.contains_key(&id) {
                return id;
            }
        }
    }

    /// Generate a unique href based on a base name.
    pub fn generate_href(&self, base: &str, ext: &str) -> String {
        let candidate = format!("{}.{}", base, ext);
        if !self.href_index.contains_key(&candidate) {
            return candidate;
        }
        for i in 1.. {
            let candidate = format!("{}_{}.{}", base, i, ext);
            if !self.href_index.contains_key(&candidate) {
                return candidate;
            }
        }
        unreachable!()
    }

    /// Get all items as a slice.
    pub fn items(&self) -> &[ManifestItem] {
        &self.items
    }

    /// Get items matching a media type prefix (e.g., "image/").
    pub fn items_of_type(&self, media_type_prefix: &str) -> Vec<&ManifestItem> {
        self.items
            .iter()
            .filter(|item| item.media_type.starts_with(media_type_prefix))
            .collect()
    }

    fn rebuild_indices(&mut self) {
        self.id_index.clear();
        self.href_index.clear();
        for (idx, item) in self.items.iter().enumerate() {
            self.id_index.insert(item.id.clone(), idx);
            self.href_index.insert(item.href.clone(), idx);
        }
    }
}

/// A single item in the manifest.
#[derive(Debug, Clone)]
pub struct ManifestItem {
    /// Unique identifier within the manifest
    pub id: String,
    /// Relative path/href within the book
    pub href: String,
    /// MIME type (e.g., "application/xhtml+xml", "image/jpeg")
    pub media_type: String,
    /// The actual content data
    pub data: ManifestData,
    /// Fallback item id for unsupported types
    pub fallback: Option<String>,
}

impl ManifestItem {
    pub fn new(
        id: impl Into<String>,
        href: impl Into<String>,
        media_type: impl Into<String>,
        data: ManifestData,
    ) -> Self {
        Self {
            id: id.into(),
            href: href.into(),
            media_type: media_type.into(),
            data,
            fallback: None,
        }
    }

    /// Is this an XHTML content document?
    pub fn is_xhtml(&self) -> bool {
        self.media_type == "application/xhtml+xml" || self.media_type == "text/html"
    }

    /// Is this a CSS stylesheet?
    pub fn is_css(&self) -> bool {
        self.media_type == "text/css"
    }

    /// Is this an image?
    pub fn is_image(&self) -> bool {
        self.media_type.starts_with("image/")
    }

    /// Is this a font?
    pub fn is_font(&self) -> bool {
        matches!(
            self.media_type.as_str(),
            "application/x-font-ttf"
                | "application/x-font-opentype"
                | "application/font-woff"
                | "application/font-woff2"
                | "font/ttf"
                | "font/otf"
                | "font/woff"
                | "font/woff2"
        )
    }
}

/// Content data for a manifest item.
/// Keeps different data types for different content kinds.
#[derive(Debug, Clone)]
pub enum ManifestData {
    /// Parsed XHTML content as a string (to be re-parsed as needed)
    Xhtml(String),
    /// CSS stylesheet source
    Css(String),
    /// Raw binary data (images, fonts, etc.)
    Binary(Vec<u8>),
    /// Content not yet loaded; stored at this path
    Lazy(PathBuf),
    /// Empty/placeholder
    Empty,
}

impl ManifestData {
    /// Get as XHTML string, if applicable.
    pub fn as_xhtml(&self) -> Option<&str> {
        match self {
            ManifestData::Xhtml(s) => Some(s),
            _ => None,
        }
    }

    /// Get as CSS string, if applicable.
    pub fn as_css(&self) -> Option<&str> {
        match self {
            ManifestData::Css(s) => Some(s),
            _ => None,
        }
    }

    /// Get as binary data, if applicable.
    pub fn as_binary(&self) -> Option<&[u8]> {
        match self {
            ManifestData::Binary(b) => Some(b),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Spine
// ---------------------------------------------------------------------------

/// Ordered reading sequence of content documents.
#[derive(Debug, Clone, Default)]
pub struct Spine {
    /// Ordered list of (manifest_item_id, linear) pairs
    items: Vec<SpineItem>,
    /// Page progression direction
    pub page_progression_direction: Option<PageDirection>,
}

#[derive(Debug, Clone)]
pub struct SpineItem {
    /// ID referencing a manifest item
    pub idref: String,
    /// Whether this item is part of the linear reading order
    pub linear: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PageDirection {
    Ltr,
    Rtl,
}

impl Spine {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            page_progression_direction: None,
        }
    }

    /// Add an item to the end of the spine.
    pub fn push(&mut self, idref: impl Into<String>, linear: bool) {
        self.items.push(SpineItem {
            idref: idref.into(),
            linear,
        });
    }

    /// Insert an item at a specific position.
    pub fn insert(&mut self, index: usize, idref: impl Into<String>, linear: bool) {
        self.items.insert(
            index,
            SpineItem {
                idref: idref.into(),
                linear,
            },
        );
    }

    /// Remove an item by idref.
    pub fn remove(&mut self, idref: &str) -> bool {
        if let Some(pos) = self.items.iter().position(|i| i.idref == idref) {
            self.items.remove(pos);
            true
        } else {
            false
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &SpineItem> {
        self.items.iter()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn items(&self) -> &[SpineItem] {
        &self.items
    }

    /// Get idrefs of linear items only.
    pub fn linear_items(&self) -> impl Iterator<Item = &str> {
        self.items
            .iter()
            .filter(|i| i.linear)
            .map(|i| i.idref.as_str())
    }
}

// ---------------------------------------------------------------------------
// Table of Contents
// ---------------------------------------------------------------------------

/// Hierarchical navigation tree.
/// Each node can have children forming a tree structure.
#[derive(Debug, Clone, Default)]
pub struct Toc {
    /// Top-level TOC entries
    pub entries: Vec<TocEntry>,
}

impl Toc {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Add a top-level entry.
    pub fn add(&mut self, entry: TocEntry) {
        self.entries.push(entry);
    }

    /// Iterate depth-first over all entries.
    pub fn iter_depth_first(&self) -> TocIter<'_> {
        TocIter {
            stack: self.entries.iter().rev().collect(),
        }
    }

    /// Rationalize play orders (assign sequential numbers).
    pub fn rationalize_play_orders(&mut self) {
        let mut order = 1;
        for entry in &mut self.entries {
            entry.rationalize_play_orders_recursive(&mut order);
        }
    }
}

#[derive(Debug, Clone)]
pub struct TocEntry {
    pub title: String,
    pub href: String,
    pub children: Vec<TocEntry>,
    pub play_order: Option<u32>,
    pub id: Option<String>,
    pub klass: Option<String>,
}

impl TocEntry {
    pub fn new(title: impl Into<String>, href: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            href: href.into(),
            children: Vec::new(),
            play_order: None,
            id: None,
            klass: None,
        }
    }

    pub fn add_child(&mut self, child: TocEntry) {
        self.children.push(child);
    }

    fn rationalize_play_orders_recursive(&mut self, order: &mut u32) {
        self.play_order = Some(*order);
        *order += 1;
        for child in &mut self.children {
            child.rationalize_play_orders_recursive(order);
        }
    }
}

/// Depth-first iterator over TOC entries.
pub struct TocIter<'a> {
    stack: Vec<&'a TocEntry>,
}

impl<'a> Iterator for TocIter<'a> {
    type Item = &'a TocEntry;

    fn next(&mut self) -> Option<Self::Item> {
        let entry = self.stack.pop()?;
        // Push children in reverse so first child is popped next
        for child in entry.children.iter().rev() {
            self.stack.push(child);
        }
        Some(entry)
    }
}

// ---------------------------------------------------------------------------
// Guide
// ---------------------------------------------------------------------------

/// Standard section references (cover, table of contents, etc.).
/// Maps reference type to a Guide reference.
#[derive(Debug, Clone, Default)]
pub struct Guide {
    refs: Vec<GuideRef>,
}

impl Guide {
    pub fn new() -> Self {
        Self { refs: Vec::new() }
    }

    pub fn add(&mut self, reference: GuideRef) {
        // Replace existing reference of same type
        self.refs.retain(|r| r.ref_type != reference.ref_type);
        self.refs.push(reference);
    }

    pub fn get(&self, ref_type: &str) -> Option<&GuideRef> {
        self.refs.iter().find(|r| r.ref_type == ref_type)
    }

    pub fn remove(&mut self, ref_type: &str) {
        self.refs.retain(|r| r.ref_type != ref_type);
    }

    pub fn iter(&self) -> impl Iterator<Item = &GuideRef> {
        self.refs.iter()
    }

    pub fn is_empty(&self) -> bool {
        self.refs.is_empty()
    }
}

#[derive(Debug, Clone)]
pub struct GuideRef {
    pub ref_type: String,
    pub title: String,
    pub href: String,
}

impl GuideRef {
    pub fn new(
        ref_type: impl Into<String>,
        title: impl Into<String>,
        href: impl Into<String>,
    ) -> Self {
        Self {
            ref_type: ref_type.into(),
            title: title.into(),
            href: href.into(),
        }
    }
}

// ---------------------------------------------------------------------------
// Format enumeration
// ---------------------------------------------------------------------------

/// Supported ebook formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EbookFormat {
    Epub,
    Pdf,
    Mobi,
    Azw,
    Azw3,
    Html,
    Xhtml,
    Txt,
    Markdown,
    Docx,
    Fb2,
    Rtf,
    Odt,
}

impl EbookFormat {
    /// Parse from file extension.
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_lowercase().as_str() {
            "epub" => Some(Self::Epub),
            "pdf" => Some(Self::Pdf),
            "mobi" | "prc" => Some(Self::Mobi),
            "azw" => Some(Self::Azw),
            "azw3" | "kf8" | "kfx" => Some(Self::Azw3),
            "html" | "htm" => Some(Self::Html),
            "xhtml" | "xhtm" => Some(Self::Xhtml),
            "txt" => Some(Self::Txt),
            "md" | "markdown" => Some(Self::Markdown),
            "docx" => Some(Self::Docx),
            "fb2" => Some(Self::Fb2),
            "rtf" => Some(Self::Rtf),
            "odt" => Some(Self::Odt),
            _ => None,
        }
    }

    /// Get the canonical file extension.
    pub fn extension(&self) -> &'static str {
        match self {
            Self::Epub => "epub",
            Self::Pdf => "pdf",
            Self::Mobi => "mobi",
            Self::Azw => "azw",
            Self::Azw3 => "azw3",
            Self::Html => "html",
            Self::Xhtml => "xhtml",
            Self::Txt => "txt",
            Self::Markdown => "md",
            Self::Docx => "docx",
            Self::Fb2 => "fb2",
            Self::Rtf => "rtf",
            Self::Odt => "odt",
        }
    }

    /// Get MIME type.
    pub fn mime_type(&self) -> &'static str {
        match self {
            Self::Epub => "application/epub+zip",
            Self::Pdf => "application/pdf",
            Self::Mobi | Self::Azw => "application/x-mobipocket-ebook",
            Self::Azw3 => "application/x-mobi8-ebook",
            Self::Html | Self::Xhtml => "text/html",
            Self::Txt | Self::Markdown => "text/plain",
            Self::Docx => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
            Self::Fb2 => "application/x-fictionbook+xml",
            Self::Rtf => "application/rtf",
            Self::Odt => "application/vnd.oasis.opendocument.text",
        }
    }
}

impl std::fmt::Display for EbookFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.extension().to_uppercase())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_book_document_creation() {
        let book = BookDocument::new();
        assert!(book.manifest.is_empty());
        assert!(book.spine.is_empty());
        assert_eq!(book.version, "2.0");
    }

    #[test]
    fn test_metadata() {
        let mut meta = Metadata::new();
        meta.set_title("Test Book");
        meta.add("creator", "Author One");
        meta.add("creator", "Author Two");

        assert_eq!(meta.title(), Some("Test Book"));
        assert_eq!(meta.authors().len(), 2);
        assert_eq!(meta.authors()[0], "Author One");
    }

    #[test]
    fn test_manifest() {
        let mut manifest = Manifest::new();
        let item = ManifestItem::new(
            "ch1",
            "chapter1.xhtml",
            "application/xhtml+xml",
            ManifestData::Xhtml("<html><body>Hello</body></html>".to_string()),
        );
        manifest.add(item);

        assert_eq!(manifest.len(), 1);
        assert!(manifest.by_id("ch1").is_some());
        assert!(manifest.by_href("chapter1.xhtml").is_some());
        assert!(manifest.by_id("ch1").unwrap().is_xhtml());
    }

    #[test]
    fn test_manifest_generate_id() {
        let mut manifest = Manifest::new();
        let id1 = manifest.generate_id("item");
        let id2 = manifest.generate_id("item");
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_spine() {
        let mut spine = Spine::new();
        spine.push("ch1", true);
        spine.push("ch2", true);
        spine.push("notes", false);

        assert_eq!(spine.len(), 3);
        assert_eq!(spine.linear_items().count(), 2);
    }

    #[test]
    fn test_toc() {
        let mut toc = Toc::new();
        let mut ch1 = TocEntry::new("Chapter 1", "ch1.xhtml");
        ch1.add_child(TocEntry::new("Section 1.1", "ch1.xhtml#s1"));
        ch1.add_child(TocEntry::new("Section 1.2", "ch1.xhtml#s2"));
        toc.add(ch1);
        toc.add(TocEntry::new("Chapter 2", "ch2.xhtml"));

        assert_eq!(toc.iter_depth_first().count(), 4);

        toc.rationalize_play_orders();
        let orders: Vec<u32> = toc
            .iter_depth_first()
            .map(|e| e.play_order.unwrap())
            .collect();
        assert_eq!(orders, vec![1, 2, 3, 4]);
    }

    #[test]
    fn test_guide() {
        let mut guide = Guide::new();
        guide.add(GuideRef::new("cover", "Cover", "cover.xhtml"));
        guide.add(GuideRef::new("toc", "Table of Contents", "toc.xhtml"));

        assert_eq!(guide.get("cover").unwrap().href, "cover.xhtml");
        guide.remove("cover");
        assert!(guide.get("cover").is_none());
    }

    #[test]
    fn test_format_parsing() {
        assert_eq!(EbookFormat::from_extension("epub"), Some(EbookFormat::Epub));
        assert_eq!(EbookFormat::from_extension("PDF"), Some(EbookFormat::Pdf));
        assert_eq!(EbookFormat::from_extension("unknown"), None);
        assert_eq!(EbookFormat::Epub.extension(), "epub");
    }
}
