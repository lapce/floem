//! Benchmarks comparing `start_layout_cols()` (standalone, no provider)
//! against `layout_cols()` (requires TextLayoutProvider with rope lookups).
//!
//! The key optimisation: `start_layout_cols` skips the expensive end-column
//! adjustment that calls `before_phantom_col`, `rope_text()`, `offset_of_line`,
//! `line_end_col`, and `slice_to_cow` per visual line.

use std::hint::black_box;
use std::sync::{Arc, Once};

use criterion::{Criterion, criterion_group, criterion_main};
use floem::views::editor::{
    layout::TextLayoutLine,
    phantom_text::PhantomTextLine,
    visual_line::{ResolvedWrap, TextLayoutProvider},
};
use floem_editor_core::buffer::rope_text::RopeTextVal;
use floem_renderer::text::{
    Attrs, AttrsList, FONT_CONTEXT, FamilyOwned, TextLayout, Wrap,
};
use lapce_xi_rope::Rope;

// ============================================================================
// Font setup
// ============================================================================

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

// ============================================================================
// Minimal TextLayoutProvider for benchmarking layout_cols
// ============================================================================

struct BenchProvider {
    rope: Rope,
}

impl TextLayoutProvider for BenchProvider {
    fn text(&self) -> Rope {
        self.rope.clone()
    }

    fn rope_text(&self) -> RopeTextVal {
        RopeTextVal::new(self.rope.clone())
    }

    fn new_text_layout(
        &self,
        _line: usize,
        _font_size: usize,
        _wrap: ResolvedWrap,
    ) -> Arc<TextLayoutLine> {
        unimplemented!("not needed for layout_cols benchmark")
    }

    fn before_phantom_col(&self, _line: usize, col: usize) -> usize {
        col // no phantom text
    }

    fn has_multiline_phantom(&self) -> bool {
        false
    }
}

// ============================================================================
// Helpers
// ============================================================================

fn make_layout_line(text: &str, wrap_width: f32) -> TextLayoutLine {
    let family = vec![FamilyOwned::Name("DejaVu Serif".into())];
    let attrs = Attrs::new().font_size(12.0).family(&family);
    let attrs_list = AttrsList::new(attrs);

    let mut text_layout = TextLayout::new();
    text_layout.set_wrap(Wrap::Word);
    text_layout.set_text(text, attrs_list, None);
    text_layout.set_size(wrap_width, f32::MAX);

    TextLayoutLine {
        extra_style: Vec::new(),
        text: text_layout,
        whitespaces: None,
        indent: 0.0,
        phantom_text: PhantomTextLine::default(),
    }
}

// ============================================================================
// Benchmarks
// ============================================================================

fn bench_layout_cols(c: &mut Criterion) {
    ensure_font();

    let line_text = "The quick brown fox jumps over the lazy dog and keeps running ";
    // Create a line that wraps into ~4 visual lines at 200px.
    let layout_line = make_layout_line(line_text, 200.0);
    let provider = BenchProvider {
        rope: Rope::from(line_text),
    };

    let vline_count = layout_line.text.visual_line_count();
    let mut group = c.benchmark_group("editor_layout_cols");

    group.bench_function("start_layout_cols_standalone", |b| {
        b.iter(|| {
            let starts: Vec<_> = layout_line.start_layout_cols().collect();
            black_box(starts);
        });
    });

    group.bench_function("layout_cols_full", |b| {
        b.iter(|| {
            let cols: Vec<_> = layout_line.layout_cols(&provider, 0).collect();
            black_box(cols);
        });
    });

    // Also measure just iterating without collect (nth access pattern).
    group.bench_function("start_layout_cols_nth_last", |b| {
        b.iter(|| {
            let last = layout_line.start_layout_cols().last();
            black_box(last);
        });
    });

    group.bench_function("layout_cols_nth_last", |b| {
        b.iter(|| {
            let last = layout_line.layout_cols(&provider, 0).last();
            black_box(last);
        });
    });

    // Simulate the old start_layout_cols: layout_cols().map(|(s, _)| s).
    group.bench_function("layout_cols_map_start_only", |b| {
        b.iter(|| {
            let starts: Vec<_> = layout_line
                .layout_cols(&provider, 0)
                .map(|(s, _)| s)
                .collect();
            black_box(starts);
        });
    });

    println!(
        "Visual line count for benchmark text: {}",
        vline_count
    );

    group.finish();
}

// Benchmark with a longer wrapping line (more visual lines).
fn bench_layout_cols_long(c: &mut Criterion) {
    ensure_font();

    let line_text = "Lorem ipsum dolor sit amet, consectetur adipiscing elit. \
        Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. \
        Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris \
        nisi ut aliquip ex ea commodo consequat. ";
    let layout_line = make_layout_line(line_text, 150.0);
    let provider = BenchProvider {
        rope: Rope::from(line_text),
    };

    let vline_count = layout_line.text.visual_line_count();
    let mut group = c.benchmark_group("editor_layout_cols_long");

    group.bench_function("start_layout_cols_standalone", |b| {
        b.iter(|| black_box(layout_line.start_layout_cols().collect::<Vec<_>>()));
    });

    group.bench_function("layout_cols_full", |b| {
        b.iter(|| black_box(layout_line.layout_cols(&provider, 0).collect::<Vec<_>>()));
    });

    group.bench_function("layout_cols_map_start_only", |b| {
        b.iter(|| {
            black_box(
                layout_line
                    .layout_cols(&provider, 0)
                    .map(|(s, _)| s)
                    .collect::<Vec<_>>(),
            )
        });
    });

    println!(
        "Visual line count for long benchmark text: {}",
        vline_count
    );

    group.finish();
}

criterion_group!(benches, bench_layout_cols, bench_layout_cols_long);
criterion_main!(benches);
