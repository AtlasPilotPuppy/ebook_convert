//! End-to-end pipeline benchmarks: input → transforms → output.

use criterion::{black_box, criterion_group, criterion_main, Criterion};

use convert_core::book::EbookFormat;
use convert_core::options::ConversionOptions;
use convert_core::pipeline::PipelineBuilder;
use convert_core::plugin::Transform;

/// Generate an HTML file with N chapters.
fn generate_html(num_chapters: usize) -> String {
    let mut html = String::from(
        "<!DOCTYPE html>\n<html>\n<head><title>Benchmark</title></head>\n<body>\n",
    );
    for i in 0..num_chapters {
        html.push_str(&format!(
            "<h1>Chapter {}</h1>\n\
             <p>Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor \
             incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud \
             exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat.</p>\n\
             <p>Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore \
             eu fugiat nulla pariatur. Excepteur sint occaecat cupidatat non proident, sunt in \
             culpa qui officia deserunt mollit anim id est laborum.</p>\n",
            i
        ));
    }
    html.push_str("</body>\n</html>\n");
    html
}

/// Generate a plain text file with N paragraphs.
fn generate_txt(num_paragraphs: usize) -> String {
    let mut txt = String::new();
    for i in 0..num_paragraphs {
        txt.push_str(&format!(
            "Chapter {}\n\n\
             Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor \
             incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud \
             exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat.\n\n",
            i
        ));
    }
    txt
}

fn bench_html_to_epub(c: &mut Criterion) {
    let mut group = c.benchmark_group("E2E_HTML_to_EPUB");
    group.sample_size(10);

    for n in [10, 100] {
        group.bench_function(format!("{}_chapters", n), |b| {
            let html_content = generate_html(n);
            let input_path = std::env::temp_dir().join(format!("bench_input_{}.html", n));
            std::fs::write(&input_path, &html_content).unwrap();

            let output_path = std::env::temp_dir().join(format!("bench_output_{}.epub", n));

            b.iter(|| {
                let mut opts = ConversionOptions::default();
                opts.input_format = Some(EbookFormat::Html);
                opts.output_format = Some(EbookFormat::Epub);

                let input_plugin =
                    Box::new(convert_input_html::HtmlInputPlugin) as Box<dyn convert_core::plugin::InputPlugin>;
                let output_plugin =
                    Box::new(convert_output_epub::EpubOutputPlugin) as Box<dyn convert_core::plugin::OutputPlugin>;
                let transforms: Vec<Box<dyn Transform>> = convert_transforms::standard_transforms();

                let mut builder = PipelineBuilder::new()
                    .input(input_plugin)
                    .output(output_plugin);
                for t in transforms {
                    builder = builder.transform(t);
                }
                let pipeline = builder.build().unwrap();
                pipeline.run(black_box(&input_path), &output_path, &opts).unwrap();
            });

            std::fs::remove_file(&input_path).ok();
            std::fs::remove_file(&output_path).ok();
        });
    }

    group.finish();
}

fn bench_txt_to_epub(c: &mut Criterion) {
    let mut group = c.benchmark_group("E2E_TXT_to_EPUB");
    group.sample_size(10);

    for (label, n) in [("small_10", 10), ("large_100", 100)] {
        group.bench_function(label, |b| {
            let txt_content = generate_txt(n);
            let input_path = std::env::temp_dir().join(format!("bench_input_{}.txt", n));
            std::fs::write(&input_path, &txt_content).unwrap();

            let output_path = std::env::temp_dir().join(format!("bench_output_{}.epub", n));

            b.iter(|| {
                let mut opts = ConversionOptions::default();
                opts.input_format = Some(EbookFormat::Txt);
                opts.output_format = Some(EbookFormat::Epub);

                let input_plugin =
                    Box::new(convert_input_txt::TxtInputPlugin) as Box<dyn convert_core::plugin::InputPlugin>;
                let output_plugin =
                    Box::new(convert_output_epub::EpubOutputPlugin) as Box<dyn convert_core::plugin::OutputPlugin>;
                let transforms: Vec<Box<dyn Transform>> = convert_transforms::standard_transforms();

                let mut builder = PipelineBuilder::new()
                    .input(input_plugin)
                    .output(output_plugin);
                for t in transforms {
                    builder = builder.transform(t);
                }
                let pipeline = builder.build().unwrap();
                pipeline.run(black_box(&input_path), &output_path, &opts).unwrap();
            });

            std::fs::remove_file(&input_path).ok();
            std::fs::remove_file(&output_path).ok();
        });
    }

    group.finish();
}

criterion_group!(benches, bench_html_to_epub, bench_txt_to_epub);
criterion_main!(benches);
