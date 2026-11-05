//! Dependency-free microbenchmark for the kasumi-text engine.
//!
//! Run with:
//!     cargo run --release -p kasumi-text --example bench
//!
//! The workload mirrors the benchmark used to evaluate the old
//! cosmic-text/swash pipeline: warm cache hits, unique cache misses, a
//! fallback-heavy mixed-script paragraph and glyph rasterization.

use std::time::{Duration, Instant};

use kasumi_core::TextStyle;
use kasumi_text::TextSystem;

fn median(mut samples: Vec<Duration>) -> Duration {
    samples.sort();
    samples[samples.len() / 2]
}

fn main() {
    let start = Instant::now();
    let mut engine = TextSystem::new();
    let init = start.elapsed();
    println!("engine_init: {:?} (faces: {})", init, engine.face_count());

    let styles = [
        TextStyle::default().size(14.0),
        TextStyle::default().size(16.0),
        TextStyle::default().size(20.0),
        TextStyle::default().size(28.0),
    ];
    let samples = [
        "Hello, Kasumi",
        "The quick brown fox jumps over the lazy dog",
        "one two three four five six seven eight nine ten",
        "中文混排测试：这是一个字体排版示例。",
        "Hi 世界 🌍 مرحبا",
        "0123456789 !@#$%^&*()",
        "Material Design 3 typography sample",
    ];

    let cold = |engine: &mut TextSystem, text: &str, style: &TextStyle| {
        let start = Instant::now();
        let layout = engine.layout(text, style, Some(320.0));
        std::hint::black_box(layout);
        start.elapsed()
    };
    println!(
        "cold_first_layout: latin {:?}, cjk {:?}, mixed {:?}, emoji {:?}",
        cold(&mut engine, samples[1], &styles[2]),
        cold(&mut engine, samples[3], &styles[1]),
        cold(&mut engine, samples[4], &styles[1]),
        cold(&mut engine, "🌍", &styles[1]),
    );

    // Warm all faces, fallbacks and shaper caches.
    for style in &styles {
        for text in samples {
            let _ = engine.layout(text, style, Some(320.0));
            let _ = engine.layout(text, style, None);
        }
    }
    println!("faces after warmup: {}", engine.face_count());

    for rounds in 0..5 {
        let mut unique = rounds * 10_000;
        let mut hit_samples = Vec::new();
        for _ in 0..20 {
            let start = Instant::now();
            for _ in 0..1_000 {
                let layout = engine.layout("Hello, Kasumi", &styles[1], Some(320.0));
                std::hint::black_box(layout);
            }
            hit_samples.push(start.elapsed() / 1_000);
        }

        let mut shape_samples = Vec::new();
        for _ in 0..20 {
            let texts: Vec<(String, usize)> = (0..50)
                .map(|i| {
                    unique += 1;
                    let base = samples[i % samples.len()];
                    (format!("{base} #{unique}"), i)
                })
                .collect();
            let start = Instant::now();
            for (text, i) in &texts {
                let layout = engine.layout(text, &styles[i % styles.len()], Some(320.0));
                std::hint::black_box(layout);
            }
            shape_samples.push(start.elapsed() / texts.len() as u32);
        }

        let layout = engine.layout(samples[1], &styles[2], None).clone();
        let mut raster_samples = Vec::new();
        for _ in 0..20 {
            let start = Instant::now();
            for glyph in &layout.glyphs {
                let raster = engine.rasterize(glyph.face, glyph.glyph_id, glyph.font_size, 1.0);
                std::hint::black_box(raster);
            }
            raster_samples.push(start.elapsed() / layout.glyphs.len() as u32);
        }

        let mut mixed_samples = Vec::new();
        for _ in 0..20 {
            unique += 1;
            let text = format!("{}{} #{}", samples[3], samples[4], unique);
            let start = Instant::now();
            let layout = engine.layout(&text, &styles[1], Some(320.0));
            std::hint::black_box(layout);
            mixed_samples.push(start.elapsed());
        }

        println!(
            "round {rounds}: cache_hit {:?}, shape_unique {:?}, mixed_unique {:?}, raster {:?}",
            median(hit_samples),
            median(shape_samples),
            median(mixed_samples),
            median(raster_samples)
        );
    }

    let layout = engine.layout(samples[1], &styles[2], Some(320.0));
    println!(
        "latin layout: {} glyphs, {} lines, {:.2}x{:.2}",
        layout.glyphs.len(),
        layout.lines.len(),
        layout.size.width,
        layout.size.height
    );
    let mixed = engine.layout(samples[4], &styles[1], Some(320.0));
    println!(
        "mixed layout: {} glyphs, {} lines, {:.2}x{:.2}",
        mixed.glyphs.len(),
        mixed.lines.len(),
        mixed.size.width,
        mixed.size.height
    );
}
