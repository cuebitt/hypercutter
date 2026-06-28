//! Batch-compare all reference images against current output.
//!
//! Usage: `cargo run --example compare-all -- <reference-dir> <output-dir> [threshold]`
//!
//! Walks `<reference-dir>` recursively. For each `.png`, looks for the
//! same path under `<output-dir>` and reports the diff. The optional
//! third argument sets the "significant pixel %" threshold; the run
//! exits non-zero if any file exceeds it.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result};
use image::{Rgba, RgbaImage};
use walkdir::WalkDir;

fn compare(expected: &Path, actual: &Path) -> Result<CompareStats> {
    let expected_img = image::open(expected)
        .with_context(|| format!("opening {}", expected.display()))?
        .to_rgba8();
    let actual_img = image::open(actual)
        .with_context(|| format!("opening {}", actual.display()))?
        .to_rgba8();

    if expected_img.dimensions() != actual_img.dimensions() {
        anyhow::bail!(
            "dimension mismatch: expected {}x{}, actual {}x{}",
            expected_img.width(),
            expected_img.height(),
            actual_img.width(),
            actual_img.height(),
        );
    }

    let (w, h) = expected_img.dimensions();
    let total = u64::from(w) * u64::from(h);
    let mut per_channel = [0u64; 4];
    let mut max_diff = 0u32;
    let mut significant = 0u64;
    let mut diff_img = RgbaImage::new(w, h);

    for y in 0..h {
        for x in 0..w {
            let e: Rgba<u8> = *expected_img.get_pixel(x, y);
            let a: Rgba<u8> = *actual_img.get_pixel(x, y);
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

    Ok(CompareStats {
        significant,
        total,
        per_channel,
        max_diff,
        diff_img,
    })
}

#[allow(dead_code)]
struct CompareStats {
    significant: u64,
    total: u64,
    per_channel: [u64; 4],
    max_diff: u32,
    diff_img: RgbaImage,
}

fn main() -> Result<ExitCode> {
    let args: Vec<String> = std::env::args().collect();
    let reference_dir = args
        .get(1)
        .context("usage: compare-all <reference-dir> <output-dir> [threshold]")?;
    let output_dir = args
        .get(2)
        .context("usage: compare-all <reference-dir> <output-dir> [threshold]")?;
    let threshold: f64 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(5.0);

    let mut failed: Vec<(PathBuf, f64)> = Vec::new();
    let mut compared = 0u32;
    let mut passed = 0u32;
    let mut missing = 0u32;

    for entry in WalkDir::new(reference_dir)
        .into_iter()
        .filter_map(Result::ok)
    {
        if !entry.file_type().is_file() {
            continue;
        }
        if entry.path().extension().and_then(|s| s.to_str()) != Some("png") {
            continue;
        }
        let rel = entry.path().strip_prefix(reference_dir)?.to_path_buf();
        let actual = Path::new(output_dir).join(&rel);
        if !actual.exists() {
            missing += 1;
            eprintln!("MISSING: {}", rel.display());
            continue;
        }
        compared += 1;
        match compare(entry.path(), &actual) {
            Ok(s) => {
                let pct = 100.0 * s.significant as f64 / s.total as f64;
                if pct > threshold {
                    failed.push((rel, pct));
                } else {
                    passed += 1;
                }
            }
            Err(e) => {
                eprintln!("ERROR {}: {e}", rel.display());
                failed.push((rel, f64::INFINITY));
            }
        }
    }

    println!();
    println!("compared: {compared}");
    println!("passed (<= {threshold}% significant pixels): {passed}");
    println!("failed (> {threshold}%): {}", failed.len());
    println!("missing in output: {missing}");

    if !failed.is_empty() {
        println!();
        println!("Worst failures (sorted by %):");
        failed.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        for (path, pct) in failed.iter().take(20) {
            let p = if pct.is_finite() {
                format!("{pct:.2}%")
            } else {
                "ERR".to_owned()
            };
            println!("  {p:>8}  {}", path.display());
        }
    }

    if failed.is_empty() {
        Ok(ExitCode::SUCCESS)
    } else {
        Ok(ExitCode::FAILURE)
    }
}
