//! Benchmarks for ebook conversion transforms.

use criterion::{black_box, criterion_group, criterion_main, Criterion};

use convert_core::book::{BookDocument, ManifestData, ManifestItem, TocEntry};
use convert_core::options::ConversionOptions;
use convert_core::plugin::Transform;
use convert_transforms::css_flattener::CssFlattener;
use convert_transforms::data_url::DataUrl;
use convert_transforms::detect_structure::DetectStructure;
use convert_transforms::image_rescale::ImageRescale;
use convert_transforms::jacket::Jacket;
use convert_transforms::linearize_tables::LinearizeTables;
use convert_transforms::manifest_trimmer::ManifestTrimmer;
use convert_transforms::merge_metadata::MergeMetadata;
use convert_transforms::page_margin::PageMargin;
use convert_transforms::split_chapters::SplitChapters;
use convert_transforms::unsmarten::UnsmartenPunctuation;

/// Create a realistic BookDocument with N chapters and images.
fn make_book(num_chapters: usize, num_images: usize) -> BookDocument {
    let mut book = BookDocument::new();
    book.metadata.set_title("Benchmark Book");
    book.metadata.add("creator", "Bench Author");
    book.metadata.set("language", "en");

    // Add CSS
    let css = "body { margin: 1em; font-family: serif; line-height: 1.6; }\n\
               p { margin: 0.5em 0; text-indent: 1em; }\n\
               h1, h2, h3 { font-family: sans-serif; }\n\
               .chapter { page-break-before: always; }\n\
               img { max-width: 100%; height: auto; }";
    book.manifest.add(ManifestItem::new(
        "style",
        "style.css",
        "text/css",
        ManifestData::Css(css.to_string()),
    ));

    // Add chapters
    for i in 0..num_chapters {
        let id = format!("ch{}", i);
        let href = format!("chapter{}.xhtml", i);
        let body = format!(
            "<h1>Chapter {}</h1>\n\
             <p>This is paragraph one of chapter {}. It contains some text to simulate a real ebook.</p>\n\
             <p>This is paragraph two with <strong>bold</strong> and <em>italic</em> text.</p>\n\
             <p>Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua.</p>",
            i, i
        );
        let xhtml = convert_utils::xml::xhtml11_document(
            &format!("Chapter {}", i),
            "en",
            Some("style.css"),
            &body,
        );
        book.manifest.add(ManifestItem::new(
            &id,
            &href,
            "application/xhtml+xml",
            ManifestData::Xhtml(xhtml),
        ));
        book.spine.push(&id, true);
        book.toc.add(TocEntry::new(format!("Chapter {}", i), &href));
    }

    // Add images (small 10x10 PNGs)
    for i in 0..num_images {
        let id = format!("img{}", i);
        let href = format!("images/img{}.png", i);
        let mut img = image::RgbaImage::new(10, 10);
        for pixel in img.pixels_mut() {
            *pixel = image::Rgba([
                (i * 37 % 256) as u8,
                (i * 73 % 256) as u8,
                (i * 119 % 256) as u8,
                255,
            ]);
        }
        let mut buf = Vec::new();
        image::DynamicImage::ImageRgba8(img)
            .write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png)
            .unwrap();
        book.manifest.add(ManifestItem::new(
            &id,
            &href,
            "image/png",
            ManifestData::Binary(buf),
        ));
    }

    book
}

/// Create a book with large chapters suitable for SplitChapters benchmarking.
fn make_large_chapter_book(num_chapters: usize) -> BookDocument {
    let mut book = BookDocument::new();
    book.metadata.set_title("Large Chapter Book");
    book.metadata.add("creator", "Bench Author");

    for i in 0..num_chapters {
        let id = format!("ch{}", i);
        let href = format!("chapter{}.xhtml", i);
        // Generate >10KB of content per chapter with multiple headings
        let mut body = format!("<h1>Chapter {}</h1>\n", i);
        for j in 0..20 {
            body.push_str(&format!(
                "<h2>Section {}.{}</h2>\n\
                 <p>Lorem ipsum dolor sit amet, consectetur adipiscing elit. \
                 Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. \
                 Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris \
                 nisi ut aliquip ex ea commodo consequat. Duis aute irure dolor in \
                 reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla \
                 pariatur.</p>\n",
                i, j
            ));
        }
        let xhtml =
            convert_utils::xml::xhtml11_document(&format!("Chapter {}", i), "en", None, &body);
        book.manifest.add(ManifestItem::new(
            &id,
            &href,
            "application/xhtml+xml",
            ManifestData::Xhtml(xhtml),
        ));
        book.spine.push(&id, true);
        book.toc.add(TocEntry::new(format!("Chapter {}", i), &href));
    }

    book
}

/// Create a book with data: URIs embedded in XHTML.
fn make_book_with_data_urls(num_chapters: usize) -> BookDocument {
    let mut book = BookDocument::new();
    book.metadata.set_title("Data URL Book");

    // Small 1x1 PNG as base64
    let pixel_png = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";

    for i in 0..num_chapters {
        let id = format!("ch{}", i);
        let href = format!("chapter{}.xhtml", i);
        let body = format!(
            "<h1>Chapter {}</h1>\n\
             <p>Text with embedded image:</p>\n\
             <img src=\"data:image/png;base64,{}\" />\n\
             <p>More text after the image.</p>\n\
             <img src=\"data:image/png;base64,{}\" />",
            i, pixel_png, pixel_png
        );
        let xhtml =
            convert_utils::xml::xhtml11_document(&format!("Chapter {}", i), "en", None, &body);
        book.manifest.add(ManifestItem::new(
            &id,
            &href,
            "application/xhtml+xml",
            ManifestData::Xhtml(xhtml),
        ));
        book.spine.push(&id, true);
    }

    book
}

/// Create a book with smart (curly) quotes for UnsmartenPunctuation.
fn make_book_with_smart_quotes(num_chapters: usize) -> BookDocument {
    let mut book = BookDocument::new();
    book.metadata.set_title("Smart Quotes Book");

    for i in 0..num_chapters {
        let id = format!("ch{}", i);
        let href = format!("chapter{}.xhtml", i);
        let body = format!(
            "<h1>Chapter {}</h1>\n\
             <p>\u{201c}Hello,\u{201d} she said. \u{201c}It\u{2019}s a beautiful day.\u{201d}</p>\n\
             <p>He replied, \u{201c}Indeed it is!\u{201d} \u{2014} and they walked on.</p>\n\
             <p>The \u{2018}quick\u{2019} brown fox\u{2026} jumped over the lazy dog.</p>\n\
             <p>\u{201c}Shall we go?\u{201d} \u{2014} \u{201c}Yes, let\u{2019}s!\u{201d}</p>",
            i
        );
        let xhtml =
            convert_utils::xml::xhtml11_document(&format!("Chapter {}", i), "en", None, &body);
        book.manifest.add(ManifestItem::new(
            &id,
            &href,
            "application/xhtml+xml",
            ManifestData::Xhtml(xhtml),
        ));
        book.spine.push(&id, true);
    }

    book
}

/// Create a book with HTML tables for LinearizeTables.
fn make_book_with_tables(num_chapters: usize) -> BookDocument {
    let mut book = BookDocument::new();
    book.metadata.set_title("Table Book");

    for i in 0..num_chapters {
        let id = format!("ch{}", i);
        let href = format!("chapter{}.xhtml", i);
        let body = format!(
            "<h1>Chapter {}</h1>\n\
             <table>\n\
               <tr><th>Name</th><th>Value</th><th>Notes</th></tr>\n\
               <tr><td>Alpha</td><td>100</td><td>First item</td></tr>\n\
               <tr><td>Beta</td><td>200</td><td>Second item</td></tr>\n\
               <tr><td>Gamma</td><td>300</td><td>Third item</td></tr>\n\
             </table>\n\
             <p>Some text between tables.</p>\n\
             <table>\n\
               <tr><td>Row 1</td><td>Data 1</td></tr>\n\
               <tr><td>Row 2</td><td>Data 2</td></tr>\n\
             </table>",
            i
        );
        let xhtml =
            convert_utils::xml::xhtml11_document(&format!("Chapter {}", i), "en", None, &body);
        book.manifest.add(ManifestItem::new(
            &id,
            &href,
            "application/xhtml+xml",
            ManifestData::Xhtml(xhtml),
        ));
        book.spine.push(&id, true);
    }

    book
}

/// Create a book with inline margin styles for PageMargin.
fn make_book_with_margins(num_chapters: usize) -> BookDocument {
    let mut book = BookDocument::new();
    book.metadata.set_title("Margin Book");

    let css = "body { margin: 2em; } p { margin: 1em 0; }";
    book.manifest.add(ManifestItem::new(
        "style",
        "style.css",
        "text/css",
        ManifestData::Css(css.to_string()),
    ));

    for i in 0..num_chapters {
        let id = format!("ch{}", i);
        let href = format!("chapter{}.xhtml", i);
        let body = format!(
            "<h1>Chapter {}</h1>\n\
             <p style=\"margin-left: 2em;\">Indented paragraph one.</p>\n\
             <p style=\"margin: 1em 3em;\">Custom margins paragraph.</p>\n\
             <div style=\"padding: 1em; margin: 0.5em;\">A styled div.</div>\n\
             <p>Normal paragraph without inline styles.</p>",
            i
        );
        let xhtml = convert_utils::xml::xhtml11_document(
            &format!("Chapter {}", i),
            "en",
            Some("style.css"),
            &body,
        );
        book.manifest.add(ManifestItem::new(
            &id,
            &href,
            "application/xhtml+xml",
            ManifestData::Xhtml(xhtml),
        ));
        book.spine.push(&id, true);
    }

    book
}

fn bench_merge_metadata(c: &mut Criterion) {
    let mut group = c.benchmark_group("MergeMetadata");

    group.bench_function("10_chapters", |b| {
        b.iter(|| {
            let mut book = make_book(10, 0);
            let opts = ConversionOptions::default();
            MergeMetadata.apply(black_box(&mut book), &opts).unwrap();
        })
    });

    group.finish();
}

fn bench_detect_structure(c: &mut Criterion) {
    let mut group = c.benchmark_group("DetectStructure");

    for n in [10, 50, 100] {
        group.bench_function(format!("{}_chapters", n), |b| {
            b.iter(|| {
                let mut book = make_book(n, 0);
                book.toc.entries.clear(); // Force structure detection
                let opts = ConversionOptions::default();
                DetectStructure.apply(black_box(&mut book), &opts).unwrap();
            })
        });
    }

    group.finish();
}

fn bench_css_flattener(c: &mut Criterion) {
    let mut group = c.benchmark_group("CSSFlattener");

    for n in [10, 50, 100] {
        group.bench_function(format!("{}_chapters", n), |b| {
            b.iter(|| {
                let mut book = make_book(n, 0);
                let opts = ConversionOptions::default();
                CssFlattener.apply(black_box(&mut book), &opts).unwrap();
            })
        });
    }

    group.finish();
}

fn bench_image_rescale(c: &mut Criterion) {
    let mut group = c.benchmark_group("ImageRescale");
    group.sample_size(10);

    let make_book_with_large_images = |num_images: usize, size: u32| -> BookDocument {
        let mut book = make_book(1, 0);
        for i in 0..num_images {
            let id = format!("img{}", i);
            let href = format!("images/img{}.png", i);
            let img = image::RgbaImage::new(size, size);
            let mut buf = Vec::new();
            image::DynamicImage::ImageRgba8(img)
                .write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png)
                .unwrap();
            book.manifest.add(ManifestItem::new(
                &id,
                &href,
                "image/png",
                ManifestData::Binary(buf),
            ));
        }
        book
    };

    group.bench_function("5_images_200x200", |b| {
        b.iter(|| {
            let mut book = make_book_with_large_images(5, 200);
            let mut opts = ConversionOptions::default();
            opts.max_image_size = Some((100, 100));
            ImageRescale.apply(black_box(&mut book), &opts).unwrap();
        })
    });

    group.bench_function("20_images_200x200", |b| {
        b.iter(|| {
            let mut book = make_book_with_large_images(20, 200);
            let mut opts = ConversionOptions::default();
            opts.max_image_size = Some((100, 100));
            ImageRescale.apply(black_box(&mut book), &opts).unwrap();
        })
    });

    group.bench_function("50_images_1000x1000", |b| {
        b.iter(|| {
            let mut book = make_book_with_large_images(50, 1000);
            let mut opts = ConversionOptions::default();
            opts.max_image_size = Some((600, 800));
            ImageRescale.apply(black_box(&mut book), &opts).unwrap();
        })
    });

    group.finish();
}

fn bench_manifest_trimmer(c: &mut Criterion) {
    let mut group = c.benchmark_group("ManifestTrimmer");

    group.bench_function("100_chapters_50_unreferenced", |b| {
        b.iter(|| {
            let mut book = make_book(100, 50);
            let opts = ConversionOptions::default();
            ManifestTrimmer.apply(black_box(&mut book), &opts).unwrap();
        })
    });

    group.finish();
}

fn bench_split_chapters(c: &mut Criterion) {
    let mut group = c.benchmark_group("SplitChapters");

    for n in [10, 50, 100] {
        group.bench_function(format!("{}_large_chapters", n), |b| {
            b.iter(|| {
                let mut book = make_large_chapter_book(n);
                let opts = ConversionOptions::default();
                SplitChapters.apply(black_box(&mut book), &opts).unwrap();
            })
        });
    }

    group.finish();
}

fn bench_data_url(c: &mut Criterion) {
    let mut group = c.benchmark_group("DataURL");

    for n in [10, 50] {
        group.bench_function(format!("{}_chapters", n), |b| {
            b.iter(|| {
                let mut book = make_book_with_data_urls(n);
                let opts = ConversionOptions::default();
                DataUrl.apply(black_box(&mut book), &opts).unwrap();
            })
        });
    }

    group.finish();
}

fn bench_unsmarten(c: &mut Criterion) {
    let mut group = c.benchmark_group("UnsmartenPunctuation");

    for n in [50, 200] {
        group.bench_function(format!("{}_chapters", n), |b| {
            b.iter(|| {
                let mut book = make_book_with_smart_quotes(n);
                let mut opts = ConversionOptions::default();
                opts.unsmarten_punctuation = true;
                UnsmartenPunctuation
                    .apply(black_box(&mut book), &opts)
                    .unwrap();
            })
        });
    }

    group.finish();
}

fn bench_linearize_tables(c: &mut Criterion) {
    let mut group = c.benchmark_group("LinearizeTables");

    for n in [10, 50] {
        group.bench_function(format!("{}_chapters", n), |b| {
            b.iter(|| {
                let mut book = make_book_with_tables(n);
                let mut opts = ConversionOptions::default();
                opts.linearize_tables = true;
                LinearizeTables.apply(black_box(&mut book), &opts).unwrap();
            })
        });
    }

    group.finish();
}

fn bench_page_margin(c: &mut Criterion) {
    let mut group = c.benchmark_group("PageMargin");

    for n in [50, 100] {
        group.bench_function(format!("{}_chapters", n), |b| {
            b.iter(|| {
                let mut book = make_book_with_margins(n);
                let opts = ConversionOptions::default();
                PageMargin.apply(black_box(&mut book), &opts).unwrap();
            })
        });
    }

    group.finish();
}

fn bench_jacket(c: &mut Criterion) {
    let mut group = c.benchmark_group("Jacket");

    for n in [10, 50] {
        group.bench_function(format!("{}_chapters_metadata", n), |b| {
            b.iter(|| {
                let mut book = make_book(n, 0);
                let mut opts = ConversionOptions::default();
                opts.insert_metadata = true;
                Jacket.apply(black_box(&mut book), &opts).unwrap();
            })
        });
    }

    group.finish();
}

fn bench_full_transform_pipeline(c: &mut Criterion) {
    let mut group = c.benchmark_group("FullPipeline");

    for n in [10, 50, 200] {
        group.bench_function(format!("{}_chapters", n), |b| {
            b.iter(|| {
                let mut book = make_book(n, 5);
                let opts = ConversionOptions::default();
                let transforms = convert_transforms::standard_transforms();
                for t in &transforms {
                    if t.should_run(&opts) {
                        t.apply(black_box(&mut book), &opts).unwrap();
                    }
                }
            })
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_merge_metadata,
    bench_detect_structure,
    bench_css_flattener,
    bench_image_rescale,
    bench_manifest_trimmer,
    bench_split_chapters,
    bench_data_url,
    bench_unsmarten,
    bench_linearize_tables,
    bench_page_margin,
    bench_jacket,
    bench_full_transform_pipeline,
);
criterion_main!(benches);
