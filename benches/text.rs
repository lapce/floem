//! Benchmarks for text layout performance in Floem.
//!
//! These benchmarks measure the performance of floem's `TextLayout` abstraction
//! (which wraps Parley 0.7), including:
//! - Layout creation with varying text sizes and scripts
//! - Styled text with attribute spans
//! - Line breaking / reflow (`set_size`)
//! - Hit testing (forward and reverse)
//! - Selection geometry computation
//! - Visual line iteration
//! - Stress tests with large/mixed-script documents
//!
//! A bundled font (DejaVu Serif) is registered to ensure reproducible results
//! across machines.

use std::hint::black_box;
use std::sync::Once;

use criterion::{Criterion, criterion_group, criterion_main};
use floem::text::{
    Affinity, Attrs, AttrsList, FONT_CONTEXT, FamilyOwned, FontStyle, FontWeight, TextLayout,
};
use peniko::Color;

// =============================================================================
// Font setup — bundled DejaVu Serif for reproducibility
// =============================================================================

const DEJAVU_SERIF: &[u8] = include_bytes!("../examples/webgpu/fonts/DejaVuSerif.ttf");

static FONT_INIT: Once = Once::new();

fn ensure_font() {
    FONT_INIT.call_once(|| {
        let mut font_cx = FONT_CONTEXT.lock();
        font_cx
            .collection
            .register_fonts(DEJAVU_SERIF.to_vec().into(), None);
    });
}

// =============================================================================
// Test data
// =============================================================================

const SHORT: &str = "Hello, world!";

const SENTENCE: &str = "The quick brown fox jumps over the lazy dog 0123456789 abcdefghij";

const PARAGRAPH: &str = "Lorem ipsum dolor sit amet, consectetur adipiscing elit. \
    Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. \
    Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris \
    nisi ut aliquip ex ea commodo consequat. Duis aute irure dolor in \
    reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla \
    pariatur. Excepteur sint occaecat cupidatat non proident, sunt in \
    culpa qui officia deserunt mollit anim id est laborum.";

const ARABIC: &str = "هذا نص عربي لاختبار الاتجاه من اليمين إلى اليسار \
    ومعالجة النصوص ثنائية الاتجاه. يتضمن هذا النص عدة جمل لاختبار \
    تخطيط النص العربي والتفاف الكلمات بشكل صحيح.";

const MIXED_BIDI: &str = "Mixed English and العربية النص العربي text for BiDi testing. \
    This tests combined left-to-right and right-to-left processing \
    with embedded العربية segments في النص الإنجليزي throughout.";

const EMOJI: &str = "Hello 👋 World 🌍! Testing emoji 🎉🎊🎈 with text 📝 \
    and flags 🇺🇸🇬🇧🇫🇷🇩🇪🇯🇵 and families 👨‍👩‍👧‍👦 and skin tones 👋🏻👋🏽👋🏿 \
    plus sequences 🏳️‍🌈 and more complex ones 👩‍💻🧑‍🔬.";

fn multi_paragraph() -> String {
    (0..4).map(|_| PARAGRAPH).collect::<Vec<_>>().join("\n")
}

fn long_document() -> String {
    (0..20).map(|_| PARAGRAPH).collect::<Vec<_>>().join("\n")
}

fn default_attrs() -> AttrsList {
    let family = vec![FamilyOwned::Name("DejaVu Serif".into())];
    AttrsList::new(Attrs::new().font_size(16.0).family(&family))
}

fn styled_attrs(text_len: usize, span_count: usize) -> AttrsList {
    let family = vec![FamilyOwned::Name("DejaVu Serif".into())];
    let mut attrs = AttrsList::new(Attrs::new().font_size(16.0).family(&family));
    if span_count == 0 {
        return attrs;
    }
    let chunk = text_len / span_count;
    for i in 0..span_count {
        let start = i * chunk;
        let end = (start + chunk).min(text_len);
        let span_attrs = match i % 3 {
            0 => Attrs::new()
                .font_size(16.0)
                .family(&family)
                .weight(FontWeight::BOLD),
            1 => Attrs::new()
                .font_size(16.0)
                .family(&family)
                .font_style(FontStyle::Italic),
            _ => Attrs::new()
                .font_size(20.0)
                .family(&family)
                .color(Color::from_rgba8(255, 0, 0, 255)),
        };
        attrs.add_span(start..end, span_attrs);
    }
    attrs
}

// =============================================================================
// Group 1: Layout creation — end-to-end TextLayout::set_text()
// =============================================================================

fn bench_layout_creation(c: &mut Criterion) {
    ensure_font();
    let mut group = c.benchmark_group("layout_creation");

    // Short text, no wrapping
    group.bench_function("short_plain", |b| {
        b.iter(|| {
            let mut layout = TextLayout::new();
            layout.set_text(SHORT, default_attrs(), None);
            black_box(&layout);
        });
    });

    // Single sentence, no wrapping
    group.bench_function("sentence_plain", |b| {
        b.iter(|| {
            let mut layout = TextLayout::new();
            layout.set_text(SENTENCE, default_attrs(), None);
            black_box(&layout);
        });
    });

    // Paragraph with word wrapping at 400px
    group.bench_function("paragraph_wrap", |b| {
        b.iter(|| {
            let mut layout = TextLayout::new();
            layout.set_text(PARAGRAPH, default_attrs(), None);
            layout.set_size(400.0, f32::MAX);
            black_box(&layout);
        });
    });

    // Arabic text with wrapping
    group.bench_function("arabic_wrap", |b| {
        b.iter(|| {
            let mut layout = TextLayout::new();
            layout.set_text(ARABIC, default_attrs(), None);
            layout.set_size(400.0, f32::MAX);
            black_box(&layout);
        });
    });

    // Mixed BiDi text with wrapping
    group.bench_function("mixed_bidi_wrap", |b| {
        b.iter(|| {
            let mut layout = TextLayout::new();
            layout.set_text(MIXED_BIDI, default_attrs(), None);
            layout.set_size(400.0, f32::MAX);
            black_box(&layout);
        });
    });

    // Emoji-heavy text with wrapping
    group.bench_function("emoji_wrap", |b| {
        b.iter(|| {
            let mut layout = TextLayout::new();
            layout.set_text(EMOJI, default_attrs(), None);
            layout.set_size(400.0, f32::MAX);
            black_box(&layout);
        });
    });

    // Multi-paragraph with wrapping
    let mp = multi_paragraph();
    group.bench_function("multi_paragraph_wrap", |b| {
        b.iter(|| {
            let mut layout = TextLayout::new();
            layout.set_text(&mp, default_attrs(), None);
            layout.set_size(400.0, f32::MAX);
            black_box(&layout);
        });
    });

    // Long document with wrapping
    let ld = long_document();
    group.bench_function("long_document_wrap", |b| {
        b.iter(|| {
            let mut layout = TextLayout::new();
            layout.set_text(&ld, default_attrs(), None);
            layout.set_size(600.0, f32::MAX);
            black_box(&layout);
        });
    });

    group.finish();
}

// =============================================================================
// Group 2: Styled layout — varying span counts
// =============================================================================

fn bench_styled_layout(c: &mut Criterion) {
    ensure_font();
    let mut group = c.benchmark_group("styled_layout");

    for (name, span_count) in [("no_spans", 0), ("few_spans", 5), ("many_spans", 50)] {
        group.bench_function(name, |b| {
            let attrs = styled_attrs(PARAGRAPH.len(), span_count);
            b.iter(|| {
                let mut layout = TextLayout::new();
                layout.set_text(PARAGRAPH, attrs.clone(), None);
                layout.set_size(400.0, f32::MAX);
                black_box(&layout);
            });
        });
    }

    group.finish();
}

// =============================================================================
// Group 3: Line breaking — set_size() reflow on pre-built layout
// =============================================================================

fn bench_line_breaking(c: &mut Criterion) {
    ensure_font();
    let mut group = c.benchmark_group("line_breaking");

    // Actual reflow: alternate widths so each call does real work
    for (name, width) in [
        ("reflow_narrow_100", 100.0f32),
        ("reflow_medium_400", 400.0),
        ("reflow_wide_1200", 1200.0),
    ] {
        group.bench_function(name, |b| {
            let mut layout = TextLayout::new();
            layout.set_text(PARAGRAPH, default_attrs(), None);
            let mut toggle = false;
            b.iter(|| {
                let w = if toggle { width } else { width + 100.0 };
                toggle = !toggle;
                layout.set_size(w, f32::MAX);
                black_box(layout.size());
            });
        });
    }

    // Early-return path: same width repeated (measures the guard check)
    group.bench_function("same_width_noop", |b| {
        let mut layout = TextLayout::new();
        layout.set_text(PARAGRAPH, default_attrs(), None);
        layout.set_size(400.0, f32::MAX);
        b.iter(|| {
            layout.set_size(400.0, f32::MAX);
            black_box(layout.size());
        });
    });

    group.finish();
}

// =============================================================================
// Group 4: Hit testing — forward (pixel→cursor) and reverse (cursor→pixel)
// =============================================================================

fn bench_hit_testing(c: &mut Criterion) {
    ensure_font();
    let mut group = c.benchmark_group("hit_testing");

    let mut layout = TextLayout::new();
    layout.set_text(PARAGRAPH, default_attrs(), None);
    layout.set_size(400.0, f32::MAX);
    let size = layout.size();
    let text_len = PARAGRAPH.len();

    // Forward: pixel → cursor
    group.bench_function("hit_start", |b| {
        b.iter(|| black_box(layout.hit_test(Point::new(0.0, 0.0))));
    });
    group.bench_function("hit_middle", |b| {
        b.iter(|| black_box(layout.hit_test(Point::new(size.width / 2.0, size.height / 2.0))));
    });
    group.bench_function("hit_end", |b| {
        b.iter(|| black_box(layout.hit_test(Point::new(size.width, size.height))));
    });

    // Reverse: cursor → pixel
    group.bench_function("hit_position_start", |b| {
        b.iter(|| black_box(layout.cursor_point(0, Affinity::Upstream)));
    });
    group.bench_function("hit_position_middle", |b| {
        b.iter(|| black_box(layout.cursor_point(text_len / 2, Affinity::Upstream)));
    });
    group.bench_function("hit_position_end", |b| {
        b.iter(|| black_box(layout.cursor_point(text_len, Affinity::Upstream)));
    });

    // Hit testing on BiDi text
    let mut bidi_layout = TextLayout::new();
    bidi_layout.set_text(MIXED_BIDI, default_attrs(), None);
    bidi_layout.set_size(400.0, f32::MAX);
    let bidi_size = bidi_layout.size();
    group.bench_function("hit_middle_bidi", |b| {
        b.iter(|| {
            black_box(
                bidi_layout.hit_test(Point::new(bidi_size.width / 2.0, bidi_size.height / 2.0)),
            )
        });
    });

    // Round-trip: hit → cursor_to_byte_index (measures decomposition + recomposition cost).
    group.bench_function("hit_then_byte_index", |b| {
        let mid_x = size.width as f32 / 2.0;
        let mid_y = size.height as f32 / 2.0;
        b.iter(|| {
            let cursor = layout
                .hit_test(Point::new(mid_x as f64, mid_y as f64))
                .unwrap();
            black_box(layout.cursor_to_byte_index(&cursor));
        });
    });

    // Full selection pipeline: hit × 2 → byte indices → selection_geometry_with.
    group.bench_function("hit_then_selection", |b| {
        let mid_x = size.width as f32 / 2.0;
        let mid_y = size.height as f32 / 2.0;
        b.iter(|| {
            let c1 = layout.hit_test(Point::new(0.0, 0.0)).unwrap();
            let c2 = layout
                .hit_test(Point::new(mid_x as f64, mid_y as f64))
                .unwrap();
            let start = layout.cursor_to_byte_index(&c1);
            let end = layout.cursor_to_byte_index(&c2);
            let selection = layout.selection_from_byte_range(start, end);
            let mut count = 0u32;
            layout.selection_geometry_with(&selection, |_x0, _y0, _x1, _y1| count += 1);
            black_box(count);
        });
    });

    group.finish();
}

// =============================================================================
// Group 5: Selection geometry — highlight rect computation
// =============================================================================

fn bench_selection(c: &mut Criterion) {
    ensure_font();
    let mut group = c.benchmark_group("selection");

    let mut layout = TextLayout::new();
    layout.set_text(PARAGRAPH, default_attrs(), None);
    layout.set_size(400.0, f32::MAX);
    let len = PARAGRAPH.len();

    // Small selection: single word (~20 bytes)
    group.bench_function("select_word", |b| {
        b.iter(|| {
            let selection = layout.selection_from_byte_range(10, 30);
            let mut count = 0u32;
            layout.selection_geometry_with(&selection, |_x0, _y0, _x1, _y1| count += 1);
            black_box(count);
        });
    });

    // Cross-line selection: ~half the text
    group.bench_function("select_half", |b| {
        b.iter(|| {
            let selection = layout.selection_from_byte_range(0, len / 2);
            let mut count = 0u32;
            layout.selection_geometry_with(&selection, |_x0, _y0, _x1, _y1| count += 1);
            black_box(count);
        });
    });

    // Full selection: entire text
    group.bench_function("select_full", |b| {
        b.iter(|| {
            let selection = layout.selection_from_byte_range(0, len);
            let mut count = 0u32;
            layout.selection_geometry_with(&selection, |_x0, _y0, _x1, _y1| count += 1);
            black_box(count);
        });
    });

    // Direct cursor-to-selection (no byte-index round-trip).
    let size = layout.size();
    let c_start = layout.hit_test(Point::new(0.0, 0.0)).unwrap();
    let c_end = layout
        .hit_test(Point::new(size.width, size.height / 2.0))
        .unwrap();
    group.bench_function("select_from_cursors", |b| {
        b.iter(|| {
            let selection = layout.selection(c_start, c_end);
            let mut count = 0u32;
            layout.selection_geometry_with(&selection, |_x0, _y0, _x1, _y1| count += 1);
            black_box(count);
        });
    });

    group.finish();
}

// =============================================================================
// Group 6: Visual lines — Parley line iteration + metrics
// =============================================================================

fn bench_visual_lines(c: &mut Criterion) {
    ensure_font();
    let mut group = c.benchmark_group("visual_lines");

    let mut layout = TextLayout::new();
    layout.set_text(PARAGRAPH, default_attrs(), None);
    layout.set_size(400.0, f32::MAX);

    group.bench_function("iterate_lines", |b| {
        let count = layout.visual_line_count();
        b.iter(|| {
            for i in 0..count {
                black_box(layout.visual_line_y(i));
            }
        });
    });

    group.bench_function("iterate_visual_lines", |b| {
        let count = layout.visual_line_count();
        b.iter(|| {
            for i in 0..count {
                black_box(layout.visual_line_y(i));
                black_box(layout.visual_line_text_range(i));
            }
        });
    });

    group.finish();
}

// =============================================================================
// Group 7: Stress tests — large documents and mixed scripts
// =============================================================================

fn bench_stress(c: &mut Criterion) {
    ensure_font();
    let mut group = c.benchmark_group("stress");
    group.sample_size(10);

    // 50 paragraphs of Latin text
    let big = (0..50).map(|_| PARAGRAPH).collect::<Vec<_>>().join("\n");
    group.bench_function("large_document_50para", |b| {
        b.iter(|| {
            let mut layout = TextLayout::new();
            layout.set_text(&big, default_attrs(), None);
            layout.set_size(600.0, f32::MAX);
            black_box(&layout);
        });
    });

    // Combined: Latin + Arabic + BiDi + Emoji
    let combined = format!(
        "{}\n{}\n{}\n{}",
        PARAGRAPH,
        ARABIC.repeat(5),
        MIXED_BIDI.repeat(5),
        EMOJI.repeat(5),
    );
    group.bench_function("combined_scripts", |b| {
        b.iter(|| {
            let mut layout = TextLayout::new();
            layout.set_text(&combined, default_attrs(), None);
            layout.set_size(500.0, f32::MAX);
            black_box(&layout);
        });
    });

    // Layout-heavy: long wrapping lines (stress line breaking)
    let heavy = "This is a very long line that will wrap multiple times and stress \
        the line breaking optimization through intensive layout processing with \
        comprehensive reflow testing across word boundaries. "
        .repeat(30);
    group.bench_function("layout_heavy_wrap", |b| {
        b.iter(|| {
            let mut layout = TextLayout::new();
            layout.set_text(&heavy, default_attrs(), None);
            layout.set_size(500.0, f32::MAX);
            black_box(&layout);
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_layout_creation,
    bench_styled_layout,
    bench_line_breaking,
    bench_hit_testing,
    bench_selection,
    bench_visual_lines,
    bench_stress,
);
criterion_main!(benches);
