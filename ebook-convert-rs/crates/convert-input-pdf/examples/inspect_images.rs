fn main() {
    let doc = lopdf::Document::load("/tmp/hinduism.pdf").expect("load PDF");
    let pages = doc.get_pages();
    let mut nums: Vec<u32> = pages.keys().copied().collect();
    nums.sort();

    // Check first 3 pages
    for &pn in nums.iter().take(3) {
        let page_id = pages[&pn];
        let images = doc.get_page_images(page_id).unwrap_or_default();
        println!("\n=== Page {} ({} images) ===", pn, images.len());
        
        for (i, img) in images.iter().enumerate() {
            let filters: Vec<&str> = img.filters.as_ref()
                .map(|f| f.iter().map(|s| s.as_str()).collect())
                .unwrap_or_default();
            
            // Check the actual PDF object for this image
            if let Ok(obj) = doc.get_object(img.id) {
                if let Ok(stream) = obj.as_stream() {
                    let dict = &stream.dict;
                    let color_space = dict.get(b"ColorSpace")
                        .map(|v| format!("{:?}", v))
                        .unwrap_or("none".into());
                    let smask = dict.get(b"SMask")
                        .map(|v| format!("{:?}", v))
                        .unwrap_or("none".into());
                    let mask = dict.get(b"Mask")
                        .map(|v| format!("{:?}", v))
                        .unwrap_or("none".into());
                    let bpc = dict.get(b"BitsPerComponent")
                        .map(|v| v.as_i64().unwrap_or(0))
                        .unwrap_or(0);
                    let intent = dict.get(b"Intent")
                        .map(|v| format!("{:?}", v))
                        .unwrap_or("none".into());
                    let decode = dict.get(b"Decode")
                        .map(|v| format!("{:?}", v))
                        .unwrap_or("none".into());
                    
                    println!("  img[{}] id={:?} {}x{} {} bytes filters={:?}", 
                        i, img.id, img.width, img.height, img.content.len(), filters);
                    println!("    ColorSpace={}", color_space);
                    println!("    BitsPerComponent={}", bpc);
                    println!("    SMask={}", smask);
                    println!("    Mask={}", mask);
                    println!("    Intent={}", intent);
                    println!("    Decode={}", decode);
                    
                    // Print all dict keys
                    let keys: Vec<String> = dict.iter()
                        .map(|(k, _)| String::from_utf8_lossy(k).to_string())
                        .collect();
                    println!("    All keys: {:?}", keys);
                }
            }
        }
    }
}
