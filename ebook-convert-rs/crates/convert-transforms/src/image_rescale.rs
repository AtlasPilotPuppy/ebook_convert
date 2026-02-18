//! ImageRescale transform — resizes images to fit output profile.
//!
//! Uses rayon for parallel processing across images and fast_image_resize
//! for SIMD-accelerated resizing (SSE4.1, AVX2 on x86; NEON on ARM).

use convert_core::book::{BookDocument, ManifestData};
use convert_core::error::Result;
use convert_core::options::ConversionOptions;
use convert_core::plugin::Transform;
use rayon::prelude::*;

/// Rescales images to fit within the output profile's screen dimensions.
pub struct ImageRescale;

impl Transform for ImageRescale {
    fn name(&self) -> &str {
        "ImageRescale"
    }

    fn should_run(&self, options: &ConversionOptions) -> bool {
        !options.no_images && options.max_image_size.is_some()
    }

    fn apply(&self, book: &mut BookDocument, options: &ConversionOptions) -> Result<()> {
        let (max_w, max_h) = options.max_image_size.unwrap_or((
            options.output_profile.screen_width,
            options.output_profile.screen_height,
        ));

        // Collect image items that need processing: (index, data, media_type, href)
        let work: Vec<(usize, Vec<u8>, String, String)> = book
            .manifest
            .iter()
            .enumerate()
            .filter_map(|(i, item)| {
                if !item.is_image() {
                    return None;
                }
                if let ManifestData::Binary(ref data) = item.data {
                    if data.is_empty() {
                        return None;
                    }
                    Some((i, data.clone(), item.media_type.clone(), item.href.clone()))
                } else {
                    None
                }
            })
            .collect();

        if work.is_empty() {
            log::info!("No images to rescale");
            return Ok(());
        }

        log::info!(
            "Processing {} images in parallel (max {}x{})",
            work.len(),
            max_w,
            max_h
        );

        // Process images in parallel with rayon
        let results: Vec<(usize, Option<Vec<u8>>)> = work
            .into_par_iter()
            .map(|(idx, data, media_type, href)| {
                let resized = resize_image(&data, max_w, max_h, &media_type, &href);
                (idx, resized)
            })
            .collect();

        // Apply results back (sequential — mutating book)
        let mut resized_count = 0;
        for (idx, new_data) in results {
            if let Some(data) = new_data {
                if let Some(item) = book.manifest.iter_mut().nth(idx) {
                    item.data = ManifestData::Binary(data);
                    resized_count += 1;
                }
            }
        }

        log::info!("Resized {} images", resized_count);
        Ok(())
    }
}

/// Resize a single image if it exceeds max dimensions.
/// Uses fast_image_resize for SIMD-accelerated Lanczos3 resizing.
fn resize_image(
    data: &[u8],
    max_w: u32,
    max_h: u32,
    media_type: &str,
    href: &str,
) -> Option<Vec<u8>> {
    use fast_image_resize::images::Image;
    use fast_image_resize::{IntoImageView, Resizer};

    let src_image = match image::load_from_memory(data) {
        Ok(img) => img,
        Err(e) => {
            log::warn!("Failed to decode image {}: {}", href, e);
            return None;
        }
    };

    let (w, h) = (src_image.width(), src_image.height());
    if w <= max_w && h <= max_h {
        return None; // No resize needed
    }

    // Calculate new dimensions preserving aspect ratio
    let (new_w, new_h) = fit_dimensions(w, h, max_w, max_h);

    // Get pixel type from source
    let pixel_type = match src_image.pixel_type() {
        Some(pt) => pt,
        None => {
            log::warn!(
                "Unsupported pixel type for {}, falling back to image crate",
                href
            );
            return resize_fallback(&src_image, new_w, new_h, media_type, href);
        }
    };

    // Create destination image
    let mut dst_image = Image::new(new_w, new_h, pixel_type);

    // Resize using SIMD-accelerated Lanczos3
    let mut resizer = Resizer::new();
    if let Err(e) = resizer.resize(&src_image, &mut dst_image, None) {
        log::warn!(
            "fast_image_resize failed for {} ({}x{} → {}x{}): {}, falling back",
            href,
            w,
            h,
            new_w,
            new_h,
            e
        );
        return resize_fallback(&src_image, new_w, new_h, media_type, href);
    }

    // Re-encode
    let mut buf = Vec::new();
    let format = if media_type == "image/png" {
        image::ImageFormat::Png
    } else {
        image::ImageFormat::Jpeg
    };

    // Convert fast_image_resize Image back to DynamicImage for encoding
    let color_type = src_image.color();
    let raw_buffer = dst_image.into_vec();

    match image::RgbaImage::from_raw(new_w, new_h, raw_buffer.clone()) {
        Some(rgba) => {
            let dynamic = image::DynamicImage::ImageRgba8(rgba);
            if let Err(e) = dynamic.write_to(&mut std::io::Cursor::new(&mut buf), format) {
                log::warn!("Failed to encode resized {}: {}", href, e);
                return None;
            }
        }
        None => {
            // Try encoding raw buffer directly
            use image::ImageEncoder;
            let encoder_result = match format {
                image::ImageFormat::Png => {
                    let encoder =
                        image::codecs::png::PngEncoder::new(std::io::Cursor::new(&mut buf));
                    encoder.write_image(&raw_buffer, new_w, new_h, color_type.into())
                }
                _ => {
                    let encoder =
                        image::codecs::jpeg::JpegEncoder::new(std::io::Cursor::new(&mut buf));
                    encoder.write_image(&raw_buffer, new_w, new_h, color_type.into())
                }
            };
            if let Err(e) = encoder_result {
                log::warn!("Failed to encode resized {}: {}", href, e);
                return None;
            }
        }
    }

    log::info!(
        "Resized {} from {}x{} to {}x{} ({} → {} bytes)",
        href,
        w,
        h,
        new_w,
        new_h,
        data.len(),
        buf.len()
    );
    Some(buf)
}

/// Fallback to image crate's resize when fast_image_resize can't handle the format.
fn resize_fallback(
    img: &image::DynamicImage,
    new_w: u32,
    new_h: u32,
    media_type: &str,
    href: &str,
) -> Option<Vec<u8>> {
    let resized = img.resize(new_w, new_h, image::imageops::FilterType::Lanczos3);

    let mut buf = Vec::new();
    let format = if media_type == "image/png" {
        image::ImageFormat::Png
    } else {
        image::ImageFormat::Jpeg
    };

    if let Err(e) = resized.write_to(&mut std::io::Cursor::new(&mut buf), format) {
        log::warn!("Fallback resize encode failed for {}: {}", href, e);
        return None;
    }

    log::info!("Resized {} to {}x{} (fallback)", href, new_w, new_h);
    Some(buf)
}

/// Calculate new dimensions that fit within max_w x max_h preserving aspect ratio.
fn fit_dimensions(w: u32, h: u32, max_w: u32, max_h: u32) -> (u32, u32) {
    let ratio_w = max_w as f64 / w as f64;
    let ratio_h = max_h as f64 / h as f64;
    let ratio = ratio_w.min(ratio_h);
    let new_w = (w as f64 * ratio).round() as u32;
    let new_h = (h as f64 * ratio).round() as u32;
    (new_w.max(1), new_h.max(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fit_dimensions() {
        assert_eq!(fit_dimensions(2000, 1000, 1000, 800), (1000, 500));
        assert_eq!(fit_dimensions(500, 1600, 1000, 800), (250, 800));
        assert_eq!(fit_dimensions(100, 100, 1000, 800), (800, 800)); // scales up (caller checks if resize needed)
    }

    #[test]
    fn test_resize_small_image() {
        // Create a small 2x2 RGBA PNG in memory
        let mut img = image::RgbaImage::new(2, 2);
        img.put_pixel(0, 0, image::Rgba([255, 0, 0, 255]));
        img.put_pixel(1, 0, image::Rgba([0, 255, 0, 255]));
        img.put_pixel(0, 1, image::Rgba([0, 0, 255, 255]));
        img.put_pixel(1, 1, image::Rgba([255, 255, 0, 255]));

        let mut buf = Vec::new();
        image::DynamicImage::ImageRgba8(img)
            .write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png)
            .unwrap();

        // Image is 2x2, max is 1000x1000 — should not resize
        let result = resize_image(&buf, 1000, 1000, "image/png", "test.png");
        assert!(result.is_none()); // no resize needed
    }

    #[test]
    fn test_resize_large_image() {
        // Create a 100x200 image
        let img = image::RgbaImage::new(100, 200);
        let mut buf = Vec::new();
        image::DynamicImage::ImageRgba8(img)
            .write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png)
            .unwrap();

        // Resize to max 50x50
        let result = resize_image(&buf, 50, 50, "image/png", "test.png");
        assert!(result.is_some());

        // Verify the resized image
        let resized = image::load_from_memory(&result.unwrap()).unwrap();
        assert!(resized.width() <= 50);
        assert!(resized.height() <= 50);
    }
}
