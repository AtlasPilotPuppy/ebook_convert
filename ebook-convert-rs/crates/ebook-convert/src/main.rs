//! ebook-convert-rs — Rust reimplementation of Calibre's ebook-convert.
//!
//! Supports two CLI modes:
//! - Legacy: `ebook-convert-rs input.pdf output.epub [--options]`
//! - Modern: `ebook-convert-rs convert --from pdf --to epub input.pdf -o output.epub`

use std::path::PathBuf;
use std::process;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

use convert_core::book::EbookFormat;
use convert_core::options::{ConversionOptions, PdfEngine};
use convert_core::pipeline::PipelineBuilder;
use convert_core::plugin::{InputPlugin, OutputPlugin, Transform};

#[derive(Parser)]
#[command(name = "ebook-convert-rs", version, about = "Fast ebook format converter")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Input file (legacy mode)
    #[arg(global = false)]
    input: Option<PathBuf>,

    /// Output file (legacy mode)
    #[arg(global = false)]
    output: Option<PathBuf>,

    /// Verbosity level
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    verbose: u8,

    /// Extra CSS to apply
    #[arg(long, global = true)]
    extra_css: Option<String>,

    /// Maximum image size (WxH). Defaults to output profile screen size.
    #[arg(long, global = true)]
    max_image_size: Option<String>,

    /// JPEG quality for transcoded images (1-100, default 80)
    #[arg(long, global = true, default_value = "80")]
    jpeg_quality: u8,

    /// Debug pipeline output directory
    #[arg(long, global = true)]
    debug_pipeline: Option<PathBuf>,

    /// PDF extraction engine: auto, image-only, text-only (default: auto)
    #[arg(long, global = true, default_value = "auto")]
    pdf_engine: String,

    /// PDF rendering DPI (default: 200)
    #[arg(long, global = true, default_value = "200")]
    pdf_dpi: u16,
}

#[derive(Subcommand)]
enum Commands {
    /// Convert an ebook (modern interface)
    Convert {
        /// Input file
        input: PathBuf,

        /// Output file
        #[arg(short, long)]
        output: PathBuf,

        /// Input format (auto-detected from extension if omitted)
        #[arg(long)]
        from: Option<String>,

        /// Output format (auto-detected from extension if omitted)
        #[arg(long)]
        to: Option<String>,
    },
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let cli = Cli::parse();

    let result = match &cli.command {
        Some(Commands::Convert {
            input,
            output,
            from,
            to,
        }) => run_conversion(input.clone(), output.clone(), from.clone(), to.clone(), &cli),
        None => {
            // Legacy mode: positional args
            match (&cli.input, &cli.output) {
                (Some(input), Some(output)) => {
                    run_conversion(input.clone(), output.clone(), None, None, &cli)
                }
                _ => {
                    eprintln!("Usage: ebook-convert-rs <input> <output> [options]");
                    eprintln!("   or: ebook-convert-rs convert <input> -o <output> [options]");
                    process::exit(1);
                }
            }
        }
    };

    if let Err(e) = result {
        eprintln!("Error: {:#}", e);
        process::exit(1);
    }
}

fn run_conversion(
    input: PathBuf,
    output: PathBuf,
    from: Option<String>,
    to: Option<String>,
    cli: &Cli,
) -> Result<()> {
    // Detect formats
    let input_format = from
        .as_deref()
        .and_then(EbookFormat::from_extension)
        .or_else(|| {
            input
                .extension()
                .and_then(|e| e.to_str())
                .and_then(EbookFormat::from_extension)
        })
        .context("Cannot detect input format. Use --from to specify.")?;

    let output_format = to
        .as_deref()
        .and_then(EbookFormat::from_extension)
        .or_else(|| {
            output
                .extension()
                .and_then(|e| e.to_str())
                .and_then(EbookFormat::from_extension)
        })
        .context("Cannot detect output format. Use --to to specify.")?;

    log::info!(
        "Converting {} → {} : {} → {}",
        input.display(),
        output.display(),
        input_format,
        output_format
    );

    // Build options
    let mut options = ConversionOptions::default();
    options.verbose = cli.verbose;
    options.input_format = Some(input_format);
    options.output_format = Some(output_format);
    options.extra_css = cli.extra_css.clone();
    options.debug_pipeline = cli.debug_pipeline.clone();

    options.jpeg_quality = cli.jpeg_quality.clamp(1, 100);

    if let Some(ref size_str) = cli.max_image_size {
        if let Some((w, h)) = parse_size(size_str) {
            options.max_image_size = Some((w, h));
        }
    }

    options.pdf_engine = match cli.pdf_engine.as_str() {
        "image-only" => PdfEngine::ImageOnly,
        "text-only" => PdfEngine::TextOnly,
        _ => PdfEngine::Auto,
    };
    options.pdf_dpi = cli.pdf_dpi;

    // Get plugins
    let input_plugin = get_input_plugin(input_format)?;
    let output_plugin = get_output_plugin(output_format)?;
    let transforms = get_transforms(input_format, output_format);

    // Build pipeline
    let mut builder = PipelineBuilder::new()
        .input(input_plugin)
        .output(output_plugin);

    for t in transforms {
        builder = builder.transform(t);
    }

    let pipeline = builder
        .progress_reporter(Box::new(|frac, msg| {
            if frac < 1.0 {
                log::info!("[{:3.0}%] {}", frac * 100.0, msg);
            } else {
                log::info!("Done!");
            }
        }))
        .build()
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    pipeline
        .run(&input, &output, &options)
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    Ok(())
}

fn get_input_plugin(format: EbookFormat) -> Result<Box<dyn InputPlugin>> {
    match format {
        EbookFormat::Pdf => Ok(Box::new(convert_input_pdf::PdfInputPlugin)),
        EbookFormat::Epub => Ok(Box::new(convert_input_epub::EpubInputPlugin)),
        EbookFormat::Html | EbookFormat::Xhtml => {
            Ok(Box::new(convert_input_html::HtmlInputPlugin))
        }
        EbookFormat::Txt | EbookFormat::Markdown => {
            Ok(Box::new(convert_input_txt::TxtInputPlugin))
        }
        EbookFormat::Mobi | EbookFormat::Azw | EbookFormat::Azw3 => {
            Ok(Box::new(convert_input_mobi::MobiInputPlugin))
        }
        EbookFormat::Docx => Ok(Box::new(convert_input_docx::DocxInputPlugin)),
        EbookFormat::Fb2 => Ok(Box::new(convert_input_fb2::Fb2InputPlugin)),
        EbookFormat::Rtf => Ok(Box::new(convert_input_rtf::RtfInputPlugin)),
        EbookFormat::Odt => Ok(Box::new(convert_input_odt::OdtInputPlugin)),
    }
}

fn get_output_plugin(format: EbookFormat) -> Result<Box<dyn OutputPlugin>> {
    match format {
        EbookFormat::Epub => Ok(Box::new(convert_output_epub::EpubOutputPlugin)),
        EbookFormat::Html | EbookFormat::Xhtml => {
            Ok(Box::new(convert_output_html::HtmlOutputPlugin))
        }
        EbookFormat::Txt => Ok(Box::new(convert_output_txt::TxtOutputPlugin)),
        EbookFormat::Pdf => Ok(Box::new(convert_output_pdf::PdfOutputPlugin)),
        EbookFormat::Mobi | EbookFormat::Azw | EbookFormat::Azw3 => {
            Ok(Box::new(convert_output_mobi::MobiOutputPlugin))
        }
        _ => anyhow::bail!("Unsupported output format: {}", format),
    }
}

fn get_transforms(
    _input_format: EbookFormat,
    _output_format: EbookFormat,
) -> Vec<Box<dyn Transform>> {
    convert_transforms::standard_transforms()
}

fn parse_size(s: &str) -> Option<(u32, u32)> {
    let parts: Vec<&str> = s.split('x').collect();
    if parts.len() == 2 {
        let w = parts[0].parse().ok()?;
        let h = parts[1].parse().ok()?;
        Some((w, h))
    } else {
        None
    }
}
