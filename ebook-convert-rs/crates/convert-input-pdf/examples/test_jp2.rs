use std::io::Write;

fn main() {
    let doc = lopdf::Document::load("/tmp/hinduism.pdf").expect("load PDF");
    let pages = doc.get_pages();
    let mut nums: Vec<u32> = pages.keys().copied().collect();
    nums.sort();

    // Page 1 (cover)
    let page_id = pages[&nums[0]];
    let images = doc.get_page_images(page_id).expect("images");
    println!("Page 1: {} images", images.len());

    std::fs::create_dir_all("/tmp/jp2_test").ok();

    for (i, image) in images.iter().enumerate().take(2) {
        let raw = image.content.to_vec();
        let filters: Vec<&str> = image
            .filters
            .as_ref()
            .map(|f| f.iter().map(|s| s.as_str()).collect())
            .unwrap_or_default();
        println!(
            "\nImage {}: id={:?}, {}x{} (PDF), {} bytes, filters={:?}",
            i, image.id, image.width, image.height, raw.len(), filters
        );

        let is_jp2 = filters.iter().any(|s| *s == "JPXDecode");
        if !is_jp2 {
            continue;
        }

        // Save raw JP2 bytes
        let jp2_path = format!("/tmp/jp2_test/img_{}.jp2", i);
        std::fs::write(&jp2_path, &raw).unwrap();
        println!("  Saved JP2: {} ({} bytes)", jp2_path, raw.len());

        // Decode with openjp2
        let format = match openjp2::detect_format(&raw) {
            Ok(openjp2::J2KFormat::JP2) => {
                println!("  Format: JP2");
                openjp2::OPJ_CODEC_JP2
            }
            Ok(openjp2::J2KFormat::J2K) => {
                println!("  Format: J2K");
                openjp2::OPJ_CODEC_J2K
            }
            Ok(openjp2::J2KFormat::JPT) => {
                println!("  Format: JPT");
                openjp2::OPJ_CODEC_JPT
            }
            Err(_) => {
                println!("  Format: fallback J2K");
                openjp2::OPJ_CODEC_J2K
            }
        };

        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        tmp.write_all(&raw).unwrap();
        tmp.flush().unwrap();

        let mut codec = openjp2::Codec::new_decoder(format).unwrap();
        let mut params = openjp2::opj_dparameters_t::default();
        assert!(codec.setup_decoder(&mut params) != 0);

        let mut stream = openjp2::Stream::new_file(tmp.path(), 1024 * 1024, true).unwrap();
        let mut img = codec.read_header(&mut stream).unwrap();

        println!(
            "  Image rect: ({},{}) -> ({},{})",
            img.x0, img.y0, img.x1, img.y1
        );
        println!("  Color space: {:?}", img.color_space);
        println!("  Num comps: {}", img.numcomps);

        assert!(codec.decode(&mut stream, &mut img) != 0);
        codec.end_decompress(&mut stream);

        let comps = img.comps().unwrap();
        for (ci, c) in comps.iter().enumerate() {
            let d = c.data().unwrap();
            let min = d.iter().copied().min().unwrap_or(0);
            let max = d.iter().copied().max().unwrap_or(0);
            println!(
                "  comp[{}]: w={} h={} dx={} dy={} prec={} sgnd={} len={} range=[{},{}]",
                ci, c.w, c.h, c.dx, c.dy, c.prec, c.sgnd, d.len(), min, max
            );
        }

        let w = (img.x1 - img.x0) as u32;
        let h = (img.y1 - img.y0) as u32;
        let pdf_w = image.width as u32;
        let pdf_h = image.height as u32;

        println!("  JP2 dims: {}x{}, PDF dims: {}x{}", w, h, pdf_w, pdf_h);

        let nc = img.numcomps as usize;
        if nc >= 3 {
            let c0 = comps[0].data().unwrap();
            let c1 = comps[1].data().unwrap();
            let c2 = comps[2].data().unwrap();

            // Decode using component width as stride
            let comp_w = comps[0].w;
            let mut rgb = Vec::with_capacity((w * h * 3) as usize);
            for y in 0..h {
                for x in 0..w {
                    let idx = (y * comp_w + x) as usize;
                    if idx < c0.len() {
                        rgb.push(c0[idx].clamp(0, 255) as u8);
                        rgb.push(c1[idx].clamp(0, 255) as u8);
                        rgb.push(c2[idx].clamp(0, 255) as u8);
                    } else {
                        rgb.extend_from_slice(&[0, 0, 0]);
                    }
                }
            }
            let png_path = format!("/tmp/jp2_test/decoded_compw_{}.png", i);
            let out = image::RgbImage::from_raw(w, h, rgb).unwrap();
            out.save(&png_path).unwrap();
            println!("  Saved: {} (w={}, stride=comp.w={})", png_path, w, comp_w);

            // Decode using PDF width as both image width and stride
            if pdf_w != w || pdf_h != h {
                let mut rgb2 = Vec::with_capacity((pdf_w * pdf_h * 3) as usize);
                for y in 0..pdf_h {
                    for x in 0..pdf_w {
                        let idx = (y * pdf_w + x) as usize;
                        if idx < c0.len() {
                            rgb2.push(c0[idx].clamp(0, 255) as u8);
                            rgb2.push(c1[idx].clamp(0, 255) as u8);
                            rgb2.push(c2[idx].clamp(0, 255) as u8);
                        } else {
                            rgb2.extend_from_slice(&[0, 0, 0]);
                        }
                    }
                }
                let pdf_path = format!("/tmp/jp2_test/decoded_pdfw_{}.png", i);
                if let Some(out2) = image::RgbImage::from_raw(pdf_w, pdf_h, rgb2) {
                    out2.save(&pdf_path).unwrap();
                    println!("  Saved: {} (pdf_w={}, stride=pdf_w)", pdf_path, pdf_w);
                }
            }
        }
    }
}
