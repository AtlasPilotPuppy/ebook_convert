//! Benchmarks for PDF output plugin.

use criterion::{black_box, criterion_group, criterion_main, Criterion};

use convert_core::book::{BookDocument, ManifestData, ManifestItem, TocEntry};
use convert_core::options::ConversionOptions;
use convert_core::plugin::OutputPlugin;
use convert_output_pdf::PdfOutputPlugin;

/// Create a book with N chapters and M images.
fn make_book(num_chapters: usize, num_images: usize) -> BookDocument {
    let mut book = BookDocument::new();
    book.metadata.set_title("PDF Benchmark Book");
    book.metadata.add("creator", "Bench Author");
    book.metadata.set("language", "en");

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
            None,
            &body,
        );
        book.manifest.add(ManifestItem::new(
            &id,
            &href,
            "application/xhtml+xml",
            ManifestData::Xhtml(xhtml),
        ));
        book.spine.push(&id, true);
        book.toc.add(TocEntry::new(
            format!("Chapter {}", i),
            &href,
        ));
    }

    for i in 0..num_images {
        let id = format!("img{}", i);
        let href = format!("images/img{}.png", i);
        let img = image::RgbaImage::new(50, 50);
        let mut buf = Vec::new();
        image::DynamicImage::ImageRgba8(img)
            .write_to(
                &mut std::io::Cursor::new(&mut buf),
                image::ImageFormat::Png,
            )
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

fn bench_pdf_output(c: &mut Criterion) {
    let mut group = c.benchmark_group("PdfOutput");
    group.sample_size(10);

    let plugin = PdfOutputPlugin;
    let opts = ConversionOptions::default();

    group.bench_function("10_chapters_0_images", |b| {
        let book = make_book(10, 0);
        b.iter(|| {
            let tmp = std::env::temp_dir().join("bench_pdf_10_0.pdf");
            plugin.convert(black_box(&book), &tmp, &opts).unwrap();
            std::fs::remove_file(&tmp).ok();
        })
    });

    group.bench_function("50_chapters_0_images", |b| {
        let book = make_book(50, 0);
        b.iter(|| {
            let tmp = std::env::temp_dir().join("bench_pdf_50_0.pdf");
            plugin.convert(black_box(&book), &tmp, &opts).unwrap();
            std::fs::remove_file(&tmp).ok();
        })
    });

    group.bench_function("10_chapters_10_images", |b| {
        let book = make_book(10, 10);
        b.iter(|| {
            let tmp = std::env::temp_dir().join("bench_pdf_10_10.pdf");
            plugin.convert(black_box(&book), &tmp, &opts).unwrap();
            std::fs::remove_file(&tmp).ok();
        })
    });

    group.bench_function("50_chapters_10_images", |b| {
        let book = make_book(50, 10);
        b.iter(|| {
            let tmp = std::env::temp_dir().join("bench_pdf_50_10.pdf");
            plugin.convert(black_box(&book), &tmp, &opts).unwrap();
            std::fs::remove_file(&tmp).ok();
        })
    });

    group.finish();
}

criterion_group!(benches, bench_pdf_output);
criterion_main!(benches);
