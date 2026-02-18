//! Benchmarks for ebook conversion transforms.

use criterion::{black_box, criterion_group, criterion_main, Criterion};

use convert_core::book::{BookDocument, ManifestData, ManifestItem, TocEntry};
use convert_core::options::ConversionOptions;
use convert_core::plugin::Transform;
use convert_transforms::css_flattener::CssFlattener;
use convert_transforms::detect_structure::DetectStructure;
use convert_transforms::image_rescale::ImageRescale;
use convert_transforms::manifest_trimmer::ManifestTrimmer;
use convert_transforms::merge_metadata::MergeMetadata;

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
        book.toc.add(TocEntry::new(
            format!("Chapter {}", i),
            &href,
        ));
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
    group.sample_size(10); // Fewer samples since image ops are slower

    // Create larger images for meaningful benchmark
    let make_book_with_large_images = |num_images: usize, size: u32| -> BookDocument {
        let mut book = make_book(1, 0);
        for i in 0..num_images {
            let id = format!("img{}", i);
            let href = format!("images/img{}.png", i);
            let img = image::RgbaImage::new(size, size);
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

    group.finish();
}

fn bench_manifest_trimmer(c: &mut Criterion) {
    let mut group = c.benchmark_group("ManifestTrimmer");

    group.bench_function("100_chapters_50_unreferenced", |b| {
        b.iter(|| {
            let mut book = make_book(100, 50); // 50 images not referenced in XHTML
            let opts = ConversionOptions::default();
            ManifestTrimmer.apply(black_box(&mut book), &opts).unwrap();
        })
    });

    group.finish();
}

fn bench_full_transform_pipeline(c: &mut Criterion) {
    let mut group = c.benchmark_group("FullPipeline");

    for n in [10, 50] {
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
    bench_full_transform_pipeline,
);
criterion_main!(benches);
