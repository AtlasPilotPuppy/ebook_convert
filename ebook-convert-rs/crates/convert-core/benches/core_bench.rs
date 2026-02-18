//! Benchmarks for core IR operations.

use criterion::{black_box, criterion_group, criterion_main, Criterion};

use convert_core::book::{BookDocument, ManifestData, ManifestItem, SpineItem, TocEntry};

fn bench_manifest_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("Manifest");

    group.bench_function("add_100_items", |b| {
        b.iter(|| {
            let mut book = BookDocument::new();
            for i in 0..100 {
                let id = format!("item{}", i);
                let href = format!("content/item{}.xhtml", i);
                book.manifest.add(ManifestItem::new(
                    &id,
                    &href,
                    "application/xhtml+xml",
                    ManifestData::Xhtml(format!("<p>Content {}</p>", i)),
                ));
            }
            black_box(&book);
        })
    });

    group.bench_function("add_1000_items", |b| {
        b.iter(|| {
            let mut book = BookDocument::new();
            for i in 0..1000 {
                let id = format!("item{}", i);
                let href = format!("content/item{}.xhtml", i);
                book.manifest.add(ManifestItem::new(
                    &id,
                    &href,
                    "application/xhtml+xml",
                    ManifestData::Xhtml(format!("<p>Content {}</p>", i)),
                ));
            }
            black_box(&book);
        })
    });

    group.bench_function("lookup_by_id_1000_items", |b| {
        let mut book = BookDocument::new();
        for i in 0..1000 {
            let id = format!("item{}", i);
            let href = format!("content/item{}.xhtml", i);
            book.manifest.add(ManifestItem::new(
                &id,
                &href,
                "application/xhtml+xml",
                ManifestData::Xhtml(format!("<p>Content {}</p>", i)),
            ));
        }
        b.iter(|| {
            for i in 0..1000 {
                let id = format!("item{}", i);
                black_box(book.manifest.by_id(&id));
            }
        })
    });

    group.bench_function("lookup_by_href_1000_items", |b| {
        let mut book = BookDocument::new();
        for i in 0..1000 {
            let id = format!("item{}", i);
            let href = format!("content/item{}.xhtml", i);
            book.manifest.add(ManifestItem::new(
                &id,
                &href,
                "application/xhtml+xml",
                ManifestData::Xhtml(format!("<p>Content {}</p>", i)),
            ));
        }
        b.iter(|| {
            for i in 0..1000 {
                let href = format!("content/item{}.xhtml", i);
                black_box(book.manifest.by_href(&href));
            }
        })
    });

    group.finish();
}

fn bench_spine_toc(c: &mut Criterion) {
    let mut group = c.benchmark_group("SpineToc");

    group.bench_function("build_spine_100", |b| {
        b.iter(|| {
            let mut book = BookDocument::new();
            for i in 0..100 {
                let id = format!("ch{}", i);
                book.spine.push(&id, true);
            }
            black_box(&book);
        })
    });

    group.bench_function("build_toc_100", |b| {
        b.iter(|| {
            let mut book = BookDocument::new();
            for i in 0..100 {
                let mut entry =
                    TocEntry::new(format!("Chapter {}", i), &format!("chapter{}.xhtml", i));
                // Add some children for depth
                for j in 0..3 {
                    entry.add_child(TocEntry::new(
                        format!("Section {}.{}", i, j),
                        &format!("chapter{}.xhtml#sec{}", i, j),
                    ));
                }
                book.toc.add(entry);
            }
            black_box(&book);
        })
    });

    group.bench_function("iterate_toc_depth_first_100", |b| {
        let mut book = BookDocument::new();
        for i in 0..100 {
            let mut entry = TocEntry::new(format!("Chapter {}", i), &format!("chapter{}.xhtml", i));
            for j in 0..3 {
                entry.add_child(TocEntry::new(
                    format!("Section {}.{}", i, j),
                    &format!("chapter{}.xhtml#sec{}", i, j),
                ));
            }
            book.toc.add(entry);
        }
        b.iter(|| {
            let count = book.toc.iter_depth_first().count();
            black_box(count);
        })
    });

    group.finish();
}

criterion_group!(benches, bench_manifest_operations, bench_spine_toc);
criterion_main!(benches);
