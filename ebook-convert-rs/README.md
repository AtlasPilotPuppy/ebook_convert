# ebook-convert-rs

A high-performance Rust reimplementation of Calibre's `ebook-convert` tool. Converts between ebook formats using a pipeline architecture: **Input Plugin -> BookDocument IR -> Transforms -> Output Plugin**.

## Supported Formats

| Input | Output |
|-------|--------|
| PDF | EPUB |
| EPUB | HTML |
| HTML | TXT |
| TXT | PDF |
| MOBI/AZW/AZW3 | MOBI |
| DOCX | |
| FB2 | |
| RTF | |
| ODT | |
| CBZ/CBR | |
| PDB | |
| TCR | |
| LIT | |

## Installation

```bash
cargo install --path crates/ebook-convert
```

## Usage

```bash
# Simple conversion (auto-detects formats from extensions)
ebook-convert-rs input.pdf output.epub

# Using the convert subcommand
ebook-convert-rs convert input.epub output.pdf

# With options
ebook-convert-rs input.pdf output.epub \
  --pdf-engine auto \
  --pdf-dpi 300 \
  --max-image-size 1600
```

### PDF-Specific Options

| Flag | Default | Description |
|------|---------|-------------|
| `--pdf-engine` | `auto` | Extraction engine: `auto`, `text-only`, `image-only` |
| `--pdf-dpi` | `300` | DPI for image-based extraction |

### General Options

| Flag | Default | Description |
|------|---------|-------------|
| `--max-image-size` | `1600` | Max image dimension in pixels |
| `--embed-font-family` | - | Font family to embed |
| `--unsmarten-punctuation` | `false` | Convert smart quotes to ASCII |
| `--linearize-tables` | `false` | Convert tables to divs |
| `--insert-metadata` | `false` | Insert metadata jacket page |
| `--debug-pipeline` | - | Dump intermediate IR to directory |

## Architecture

```
ebook-convert-rs/
├── crates/
│   ├── convert-core/          # BookDocument IR, plugin traits, pipeline
│   ├── convert-utils/         # XML, CSS, image utilities
│   ├── convert-input-pdf/     # Hybrid pdftohtml + pdftoppm extraction
│   ├── convert-input-epub/    # EPUB 2/3 reader
│   ├── convert-input-html/    # HTML to XHTML
│   ├── convert-input-txt/     # Plain text with paragraph detection
│   ├── convert-input-mobi/    # MOBI/AZW/AZW3 reader
│   ├── convert-input-docx/    # OOXML to XHTML
│   ├── convert-input-fb2/     # FictionBook2 reader
│   ├── convert-input-rtf/     # Rich Text Format reader
│   ├── convert-input-odt/     # OpenDocument Text reader
│   ├── convert-output-epub/   # EPUB 3 writer
│   ├── convert-output-html/   # Single-file HTML writer
│   ├── convert-output-txt/    # Plain text writer
│   ├── convert-output-pdf/    # PDF writer (printpdf)
│   ├── convert-output-mobi/   # MOBI6/PDB writer
│   ├── convert-transforms/    # 12 Calibre-compatible transforms
│   └── ebook-convert/         # CLI binary
```

### Pipeline

The conversion pipeline mirrors Calibre's `Plumber.run()`:

1. **Input** (0-34%): Input plugin parses source format into `BookDocument`
2. **Transforms** (34-90%): Sequential transforms in Calibre's exact order:
   - DataURL resolver
   - Guide cleanup
   - Metadata merge
   - Structure detection (headings, TOC)
   - Jacket page insertion
   - Chapter splitting
   - Table linearization
   - Punctuation unsmarting
   - CSS flattening
   - Page margin cleanup
   - Image rescaling
   - Manifest trimming
3. **Output** (90-100%): Output plugin serializes to target format

### BookDocument IR

The intermediate representation (`BookDocument`) is equivalent to Calibre's `OEBBook`:

- **Metadata**: Dublin Core fields (title, author, language, etc.)
- **Manifest**: All resources (XHTML, images, CSS, fonts)
- **Spine**: Reading order of XHTML documents
- **TOC**: Hierarchical table of contents
- **Guide**: Semantic references (cover, titlepage, toc)

## Building

```bash
cd ebook-convert-rs
cargo build --release
```

### External Dependencies

The PDF input plugin requires poppler-utils for extraction:

```bash
# macOS
brew install poppler

# Ubuntu/Debian
sudo apt install poppler-utils
```

## Testing

```bash
cargo test              # all tests
cargo test -p convert-core      # single crate
cargo clippy --all-targets      # lint
```

163 tests across 18 crates covering all input/output plugins and transforms.

## License

MIT
