# ebook-convert-rs

A high-performance Rust reimplementation of [Calibre's](https://calibre-ebook.com/) `ebook-convert` command-line tool. Converts between ebook formats using a three-phase pipeline architecture with parallel processing via [rayon](https://github.com/rayon-rs/rayon).

Mirrored from - https://git.asthana.site/atlas/ebook_convert

## Supported Formats

| Input Formats | Output Formats |
|---------------|----------------|
| PDF | EPUB |
| EPUB | PDF |
| HTML/XHTML | HTML |
| TXT/Markdown | TXT |
| MOBI/AZW/AZW3 | MOBI |
| DOCX | |
| FB2 | |
| RTF | |
| ODT | |

Any input format can be converted to any output format. The tool normalizes all inputs into a common intermediate representation (BookDocument IR) before serializing to the target format.

## Installation

### From source

```bash
cd ebook-convert-rs
cargo install --path crates/ebook-convert
```

### External dependencies

The PDF input plugin requires **poppler-utils** (`pdftohtml` and `pdftoppm`) for text extraction and page rendering:

```bash
# macOS
brew install poppler

# Ubuntu/Debian
sudo apt install poppler-utils

# Fedora
sudo dnf install poppler-utils

# Arch
sudo pacman -S poppler
```

All other input/output formats use pure Rust implementations with no external dependencies.

## Usage

The CLI supports two modes: a **legacy positional** interface compatible with Calibre's `ebook-convert`, and a **modern subcommand** interface.

### Legacy mode

```bash
ebook-convert-rs input.pdf output.epub [options]
```

### Modern mode

```bash
ebook-convert-rs convert input.pdf -o output.epub [options]
```

Format detection is automatic from file extensions. Override with `--from` / `--to`:

```bash
ebook-convert-rs convert input.dat -o output.dat --from pdf --to epub
```

### Examples

```bash
# PDF to EPUB with image-only extraction at 300 DPI
ebook-convert-rs input.pdf output.epub --pdf-engine image-only --pdf-dpi 300

# EPUB to MOBI
ebook-convert-rs novel.epub novel.mobi

# DOCX to EPUB with custom CSS and image size limit
ebook-convert-rs report.docx report.epub --extra-css "body { font-size: 14px; }" --max-image-size 800x1200

# HTML to TXT
ebook-convert-rs page.html page.txt

# Debug the conversion pipeline (dumps IR at each stage)
ebook-convert-rs input.pdf output.epub --debug-pipeline /tmp/debug/
```

## CLI Options

### General

| Flag | Default | Description |
|------|---------|-------------|
| `-v`, `--verbose` | 0 | Increase verbosity (repeat for more: `-vv`, `-vvv`) |
| `--extra-css <CSS>` | - | Extra CSS stylesheet to inject into the document |
| `--max-image-size <WxH>` | profile default | Maximum image dimensions in pixels (e.g. `800x1200`) |
| `--jpeg-quality <1-100>` | 80 | JPEG quality for transcoded images (including JP2 to JPEG) |
| `--debug-pipeline <DIR>` | - | Dump intermediate BookDocument IR to this directory |
| `--dump-config` | - | Print effective merged config as TOML and exit |

### PDF Input

| Flag | Default | Description |
|------|---------|-------------|
| `--pdf-engine <MODE>` | `auto` | Extraction strategy (see below) |
| `--pdf-dpi <N>` | 200 | Rendering DPI for image-based page extraction |

**PDF engine modes:**

- **`auto`** (default): Hybrid extraction. Uses `pdftohtml -xml` for text-based pages and automatically falls back to `pdftoppm` for scanned/image-only pages. Each page is independently classified as Text, Scanned (GlyphLessFont), ImageOnly, or Blank.
- **`text-only`**: Uses `pdftohtml` exclusively. Scanned pages will have no content.
- **`image-only`**: Renders every page as a JPEG via `pdftoppm`. Best for scanned PDFs or PDFs with complex layouts that `pdftohtml` mishandles.

## Configuration

Persistent defaults can be set via TOML config files, avoiding the need to pass the same flags on every invocation. Config files are loaded in order, with later sources overriding earlier ones:

1. **Global config:** `~/.config/ebook-convert-rs/config.toml`
2. **Project-local config:** `./.ebook-convert-rs.toml` (in the current directory)
3. **CLI flags** (always win)

Any field from `ConversionOptions` can be set in the config file. Missing fields use built-in defaults.

### Example config

```toml
verbose = 1
jpeg_quality = 90
pdf_engine = "auto"
pdf_dpi = 300
extra_css = "body { font-size: 14px; }"
unsmarten_punctuation = true
max_image_size = "1200x1600"
margin_top = 10.0
epub_version = "2"
```

### Inspecting effective config

Use `--dump-config` to print the fully merged configuration (defaults + config files + CLI flags) as TOML:

```bash
ebook-convert-rs --dump-config
ebook-convert-rs --dump-config --pdf-dpi 300   # see how CLI flags override
```

### Config field reference

All CLI flags map to config fields using snake_case. Enum values use kebab-case strings:

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `verbose` | integer | `0` | Verbosity level |
| `extra_css` | string | - | Extra CSS to inject |
| `max_image_size` | string | - | Max image size as `"WxH"` (e.g. `"1200x1600"`) |
| `jpeg_quality` | integer | `80` | JPEG quality (1-100) |
| `pdf_engine` | string | `"auto"` | `"auto"`, `"image-only"`, or `"text-only"` |
| `pdf_dpi` | integer | `200` | PDF rendering DPI |
| `chapter_mark` | string | `"page-break"` | `"page-break"`, `"rule"`, `"both"`, `"none"` |
| `epub_version` | string | `"2"` | `"2"` or `"3"` |
| `unsmarten_punctuation` | boolean | `false` | Convert smart quotes to ASCII |
| `linearize_tables` | boolean | `false` | Convert tables to stacked divs |
| `insert_metadata` | boolean | `false` | Insert metadata jacket page |
| `margin_top` | float | `5.0` | Top margin |
| `margin_bottom` | float | `5.0` | Bottom margin |
| `margin_left` | float | `5.0` | Left margin |
| `margin_right` | float | `5.0` | Right margin |
| `pretty_print` | boolean | `false` | Pretty-print output XML |

## Architecture

```
ebook-convert-rs/
├── crates/
│   ├── convert-core/          # BookDocument IR, plugin traits, pipeline orchestrator
│   ├── convert-utils/         # XML builder, CSS utilities, image helpers
│   ├── convert-input-pdf/     # Hybrid pdftohtml + pdftoppm extraction
│   ├── convert-input-epub/    # EPUB 2/3 reader
│   ├── convert-input-html/    # HTML/XHTML normalizer
│   ├── convert-input-txt/     # Plain text with paragraph detection
│   ├── convert-input-mobi/    # MOBI/AZW/AZW3 reader (via mobi crate)
│   ├── convert-input-docx/    # OOXML to XHTML
│   ├── convert-input-fb2/     # FictionBook2 reader
│   ├── convert-input-rtf/     # Rich Text Format reader
│   ├── convert-input-odt/     # OpenDocument Text reader
│   ├── convert-output-epub/   # EPUB 2 writer with OPF/NCX generation
│   ├── convert-output-pdf/    # PDF writer (printpdf, Helvetica, word-wrapped)
│   ├── convert-output-html/   # Single-file HTML writer
│   ├── convert-output-txt/    # Plain text writer
│   ├── convert-output-mobi/   # MOBI/PalmDOC writer
│   ├── convert-transforms/    # 12 Calibre-compatible transforms
│   └── ebook-convert/         # CLI binary (clap)
```

### Conversion Pipeline

The pipeline mirrors Calibre's `Plumber.run()` from `plumber.py` and runs in three phases:

```
┌─────────────────┐     ┌─────────────────────────────────────────────┐     ┌──────────────────┐
│   Input Plugin   │────>│           Transform Pipeline                │────>│  Output Plugin    │
│   (0% – 34%)    │     │           (34% – 90%)                       │     │  (90% – 100%)    │
│                 │     │                                             │     │                  │
│ PDF/EPUB/HTML/  │     │  1. DataURL         7. UnsmartenPunctuation │     │ EPUB/PDF/HTML/   │
│ TXT/MOBI/DOCX/  │     │  2. CleanGuide      8. CSSFlattener        │     │ TXT/MOBI         │
│ FB2/RTF/ODT     │     │  3. MergeMetadata   9. PageMargin          │     │                  │
│        │        │     │  4. DetectStructure 10. ImageRescale        │     │                  │
│        v        │     │  5. Jacket          11. SplitChapters       │     │                  │
│   BookDocument   │     │  6. LinearizeTables 12. ManifestTrimmer    │     │                  │
└─────────────────┘     └─────────────────────────────────────────────┘     └──────────────────┘
```

**Phase 1 — Input (0–34%):** The input plugin parses the source file into a `BookDocument`, then runs postprocessing and output-format specialization hooks.

**Phase 2 — Transforms (34–90%):** Twelve transforms run sequentially in Calibre's exact order. Each transform internally parallelizes its work using rayon. Conditional transforms (Jacket, LinearizeTables, UnsmartenPunctuation) check `should_run()` and skip when their corresponding option is disabled.

**Phase 3 — Output (90–100%):** The output plugin serializes the `BookDocument` into the target format.

### BookDocument IR

The intermediate representation is equivalent to Calibre's `OEBBook` (`oeb/base.py`):

| Component | Description |
|-----------|-------------|
| **Metadata** | Dublin Core fields: title, authors, language, description, publisher, date, identifiers |
| **Manifest** | All resources — XHTML content, CSS stylesheets, images (binary or lazy-loaded), fonts |
| **Spine** | Ordered list of XHTML documents defining reading order |
| **TOC** | Hierarchical table of contents (nested `TocEntry` tree) |
| **Guide** | Semantic references: cover, title page, table of contents |

Manifest items carry their data inline (`ManifestData::Xhtml`, `ManifestData::Css`, `ManifestData::Binary`) or as lazy file references (`ManifestData::Lazy`) for large assets resolved at output time.

### Transform Details

| # | Transform | Condition | Description |
|---|-----------|-----------|-------------|
| 1 | **DataURL** | always | Extracts inline `data:` URIs from XHTML, decodes base64 content, and adds them as separate manifest items |
| 2 | **CleanGuide** | always | Removes invalid guide references that don't point to manifest items |
| 3 | **MergeMetadata** | always | Consolidates metadata from multiple sources into canonical Dublin Core fields |
| 4 | **DetectStructure** | always | Identifies chapter headings (`<h1>`–`<h6>`) across spine documents and builds the TOC |
| 5 | **Jacket** | conditional | Inserts a metadata "jacket" page (title, author, description) and optionally removes the first image |
| 6 | **LinearizeTables** | conditional | Converts HTML tables to stacked `<div>` elements for better e-reader reflow |
| 7 | **UnsmartenPunctuation** | conditional | Converts smart quotes, em/en dashes, and ellipses back to ASCII equivalents |
| 8 | **CSSFlattener** | always | Inlines CSS styles, resolves `@import`, computes font sizes relative to the base |
| 9 | **PageMargin** | always | Detects and removes the most common inline page margins for consistent layout |
| 10 | **ImageRescale** | always | Resizes images exceeding `max_image_size`, transcodes formats (e.g. JP2 to JPEG) |
| 11 | **SplitChapters** | always | Splits large XHTML documents (>10KB) at `<h1>`/`<h2>` or page-break boundaries into separate files |
| 12 | **ManifestTrimmer** | always | Removes unreferenced manifest items (images, CSS, fonts not linked from any XHTML) |

### PDF Hybrid Extraction

The PDF input plugin uses a hybrid approach for maximum quality:

1. **XML extraction** — Runs `pdftohtml -xml` to get text positions, font metadata, and embedded images
2. **Page classification** — Each page is independently classified:
   - **Text**: Contains real text with meaningful fonts
   - **Scanned**: Contains only GlyphLessFont (OCR placeholder) — falls back to image rendering
   - **ImageOnly**: No text elements, only images
   - **Blank**: No content at all
3. **Image fallback** — Scanned and image-only pages are rendered via `pdftoppm` at the configured DPI
4. **XHTML assembly** — Text pages are converted to semantic XHTML; image pages become `<img>` references

This produces significantly better output than either pure text extraction or pure image rendering alone.

## Design Decisions

### Why rewrite Calibre's ebook-convert in Rust?

Calibre's converter is mature but has limitations: it's single-threaded Python, requires a full Calibre installation, and pulls in Qt/GUI dependencies even for CLI use. This reimplementation provides a standalone binary with parallel processing while maintaining Calibre-compatible transform ordering and behavior.

### Plugin architecture

Input plugins, output plugins, and transforms are defined as traits (`InputPlugin`, `OutputPlugin`, `Transform`) in `convert-core`. This makes it straightforward to add new formats without modifying the pipeline. Each plugin is a separate crate with its own dependencies, keeping compile times manageable and allowing unused formats to be excluded.

### Calibre compatibility

The transform pipeline runs in Calibre's exact order because some transforms depend on prior transforms' output (e.g., `DetectStructure` must run before `SplitChapters`; `CSSFlattener` must run before `PageMargin`). The `BookDocument` IR maps directly to Calibre's `OEBBook`, making it easier to port transforms and verify correctness.

### Parallelism strategy

Rather than parallelizing at the pipeline level (which would break transform ordering), each transform and plugin parallelizes its own internal work using rayon's `par_iter()`. The pattern is:

1. **Collect** — Gather work items from the BookDocument (read-only)
2. **Process** — Transform items in parallel via `into_par_iter()`
3. **Apply** — Write results back sequentially

This gives safe parallelism without requiring `Arc<Mutex<>>` on the BookDocument. Key parallel sites include: PDF page classification, XHTML content processing in all transforms, image I/O, and text extraction in output plugins.

### Image handling

All image processing goes through the `image` crate (0.25), which supports JPEG, PNG, GIF, WebP, TIFF, BMP, and ICO natively. JPEG 2000 (JP2) images embedded in PDFs are handled by the external `pdftohtml`/`pdftoppm` tools, which decode JP2 and emit standard formats. The `ImageRescale` transform then resizes and transcodes as needed for the target output format.

## Building

```bash
cd ebook-convert-rs
cargo build --release
```

The release binary is at `target/release/ebook-convert-rs`.

## Testing

```bash
cargo test --lib --bins     # all unit tests (~170 tests across 18 crates)
cargo test -p convert-core  # single crate
cargo clippy --lib --bins --tests  # lint
```

### Test coverage by area

| Crate | Tests | Coverage |
|-------|-------|----------|
| convert-core | 17 | IR, metadata, manifest, spine, pipeline, config serde |
| convert-utils | 12 | XML escaping, CSS parsing, image helpers |
| convert-input-pdf | 36 | Page classification, XHTML building, hybrid extraction |
| convert-input-epub | 6 | EPUB 2/3 parsing, manifest/spine extraction |
| convert-input-html | 1 | HTML normalization |
| convert-input-txt | 2 | Paragraph detection, encoding handling |
| convert-input-mobi | 7 | MOBI record parsing, HTML extraction |
| convert-input-docx | 13 | OOXML elements, styles, images, tables |
| convert-output-epub | 4 | OPF/NCX generation, ZIP assembly |
| convert-output-html | 2 | Single-file HTML output |
| convert-output-txt | 1 | Text extraction |
| convert-transforms | 41 | All 12 transforms with edge cases |

## Key Dependencies

| Crate | Version | Purpose |
|-------|---------|---------|
| rayon | 1.11 | Data parallelism across transforms and plugins |
| clap | 4 | CLI argument parsing with derive macros |
| html5ever | 0.38 | HTML5-compliant parsing |
| lightningcss | 1.0.0-alpha.70 | CSS parsing and transformation |
| image | 0.25 | Image decoding, resizing, and format conversion |
| fast_image_resize | 6 | High-performance image downscaling |
| zip | 8 | EPUB ZIP container assembly |
| lopdf | 0.39 | PDF structure inspection |
| printpdf | 0.8 | PDF generation for output |
| mobi | 0.8 | MOBI/AZW format parsing |
| quick-xml | 0.37 | XML parsing for DOCX/FB2/ODT |
| toml | 0.8 | Config file parsing (TOML format) |
| dirs | 6 | Platform-specific config directory paths |

## License

GPL-3.0
