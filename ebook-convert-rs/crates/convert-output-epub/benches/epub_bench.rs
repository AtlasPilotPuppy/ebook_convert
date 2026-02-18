//! Benchmarks for EPUB output plugin.

use criterion::{black_box, criterion_group, criterion_main, Criterion};

use convert_core::book::{BookDocument, ManifestData, ManifestItem, TocEntry};
use convert_core::options::ConversionOptions;
use convert_core::plugin::OutputPlugin;
use convert_output_epub::EpubOutputPlugin;

/// Create a book with N chapters and M images.
fn make_book(num_chapters: usize, num_images: usize) -> BookDocument {
    let mut book = BookDocument::new();
    book.metadata.set_title("EPUB Benchmark Book");
    book.metadata.add("creator", "Bench Author");
    book.metadata.set("language", "en");

    let css = "body { margin: 1em; font-family: serif; }";
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
             <p>Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor \
             incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud \
             exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat.</p>\n\
             <p>Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore \
             eu fugiat nulla pariatur. Excepteur sint occaecat cupidatat non proident.</p>",
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
        book.toc.add(TocEntry::new(format!("Chapter {}", i), &href));
    }

    for i in 0..num_images {
        let id = format!("img{}", i);
        let href = format!("images/img{}.png", i);
        let img = image::RgbaImage::new(50, 50);
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

fn bench_epub_output(c: &mut Criterion) {
    let mut group = c.benchmark_group("EpubOutput");
    group.sample_size(10);

    let plugin = EpubOutputPlugin;
    let opts = ConversionOptions::default();

    group.bench_function("10_chapters_0_images", |b| {
        let book = make_book(10, 0);
        b.iter(|| {
            let tmp = std::env::temp_dir().join("bench_epub_10_0.epub");
            plugin.convert(black_box(&book), &tmp, &opts).unwrap();
            std::fs::remove_file(&tmp).ok();
        })
    });

    group.bench_function("50_chapters_20_images", |b| {
        let book = make_book(50, 20);
        b.iter(|| {
            let tmp = std::env::temp_dir().join("bench_epub_50_20.epub");
            plugin.convert(black_box(&book), &tmp, &opts).unwrap();
            std::fs::remove_file(&tmp).ok();
        })
    });

    group.bench_function("200_chapters_0_images", |b| {
        let book = make_book(200, 0);
        b.iter(|| {
            let tmp = std::env::temp_dir().join("bench_epub_200_0.epub");
            plugin.convert(black_box(&book), &tmp, &opts).unwrap();
            std::fs::remove_file(&tmp).ok();
        })
    });

    group.bench_function("10_chapters_100_images", |b| {
        let book = make_book(10, 100);
        b.iter(|| {
            let tmp = std::env::temp_dir().join("bench_epub_10_100.epub");
            plugin.convert(black_box(&book), &tmp, &opts).unwrap();
            std::fs::remove_file(&tmp).ok();
        })
    });

    group.bench_function("50_chapters_0_images", |b| {
        let book = make_book(50, 0);
        b.iter(|| {
            let tmp = std::env::temp_dir().join("bench_epub_50_0.epub");
            plugin.convert(black_box(&book), &tmp, &opts).unwrap();
            std::fs::remove_file(&tmp).ok();
        })
    });

    group.bench_function("200_chapters_100_images", |b| {
        let book = make_book(200, 100);
        b.iter(|| {
            let tmp = std::env::temp_dir().join("bench_epub_200_100.epub");
            plugin.convert(black_box(&book), &tmp, &opts).unwrap();
            std::fs::remove_file(&tmp).ok();
        })
    });

    group.finish();
}

criterion_group!(benches, bench_epub_output);
criterion_main!(benches);
