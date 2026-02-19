//! PDF page rendering via `pdftoppm` (poppler-utils).
//!
//! Supports rendering all pages or specific page ranges.

use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

use rayon::prelude::*;

use convert_core::error::{ConvertError, Result};
use convert_core::options::ConversionOptions;

/// Check that pdftoppm is available on the system.
pub fn check_pdftoppm() -> Result<()> {
    let which = Command::new("which")
        .arg("pdftoppm")
        .output()
        .map_err(|e| ConvertError::Pdf(format!("Failed to check for pdftoppm: {}", e)))?;

    if !which.status.success() {
        return Err(ConvertError::Pdf(
            "pdftoppm (poppler-utils) is required for PDF conversion. \
             Install with: brew install poppler (macOS) or apt install poppler-utils (Linux)"
                .to_string(),
        ));
    }
    Ok(())
}

/// Render all PDF pages to JPEG images.
/// Returns a Vec of (page_number, jpeg_data) in order.
pub fn render_all_pages(
    pdf_path: &Path,
    num_pages: u32,
    options: &ConversionOptions,
) -> Result<Vec<(u32, Vec<u8>)>> {
    check_pdftoppm()?;

    let tmp_dir = tempfile::TempDir::new()
        .map_err(|e| ConvertError::Pdf(format!("Failed to create temp dir: {}", e)))?;

    let prefix = tmp_dir.path().join("page");
    let prefix_str = prefix
        .to_str()
        .ok_or_else(|| ConvertError::Pdf("Invalid temp path".to_string()))?;

    let dpi = options.pdf_dpi.to_string();
    let quality = options.jpeg_quality.to_string();

    log::info!(
        "Rendering {} pages with pdftoppm at {} DPI...",
        num_pages,
        dpi
    );

    let output = Command::new("pdftoppm")
        .arg("-jpeg")
        .arg("-jpegopt")
        .arg(format!("quality={}", quality))
        .arg("-r")
        .arg(&dpi)
        .arg(pdf_path.as_os_str())
        .arg(prefix_str)
        .output()
        .map_err(|e| ConvertError::Pdf(format!("Failed to run pdftoppm: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(ConvertError::Pdf(format!("pdftoppm failed: {}", stderr)));
    }

    collect_rendered_pages(tmp_dir.path(), num_pages)
}

/// Render specific page ranges via pdftoppm.
/// Returns a HashMap of page_number -> jpeg_data.
pub fn render_page_ranges(
    pdf_path: &Path,
    page_numbers: &[u32],
    total_pages: u32,
    options: &ConversionOptions,
) -> Result<HashMap<u32, Vec<u8>>> {
    if page_numbers.is_empty() {
        return Ok(HashMap::new());
    }

    check_pdftoppm()?;

    let dpi = options.pdf_dpi.to_string();
    let quality = options.jpeg_quality.to_string();
    let ranges = contiguous_ranges(page_numbers);

    log::info!(
        "Rendering {} scanned pages in {} batch(es) with pdftoppm...",
        page_numbers.len(),
        ranges.len()
    );

    // Process ranges in parallel — each spawns its own pdftoppm + temp dir
    let page_numbers_set: std::collections::HashSet<u32> = page_numbers.iter().copied().collect();

    let batch_results: Vec<Result<HashMap<u32, Vec<u8>>>> = ranges
        .par_iter()
        .map(|(first, last)| {
            log::info!(
                "[pdftoppm] Rendering pages {}-{} of {} scanned...",
                first,
                last,
                total_pages
            );

            let tmp_dir = tempfile::TempDir::new()
                .map_err(|e| ConvertError::Pdf(format!("Failed to create temp dir: {}", e)))?;

            let prefix = tmp_dir.path().join("page");
            let prefix_str = prefix
                .to_str()
                .ok_or_else(|| ConvertError::Pdf("Invalid temp path".to_string()))?;

            let output = Command::new("pdftoppm")
                .arg("-jpeg")
                .arg("-jpegopt")
                .arg(format!("quality={}", quality))
                .arg("-r")
                .arg(&dpi)
                .arg("-f")
                .arg(first.to_string())
                .arg("-l")
                .arg(last.to_string())
                .arg(pdf_path.as_os_str())
                .arg(prefix_str)
                .output()
                .map_err(|e| ConvertError::Pdf(format!("Failed to run pdftoppm: {}", e)))?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(ConvertError::Pdf(format!(
                    "pdftoppm failed for pages {}-{}: {}",
                    first, last, stderr
                )));
            }

            // Collect rendered pages from this batch
            let mut batch: HashMap<u32, Vec<u8>> = HashMap::new();
            for page_num in *first..=*last {
                if page_numbers_set.contains(&page_num) {
                    if let Some(path) = find_rendered_page(tmp_dir.path(), page_num, total_pages) {
                        let data = std::fs::read(&path).map_err(|e| {
                            ConvertError::Pdf(format!(
                                "Failed to read rendered page {}: {}",
                                page_num, e
                            ))
                        })?;
                        batch.insert(page_num, data);
                    }
                }
            }
            Ok(batch)
        })
        .collect();

    // Merge all batch results
    let mut result: HashMap<u32, Vec<u8>> = HashMap::new();
    for batch_result in batch_results {
        result.extend(batch_result?);
    }

    Ok(result)
}

/// Group non-contiguous page numbers into minimal contiguous ranges.
///
/// E.g., `[1, 2, 3, 7, 8, 12]` → `[(1, 3), (7, 8), (12, 12)]`
pub fn contiguous_ranges(pages: &[u32]) -> Vec<(u32, u32)> {
    if pages.is_empty() {
        return Vec::new();
    }

    let mut sorted: Vec<u32> = pages.to_vec();
    sorted.sort_unstable();
    sorted.dedup();

    let mut ranges = Vec::new();
    let mut start = sorted[0];
    let mut end = sorted[0];

    for &p in &sorted[1..] {
        if p == end + 1 {
            end = p;
        } else {
            ranges.push((start, end));
            start = p;
            end = p;
        }
    }
    ranges.push((start, end));

    ranges
}

/// Collect all rendered pages from a temp directory (parallel file reads).
fn collect_rendered_pages(dir: &Path, num_pages: u32) -> Result<Vec<(u32, Vec<u8>)>> {
    let results: Vec<(u32, std::result::Result<Vec<u8>, ConvertError>)> = (1..=num_pages)
        .into_par_iter()
        .map(
            |page_num| match find_rendered_page(dir, page_num, num_pages) {
                Some(path) => match std::fs::read(&path) {
                    Ok(data) => (page_num, Ok(data)),
                    Err(e) => (
                        page_num,
                        Err(ConvertError::Pdf(format!(
                            "Failed to read rendered page {}: {}",
                            page_num, e
                        ))),
                    ),
                },
                None => {
                    log::warn!("No rendered image found for page {}", page_num);
                    (page_num, Ok(Vec::new()))
                }
            },
        )
        .collect();

    let mut pages = Vec::with_capacity(num_pages as usize);
    for (page_num, data_result) in results {
        pages.push((page_num, data_result?));
    }
    Ok(pages)
}

/// Find the rendered JPEG file for a given page number.
/// pdftoppm zero-pads based on total page count.
pub fn find_rendered_page(
    dir: &Path,
    page_num: u32,
    total_pages: u32,
) -> Option<std::path::PathBuf> {
    let width = if total_pages >= 1000 {
        4
    } else if total_pages >= 100 {
        3
    } else {
        2
    };

    let padded = format!("{:0>width$}", page_num, width = width);
    let name = format!("page-{}.jpg", padded);
    let path = dir.join(&name);

    if path.exists() {
        return Some(path);
    }

    // Try other common patterns
    for w in 1..=6 {
        let padded = format!("{:0>width$}", page_num, width = w);
        let name = format!("page-{}.jpg", padded);
        let path = dir.join(&name);
        if path.exists() {
            return Some(path);
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_contiguous_ranges_basic() {
        assert_eq!(
            contiguous_ranges(&[1, 2, 3, 7, 8, 12]),
            vec![(1, 3), (7, 8), (12, 12)]
        );
    }

    #[test]
    fn test_contiguous_ranges_single() {
        assert_eq!(contiguous_ranges(&[5]), vec![(5, 5)]);
    }

    #[test]
    fn test_contiguous_ranges_empty() {
        assert_eq!(contiguous_ranges(&[]), Vec::<(u32, u32)>::new());
    }

    #[test]
    fn test_contiguous_ranges_all_contiguous() {
        assert_eq!(contiguous_ranges(&[1, 2, 3, 4, 5]), vec![(1, 5)]);
    }

    #[test]
    fn test_contiguous_ranges_all_separate() {
        assert_eq!(
            contiguous_ranges(&[1, 3, 5, 7]),
            vec![(1, 1), (3, 3), (5, 5), (7, 7)]
        );
    }

    #[test]
    fn test_contiguous_ranges_unsorted() {
        assert_eq!(contiguous_ranges(&[5, 3, 1, 2, 4]), vec![(1, 5)]);
    }

    #[test]
    fn test_find_rendered_page() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("page-03.jpg");
        std::fs::write(&path, b"fake jpeg").unwrap();
        let found = find_rendered_page(dir.path(), 3, 56);
        assert!(found.is_some());
        assert_eq!(found.unwrap(), path);
    }
}
