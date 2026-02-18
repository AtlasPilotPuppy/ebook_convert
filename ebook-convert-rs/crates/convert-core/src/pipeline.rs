//! Pipeline orchestrator — runs the full conversion pipeline.
//!
//! Equivalent to Calibre's `Plumber.run()` (plumber.py lines 999-1232).
//! The pipeline runs in three phases:
//!   Phase 1 (0-34%): Input plugin → BookDocument → postprocess → specialize
//!   Phase 2 (34-90%): Sequential transforms in exact Calibre order
//!   Phase 3 (90-100%): Output plugin → target format

use std::path::Path;

use log::info;

use crate::book::BookDocument;
use crate::error::{ConvertError, Result};
use crate::options::ConversionOptions;
use crate::plugin::{InputPlugin, OutputPlugin, ProgressReporter, Transform};

/// The conversion pipeline orchestrator.
pub struct Pipeline {
    input_plugin: Box<dyn InputPlugin>,
    output_plugin: Box<dyn OutputPlugin>,
    transforms: Vec<Box<dyn Transform>>,
    progress_reporter: Option<ProgressReporter>,
}

impl Pipeline {
    pub fn new(
        input_plugin: Box<dyn InputPlugin>,
        output_plugin: Box<dyn OutputPlugin>,
    ) -> Self {
        Self {
            input_plugin,
            output_plugin,
            transforms: Vec::new(),
            progress_reporter: None,
        }
    }

    /// Add a transform to the pipeline.
    /// Transforms are applied in the order they are added.
    pub fn add_transform(&mut self, transform: Box<dyn Transform>) {
        self.transforms.push(transform);
    }

    /// Set a progress reporter callback.
    pub fn set_progress_reporter(&mut self, reporter: ProgressReporter) {
        self.progress_reporter = Some(reporter);
    }

    /// Run the full conversion pipeline.
    pub fn run(
        &self,
        input_path: &Path,
        output_path: &Path,
        options: &ConversionOptions,
    ) -> Result<()> {
        // Phase 1: Input
        self.report_progress(0.0, "Starting conversion...");

        info!("Running {} input plugin...", self.input_plugin.name());
        self.report_progress(0.01, &format!("Running {} plugin", self.input_plugin.name()));

        let mut book = self.input_plugin.convert(input_path, options)?;
        self.report_progress(0.20, "Input parsing complete");

        // Postprocess
        info!("Running postprocess...");
        self.input_plugin.postprocess(&mut book, options)?;
        self.report_progress(0.25, "Postprocessing complete");

        // Specialize for output format
        let output_format = options
            .output_format
            .unwrap_or(self.output_plugin.output_format());
        info!("Specializing for {} output...", output_format);
        self.input_plugin
            .specialize(&mut book, options, output_format)?;
        self.report_progress(0.34, "Specialization complete");

        // Debug: dump input IR
        if let Some(ref debug_dir) = options.debug_pipeline {
            let input_dir = debug_dir.join("input");
            std::fs::create_dir_all(&input_dir).ok();
            dump_book_debug(&book, &input_dir);
        }

        // Phase 2: Transforms
        let transform_count = self
            .transforms
            .iter()
            .filter(|t| t.should_run(options))
            .count();
        let mut transform_idx = 0;

        for transform in &self.transforms {
            if !transform.should_run(options) {
                info!("Skipping transform: {}", transform.name());
                continue;
            }

            let progress = 0.34 + (0.56 * transform_idx as f64 / transform_count.max(1) as f64);
            info!("Running transform: {}", transform.name());
            self.report_progress(progress, &format!("Running {}", transform.name()));

            transform.apply(&mut book, options).map_err(|e| {
                ConvertError::Pipeline(format!("Transform '{}' failed: {}", transform.name(), e))
            })?;

            transform_idx += 1;
        }

        self.report_progress(0.90, "All transforms complete");

        // Debug: dump processed IR
        if let Some(ref debug_dir) = options.debug_pipeline {
            let processed_dir = debug_dir.join("processed");
            std::fs::create_dir_all(&processed_dir).ok();
            dump_book_debug(&book, &processed_dir);
        }

        // Phase 3: Output
        info!("Running {} output plugin...", self.output_plugin.name());
        self.report_progress(0.90, &format!("Creating {}...", self.output_plugin.name()));

        self.output_plugin.convert(&book, output_path, options)?;

        self.report_progress(1.0, "Conversion complete");
        info!(
            "{} output written to {}",
            output_format,
            output_path.display()
        );

        Ok(())
    }

    fn report_progress(&self, fraction: f64, message: &str) {
        if let Some(ref reporter) = self.progress_reporter {
            reporter(fraction, message);
        }
    }
}

/// Dump book metadata for debug purposes.
fn dump_book_debug(book: &BookDocument, dir: &Path) {
    // Write metadata summary
    let mut meta_lines = Vec::new();
    if let Some(title) = book.metadata.title() {
        meta_lines.push(format!("Title: {}", title));
    }
    for author in book.metadata.authors() {
        meta_lines.push(format!("Author: {}", author));
    }
    meta_lines.push(format!("Manifest items: {}", book.manifest.len()));
    meta_lines.push(format!("Spine items: {}", book.spine.len()));

    let meta_path = dir.join("metadata.txt");
    std::fs::write(meta_path, meta_lines.join("\n")).ok();

    // Dump manifest listing
    let mut manifest_lines = Vec::new();
    for item in book.manifest.iter() {
        manifest_lines.push(format!(
            "{}\t{}\t{}",
            item.id, item.href, item.media_type
        ));
    }
    let manifest_path = dir.join("manifest.txt");
    std::fs::write(manifest_path, manifest_lines.join("\n")).ok();
}

/// Builder for constructing a pipeline with the standard transform ordering.
pub struct PipelineBuilder {
    input_plugin: Option<Box<dyn InputPlugin>>,
    output_plugin: Option<Box<dyn OutputPlugin>>,
    transforms: Vec<Box<dyn Transform>>,
    progress_reporter: Option<ProgressReporter>,
}

impl PipelineBuilder {
    pub fn new() -> Self {
        Self {
            input_plugin: None,
            output_plugin: None,
            transforms: Vec::new(),
            progress_reporter: None,
        }
    }

    pub fn input(mut self, plugin: Box<dyn InputPlugin>) -> Self {
        self.input_plugin = Some(plugin);
        self
    }

    pub fn output(mut self, plugin: Box<dyn OutputPlugin>) -> Self {
        self.output_plugin = Some(plugin);
        self
    }

    pub fn transform(mut self, transform: Box<dyn Transform>) -> Self {
        self.transforms.push(transform);
        self
    }

    pub fn progress_reporter(mut self, reporter: ProgressReporter) -> Self {
        self.progress_reporter = Some(reporter);
        self
    }

    pub fn build(self) -> Result<Pipeline> {
        let input_plugin = self
            .input_plugin
            .ok_or_else(|| ConvertError::Pipeline("No input plugin specified".to_string()))?;
        let output_plugin = self
            .output_plugin
            .ok_or_else(|| ConvertError::Pipeline("No output plugin specified".to_string()))?;

        let mut pipeline = Pipeline::new(input_plugin, output_plugin);
        for t in self.transforms {
            pipeline.add_transform(t);
        }
        if let Some(reporter) = self.progress_reporter {
            pipeline.set_progress_reporter(reporter);
        }
        Ok(pipeline)
    }
}

impl Default for PipelineBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::book::EbookFormat;
    use std::sync::{Arc, Mutex};

    // Minimal test plugins
    struct TestInput;
    impl InputPlugin for TestInput {
        fn name(&self) -> &str {
            "Test Input"
        }
        fn supported_formats(&self) -> &[EbookFormat] {
            &[EbookFormat::Txt]
        }
        fn convert(&self, _path: &Path, _opts: &ConversionOptions) -> Result<BookDocument> {
            let mut book = BookDocument::new();
            book.metadata.set_title("Test");
            Ok(book)
        }
    }

    struct TestOutput;
    impl OutputPlugin for TestOutput {
        fn name(&self) -> &str {
            "Test Output"
        }
        fn output_format(&self) -> EbookFormat {
            EbookFormat::Epub
        }
        fn convert(&self, _book: &BookDocument, _path: &Path, _opts: &ConversionOptions) -> Result<()> {
            Ok(())
        }
    }

    struct TestTransform {
        name: String,
    }
    impl Transform for TestTransform {
        fn name(&self) -> &str {
            &self.name
        }
        fn apply(&self, book: &mut BookDocument, _opts: &ConversionOptions) -> Result<()> {
            // Just append to title to prove it ran
            let current = book.metadata.title().unwrap_or("").to_string();
            book.metadata.set_title(format!("{} [{}]", current, self.name));
            Ok(())
        }
    }

    #[test]
    fn test_pipeline_builder() {
        let pipeline = PipelineBuilder::new()
            .input(Box::new(TestInput))
            .output(Box::new(TestOutput))
            .transform(Box::new(TestTransform {
                name: "T1".to_string(),
            }))
            .transform(Box::new(TestTransform {
                name: "T2".to_string(),
            }))
            .build()
            .unwrap();

        assert_eq!(pipeline.transforms.len(), 2);
    }

    #[test]
    fn test_pipeline_run() {
        let progress = Arc::new(Mutex::new(Vec::new()));
        let progress_clone = progress.clone();

        let pipeline = PipelineBuilder::new()
            .input(Box::new(TestInput))
            .output(Box::new(TestOutput))
            .transform(Box::new(TestTransform {
                name: "T1".to_string(),
            }))
            .progress_reporter(Box::new(move |frac, msg| {
                progress_clone.lock().unwrap().push((frac, msg.to_string()));
            }))
            .build()
            .unwrap();

        let tmp_dir = std::env::temp_dir().join("ebook_convert_test");
        std::fs::create_dir_all(&tmp_dir).ok();
        let input = tmp_dir.join("test.txt");
        let output = tmp_dir.join("test.epub");
        std::fs::write(&input, "test").ok();

        let opts = ConversionOptions::default();
        pipeline.run(&input, &output, &opts).unwrap();

        let progress = progress.lock().unwrap();
        assert!(!progress.is_empty());
        // Last progress should be 1.0
        assert_eq!(progress.last().unwrap().0, 1.0);
    }
}
