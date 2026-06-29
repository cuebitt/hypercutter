//! Compare two PNG images pixel-by-pixel and report differences.
//!
//! Usage:
//!
//! ```text
//! cargo run --example compare -- <expected.png> <actual.png> [diff.png]
//! ```
//!
//! Prints per-channel mean absolute difference, the percentage of pixels
//! that match within a tolerance, and the worst-case difference. If a
//! `diff.png` path is given, writes a visualization where transparent
//! pixels are matches, yellow pixels are small differences, and red
//! pixels are large differences (per-channel sum > 10).

use anyhow::{Context, Result};
use image::{Rgba, RgbaImage};

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let expected_path = args
        .get(1)
        .context("usage: compare <expected.png> <actual.png> [diff.png]")?;
    let actual_path = args
        .get(2)
        .context("usage: compare <expected.png> <actual.png> [diff.png]")?;
    let diff_path = args.get(3).cloned();

    let expected = image::open(expected_path)
        .with_context(|| format!("opening {expected_path}"))?
        .to_rgba8();
    let actual = image::open(actual_path)
        .with_context(|| format!("opening {actual_path}"))?
        .to_rgba8();

    if expected.dimensions() != actual.dimensions() {
        anyhow::bail!(
            "dimension mismatch: expected {}x{}, actual {}x{}",
            expected.width(),
            expected.height(),
            actual.width(),
            actual.height(),
        );
    }

    let (w, h) = expected.dimensions();
    let total = u64::from(w) * u64::from(h);
    let mut per_channel = [0u64; 4];
    let mut max_diff = 0u32;
    let mut significant = 0u64;
    let mut diff_img = RgbaImage::new(w, h);

    for y in 0..h {
        for x in 0..w {
            let e: Rgba<u8> = *expected.get_pixel(x, y);
            let a: Rgba<u8> = *actual.get_pixel(x, y);
            let dr = u32::from(e.0[0].abs_diff(a.0[0]));
            let dg = u32::from(e.0[1].abs_diff(a.0[1]));
            let db = u32::from(e.0[2].abs_diff(a.0[2]));
            let da = u32::from(e.0[3].abs_diff(a.0[3]));
            per_channel[0] += u64::from(dr);
            per_channel[1] += u64::from(dg);
            per_channel[2] += u64::from(db);
            per_channel[3] += u64::from(da);
            let d = dr + dg + db + da;
            max_diff = max_diff.max(d);
            let px = if d == 0 {
                Rgba([0, 0, 0, 0])
            } else if d > 10 {
                significant += 1;
                Rgba([255, 0, 0, 255])
            } else {
                Rgba([255, 255, 0, 128])
            };
            diff_img.put_pixel(x, y, px);
        }
    }

    let matching = total - significant;
    let mean = |c: u64| c as f64 / total as f64;
    println!("dimensions: {w}x{h}");
    println!("total pixels: {total}");
    println!(
        "matching (diff <= 10): {matching} ({:.2}%)",
        100.0 * matching as f64 / total as f64
    );
    println!(
        "significant (diff > 10): {significant} ({:.2}%)",
        100.0 * significant as f64 / total as f64
    );
    println!(
        "mean abs diff per channel: R={:.2} G={:.2} B={:.2} A={:.2}",
        mean(per_channel[0]),
        mean(per_channel[1]),
        mean(per_channel[2]),
        mean(per_channel[3])
    );
    println!("max diff: {max_diff}");

    if let Some(p) = diff_path.as_deref() {
        diff_img.save(p).with_context(|| format!("saving {p}"))?;
        println!("diff visualization: {p}");
    }

    Ok(())
}
