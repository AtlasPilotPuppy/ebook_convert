# ebook-convert-rs Project Guide

## Project Overview
Rust reimplementation of Calibre's `ebook-convert` tool. Converts between ebook formats (PDF, EPUB, MOBI, HTML, TXT, DOCX) using a pipeline: **Input Plugin → BookDocument IR → Transforms → Output Plugin**.

## Architecture
- **Cargo workspace** at `ebook-convert-rs/` with crates under `crates/`
- **Core IR**: `BookDocument` in `convert-core` — equivalent to Python's `OEBBook`
- **Plugin traits**: `InputPlugin`, `OutputPlugin`, `Transform` in `convert-core`
- **Pipeline**: Orchestrator runs transforms in Calibre's exact order
- **Parallelism**: rayon (data parallelism), tokio (async I/O), SIMD (image processing)

## Key Conventions
- Use `thiserror` for error types, `anyhow` in binary crate only
- All public APIs get `/// doc comments`
- Tests go in `#[cfg(test)] mod tests` within each file
- Integration tests in `tests/integration/`
- Run `cargo clippy` and `cargo fmt` before committing

## Reference Files (Python)
- `ebook-convert/src/calibre/ebooks/conversion/plumber.py` — pipeline orchestration
- `ebook-convert/src/calibre/ebooks/oeb/base.py` — IR data model (OEBBook)
- `ebook-convert/src/calibre/customize/conversion.py` — plugin base classes

## Build & Test
```bash
cd ebook-convert-rs
cargo build                    # build all crates
cargo test                     # run all tests
cargo test -p convert-core     # test single crate
cargo clippy --all-targets     # lint
cargo bench                    # benchmarks (when available)
```

## Current Phase
Phase 1: Foundation + PDF→EPUB MVP
