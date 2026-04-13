#[cfg(test)]
mod tests {
    use blp::{Blp, FormatDetector, ImageDecoder};

    /// Inspect the StranglethornDetails_256.blp texture:
    /// - BLP1, JPEG content_type, alpha_bits = 0
    /// - Filter mode 1 (Transparent) in the grass model → needs alpha for alpha-test
    ///
    /// HiveWE decodes BLP1 JPEG as CMYK → raw BGRA bytes (no inversion).
    /// The `blp` crate does `255 - value` (CMYK→RGB inversion) AND forces
    /// alpha=255 when alpha_bits==0, which may be wrong for BLP1 JPEG data.
    #[test]
    #[ignore]
    fn inspect_stranglethorn_blp_jpeg() {
        use image::{RgbaImage, Rgba};

        let path = "/Users/nazarpunk/RustroverProjects/War3.mpq/extract/UI/Glues/SinglePlayer/NightElf_Exp/StranglethornDetails_256.blp";
        let buf = std::fs::read(path).expect("Cannot read BLP file");
        let out_dir = "/tmp/blp_test";

        // ── Parse header manually ─────────────────────────────────────
        let magic = &buf[0..4];
        assert_eq!(magic, b"BLP1", "Expected BLP1");
        let content_type = u32::from_le_bytes(buf[4..8].try_into().unwrap());
        let alpha_bits = u32::from_le_bytes(buf[8..12].try_into().unwrap());
        let width = u32::from_le_bytes(buf[12..16].try_into().unwrap());
        let height = u32::from_le_bytes(buf[16..20].try_into().unwrap());

        println!("=== BLP1 Header ===");
        println!("content_type: {} (0=JPEG, 1=Direct/Palette)", content_type);
        println!("alpha_bits: {}", alpha_bits);
        println!("size: {}x{}", width, height);

        // ── Decode with the blp crate ─────────────────────────────────
        let img = Blp::into_dynamic(&buf).expect("BLP decode failed");
        let mip0 = img.to_rgba8();
        println!("mipmap 0: {}x{}", mip0.width(), mip0.height());

        // ── Sample some pixels ────────────────────────────────────────
        // Check corners and some center pixels
        let samples = [
            (0, 0), (1, 0), (0, 1),
            (128, 128), (64, 64), (200, 200),
            (width - 1, height - 1),
            // Likely transparent areas (edges of grass)
            (0, height - 1), (width - 1, 0),
        ];

        println!("\n=== Pixel samples (blp crate output) ===");
        println!("{:<12} {:>5} {:>5} {:>5} {:>5}", "pos", "R", "G", "B", "A");
        for (x, y) in &samples {
            let px = mip0.get_pixel(*x, *y);
            println!("({:3},{:3})     {:5} {:5} {:5} {:5}", x, y, px[0], px[1], px[2], px[3]);
        }

        // ── Count alpha distribution ──────────────────────────────────
        let total = (mip0.width() * mip0.height()) as usize;
        let mut alpha_0 = 0usize;
        let mut alpha_255 = 0usize;
        let mut alpha_other = 0usize;
        let mut min_alpha = 255u8;
        let mut max_alpha = 0u8;

        for px in mip0.pixels() {
            let a = px[3];
            if a == 0 { alpha_0 += 1; }
            else if a == 255 { alpha_255 += 1; }
            else { alpha_other += 1; }
            if a < min_alpha { min_alpha = a; }
            if a > max_alpha { max_alpha = a; }
        }

        println!("\n=== Alpha channel stats ===");
        println!("total pixels: {}", total);
        println!("alpha=0 (transparent): {} ({:.1}%)", alpha_0, alpha_0 as f64 / total as f64 * 100.0);
        println!("alpha=255 (opaque):    {} ({:.1}%)", alpha_255, alpha_255 as f64 / total as f64 * 100.0);
        println!("alpha=other:           {} ({:.1}%)", alpha_other, alpha_other as f64 / total as f64 * 100.0);
        println!("alpha range: {} .. {}", min_alpha, max_alpha);

        // ── Now decode JPEG manually to see raw CMYK channels ────────
        println!("\n=== Raw JPEG decode (bypassing blp crate inversion) ===");
        // Read JPEG header
        let mip0_offset = u32::from_le_bytes(buf[28..32].try_into().unwrap()) as usize;
        let mip0_size = u32::from_le_bytes(buf[92..96].try_into().unwrap()) as usize;
        let jpeg_header_size = u32::from_le_bytes(buf[156..160].try_into().unwrap()) as usize;
        let jpeg_header_offset = 160usize;

        println!("JPEG header: offset={}, size={}", jpeg_header_offset, jpeg_header_size);
        println!("mip0: offset={}, size={}", mip0_offset, mip0_size);

        let header_bytes = &buf[jpeg_header_offset..jpeg_header_offset + jpeg_header_size];
        let tail = &buf[mip0_offset..mip0_offset + mip0_size];

        let mut full = Vec::with_capacity(header_bytes.len() + tail.len());
        full.extend_from_slice(header_bytes);
        full.extend_from_slice(tail);

        let mut dec = jpeg_decoder::Decoder::new(std::io::Cursor::new(&full));
        dec.read_info().expect("JPEG read_info failed");
        let info = dec.info().expect("No JPEG info");
        println!("JPEG pixel_format: {:?}", info.pixel_format);
        println!("JPEG size: {}x{}", info.width, info.height);

        let raw_pixels = dec.decode().expect("JPEG decode failed");
        let bpp = raw_pixels.len() / (info.width as usize * info.height as usize);
        println!("bytes_per_pixel: {}", bpp);

        if bpp == 4 {
            println!("\n=== Raw CMYK samples (no inversion) ===");
            println!("{:<12} {:>5} {:>5} {:>5} {:>5}   →  {:>5} {:>5} {:>5} {:>5}",
                     "pos", "ch0", "ch1", "ch2", "ch3",
                     "R_noInv", "G_noInv", "B_noInv", "A_noInv");

            for (x, y) in &samples {
                let p = (*y as usize * width as usize + *x as usize) * 4;
                if p + 3 < raw_pixels.len() {
                    let ch0 = raw_pixels[p + 0]; // C (or B in BLP)
                    let ch1 = raw_pixels[p + 1]; // M (or G in BLP)
                    let ch2 = raw_pixels[p + 2]; // Y (or R in BLP)
                    let ch3 = raw_pixels[p + 3]; // K (or A in BLP)

                    // HiveWE interpretation: ch0=B, ch1=G, ch2=R, ch3=A
                    // (then swap B↔R to get RGBA)
                    let r_hive = ch2;
                    let g_hive = ch1;
                    let b_hive = ch0;
                    let a_hive = ch3;

                    println!("({:3},{:3})     {:5} {:5} {:5} {:5}   →  {:5} {:5} {:5} {:5}",
                             x, y, ch0, ch1, ch2, ch3,
                             r_hive, g_hive, b_hive, a_hive);
                }
            }

            // ── Alpha stats from raw K channel ────────────────────────
            let pixel_count = info.width as usize * info.height as usize;
            let mut k_0 = 0usize;
            let mut k_255 = 0usize;
            let mut k_other = 0usize;
            let mut k_min = 255u8;
            let mut k_max = 0u8;

            for p in 0..pixel_count {
                let k = raw_pixels[p * 4 + 3];
                if k == 0 { k_0 += 1; }
                else if k == 255 { k_255 += 1; }
                else { k_other += 1; }
                if k < k_min { k_min = k; }
                if k > k_max { k_max = k; }
            }

            println!("\n=== Raw K channel (alpha) stats ===");
            println!("K=0:   {} ({:.1}%)", k_0, k_0 as f64 / pixel_count as f64 * 100.0);
            println!("K=255: {} ({:.1}%)", k_255, k_255 as f64 / pixel_count as f64 * 100.0);
            println!("K=other: {} ({:.1}%)", k_other, k_other as f64 / pixel_count as f64 * 100.0);
            println!("K range: {} .. {}", k_min, k_max);

            // ── Compare BLP crate output with HiveWE-style (no inversion) ──
            println!("\n=== Color comparison: BLP crate vs HiveWE-style ===");
            println!("{:<12} {:>20} {:>20}", "pos", "blp_crate(RGBA)", "hivewe(RGBA)");
            for (x, y) in &samples {
                let blp_px = mip0.get_pixel(*x, *y);
                let p = (*y as usize * width as usize + *x as usize) * 4;
                if p + 3 < raw_pixels.len() {
                    let r_hive = raw_pixels[p + 2];
                    let g_hive = raw_pixels[p + 1];
                    let b_hive = raw_pixels[p + 0];
                    let a_hive = raw_pixels[p + 3];
                    println!("({:3},{:3})     ({:3},{:3},{:3},{:3})     ({:3},{:3},{:3},{:3})",
                             x, y,
                             blp_px[0], blp_px[1], blp_px[2], blp_px[3],
                             r_hive, g_hive, b_hive, a_hive);
                }
            }

            // ── Save PNG variants for visual comparison ───────────────
            std::fs::create_dir_all(out_dir).ok();

            // 1) BLP crate output (current — inverted colors, forced alpha)
            mip0.save(format!("{out_dir}/1_blp_crate_current.png")).expect("save failed");
            println!("\nSaved: {out_dir}/1_blp_crate_current.png");

            // 2) BLP crate colors + K channel alpha (inverted colors, real alpha via 255-K)
            let mut img_inv_with_alpha = RgbaImage::new(width, height);
            for p in 0..pixel_count {
                let idx = p * 4;
                let ch0 = raw_pixels[idx + 0];
                let ch1 = raw_pixels[idx + 1];
                let ch2 = raw_pixels[idx + 2];
                let k   = raw_pixels[idx + 3];
                let r = 255u8.saturating_sub(ch2);
                let g = 255u8.saturating_sub(ch1);
                let b = 255u8.saturating_sub(ch0);
                let a = 255u8.saturating_sub(k);
                img_inv_with_alpha.put_pixel(p as u32 % width, p as u32 / width, Rgba([r, g, b, a]));
            }
            img_inv_with_alpha.save(format!("{out_dir}/2_inverted_colors_real_alpha.png")).expect("save failed");
            println!("Saved: {out_dir}/2_inverted_colors_real_alpha.png");

            // 3) HiveWE-style: no inversion, BGRA direct, A=K
            let mut img_hive_a_eq_k = RgbaImage::new(width, height);
            for p in 0..pixel_count {
                let idx = p * 4;
                let b_val = raw_pixels[idx + 0]; // C → B
                let g_val = raw_pixels[idx + 1]; // M → G
                let r_val = raw_pixels[idx + 2]; // Y → R
                let a_val = raw_pixels[idx + 3]; // K → A
                img_hive_a_eq_k.put_pixel(p as u32 % width, p as u32 / width, Rgba([r_val, g_val, b_val, a_val]));
            }
            img_hive_a_eq_k.save(format!("{out_dir}/3_hivewe_no_inv_A_eq_K.png")).expect("save failed");
            println!("Saved: {out_dir}/3_hivewe_no_inv_A_eq_K.png");

            // 4) HiveWE-style: no inversion, BGRA direct, A=255-K
            let mut img_hive_a_inv_k = RgbaImage::new(width, height);
            for p in 0..pixel_count {
                let idx = p * 4;
                let b_val = raw_pixels[idx + 0];
                let g_val = raw_pixels[idx + 1];
                let r_val = raw_pixels[idx + 2];
                let a_val = 255u8.saturating_sub(raw_pixels[idx + 3]);
                img_hive_a_inv_k.put_pixel(p as u32 % width, p as u32 / width, Rgba([r_val, g_val, b_val, a_val]));
            }
            img_hive_a_inv_k.save(format!("{out_dir}/4_hivewe_no_inv_A_eq_255minusK.png")).expect("save failed");
            println!("Saved: {out_dir}/4_hivewe_no_inv_A_eq_255minusK.png");

            // 5) RGB order without swap, A=K
            let mut img_rgb_a_k = RgbaImage::new(width, height);
            for p in 0..pixel_count {
                let idx = p * 4;
                let r_val = raw_pixels[idx + 0]; // C → R
                let g_val = raw_pixels[idx + 1]; // M → G
                let b_val = raw_pixels[idx + 2]; // Y → B
                let a_val = raw_pixels[idx + 3]; // K → A
                img_rgb_a_k.put_pixel(p as u32 % width, p as u32 / width, Rgba([r_val, g_val, b_val, a_val]));
            }
            img_rgb_a_k.save(format!("{out_dir}/5_no_swap_A_eq_K.png")).expect("save failed");
            println!("Saved: {out_dir}/5_no_swap_A_eq_K.png");

            // 6) RGB order without swap, A=255-K
            let mut img_rgb_a_inv_k = RgbaImage::new(width, height);
            for p in 0..pixel_count {
                let idx = p * 4;
                let r_val = raw_pixels[idx + 0];
                let g_val = raw_pixels[idx + 1];
                let b_val = raw_pixels[idx + 2];
                let a_val = 255u8.saturating_sub(raw_pixels[idx + 3]);
                img_rgb_a_inv_k.put_pixel(p as u32 % width, p as u32 / width, Rgba([r_val, g_val, b_val, a_val]));
            }
            img_rgb_a_inv_k.save(format!("{out_dir}/6_no_swap_A_eq_255minusK.png")).expect("save failed");
            println!("Saved: {out_dir}/6_no_swap_A_eq_255minusK.png");

            println!("\n=== Open /tmp/blp_test/ to compare visually ===");
        }
    }
}

