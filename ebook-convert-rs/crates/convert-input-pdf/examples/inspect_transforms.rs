/// Inspect the content stream of PDF pages to find image transform matrices.
fn main() {
    let doc = lopdf::Document::load("/tmp/hinduism.pdf").expect("load PDF");
    let pages = doc.get_pages();
    let mut nums: Vec<u32> = pages.keys().copied().collect();
    nums.sort();

    // Check first 4 pages
    for &pn in nums.iter().take(4) {
        let page_id = pages[&pn];
        println!("\n=== Page {} ===", pn);

        // Get page content stream
        if let Ok(content) = doc.get_page_content(page_id) {
            let content_str = String::from_utf8_lossy(&content);
            // Look for 'cm' (concat matrix) and 'Do' (draw XObject) operators
            // Also look for 'q' (save state) and 'Q' (restore state)
            for line in content_str.lines() {
                let trimmed = line.trim();
                if trimmed.contains("cm") || trimmed.contains("Do")
                    || trimmed.contains(" q") || trimmed == "q" || trimmed == "Q"
                    || trimmed.contains("Tm") {
                    println!("  {}", trimmed);
                }
            }
        }

        // Also try parsing content ops
        if let Ok(content_data) = doc.get_page_content(page_id) {
            let ops = lopdf::content::Content::decode(&content_data);
            if let Ok(content) = ops {
                for op in &content.operations {
                    let name = &op.operator;
                    if name == "cm" || name == "Do" || name == "q" || name == "Q" {
                        let operands: Vec<String> = op.operands.iter()
                            .map(|o| format!("{:?}", o))
                            .collect();
                        println!("  OP: {} {}", name, operands.join(" "));
                    }
                }
            }
        }
    }
}
