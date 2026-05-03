use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, BTreeSet, VecDeque},
    rc::Rc,
};

use floem_reactive::{RwSignal, SignalGet, SignalUpdate};
use peniko::{
    Color,
    kurbo::{Point, Rect, Size, Stroke},
};

use crate::{
    ElementId,
    context::PaintCx,
    event::PaintPresentInfo,
    platform::{Duration, Instant},
    prelude::*,
    style::Position,
    text::{Alignment, Attrs, AttrsList, FamilyOwned, FontWeight, TextLayout},
    view::View,
    views::Overlay,
};

const SAMPLE_COUNT: usize = 90;
const GRAPH_COUNT: usize = 44;
const HUD_WIDTH: f64 = 268.0;
const HUD_HEADER_HEIGHT: f64 = 58.0;
const HUD_LAYER_HEIGHT: f64 = 48.0;
const HUD_MAX_HEIGHT: f64 = 360.0;

#[derive(Clone)]
pub(crate) struct Hud {
    inner: Rc<HudInner>,
}

struct HudInner {
    visible: RwSignal<bool>,
    element_id: Cell<Option<ElementId>>,
    content_element_id: Cell<Option<ElementId>>,
    reports: RefCell<BTreeMap<u32, LayerReport>>,
    metrics: RwSignal<HudMetrics>,
}

#[derive(Clone, Debug)]
struct HudMetrics {
    layers: Vec<LayerMetrics>,
}

impl Default for HudMetrics {
    fn default() -> Self {
        Self { layers: Vec::new() }
    }
}

#[derive(Clone, Debug)]
struct LayerReport {
    layer_id: u32,
    name: String,
    last_presented_at: Option<Instant>,
    samples: VecDeque<Duration>,
    present_count: u64,
    missed_deadlines: u64,
    target_interval: Option<Duration>,
}

impl LayerReport {
    fn new(layer_id: u32) -> Self {
        Self {
            layer_id,
            name: format!("Layer {layer_id}"),
            last_presented_at: None,
            samples: VecDeque::with_capacity(SAMPLE_COUNT),
            present_count: 0,
            missed_deadlines: 0,
            target_interval: None,
        }
    }
}

#[derive(Clone, Debug)]
struct LayerMetrics {
    layer_id: u32,
    name: String,
    fps: f64,
    avg_ms: f64,
    min_ms: f64,
    max_ms: f64,
    frame_ms: [f32; GRAPH_COUNT],
    frame_count: u64,
    missed_deadlines: u64,
    missed_presents: u64,
    target_ms: Option<f64>,
}

impl Default for LayerMetrics {
    fn default() -> Self {
        Self {
            layer_id: 0,
            name: String::new(),
            fps: 0.0,
            avg_ms: 0.0,
            min_ms: 0.0,
            max_ms: 0.0,
            frame_ms: [0.0; GRAPH_COUNT],
            frame_count: 0,
            missed_deadlines: 0,
            missed_presents: 0,
            target_ms: None,
        }
    }
}

impl Hud {
    pub(crate) fn new() -> Self {
        Self {
            inner: Rc::new(HudInner {
                visible: RwSignal::new(false),
                element_id: Cell::new(None),
                content_element_id: Cell::new(None),
                reports: RefCell::new(BTreeMap::new()),
                metrics: RwSignal::new(HudMetrics::default()),
            }),
        }
    }

    pub(crate) fn toggle(&self) {
        let next_visible = !self.inner.visible.get_untracked();
        self.inner.visible.set(next_visible);
        if next_visible {
            self.inner
                .metrics
                .set(metrics_from_reports(&self.inner.reports.borrow()));
        }
    }

    pub(crate) fn record_present(&self, info: &PaintPresentInfo) {
        let self_element_id = self.inner.element_id.get();
        let self_content_element_id = self.inner.content_element_id.get();
        let is_hud_layer = |source_element_id: Option<ElementId>| {
            source_element_id == self_element_id || source_element_id == self_content_element_id
        };
        let active_layer_ids = info
            .active_layers
            .iter()
            .filter(|layer| !is_hud_layer(layer.source_element_id))
            .map(|layer| layer.layer_id)
            .collect::<BTreeSet<_>>();
        let presented_layers = info
            .layers
            .iter()
            .filter(|layer| !is_hud_layer(layer.source_element_id))
            .collect::<Vec<_>>();

        let presented_at = info.presented_at;
        {
            let mut reports = self.inner.reports.borrow_mut();
            reports.retain(|layer_id, _| active_layer_ids.contains(layer_id));
            for layer in presented_layers {
                let report = reports
                    .entry(layer.layer_id)
                    .or_insert_with(|| LayerReport::new(layer.layer_id));
                if let Some(debug_name) = &layer.debug_name {
                    if !debug_name.is_empty() {
                        report.name = debug_name.clone();
                    }
                }
                report.present_count += 1;
                report.target_interval = layer.target_frame_interval;
                if layer.missed_deadline {
                    report.missed_deadlines += 1;
                }
                if let Some(previous) = report.last_presented_at.replace(presented_at) {
                    let dt = presented_at.saturating_duration_since(previous);
                    if !dt.is_zero() {
                        if report.samples.len() == SAMPLE_COUNT {
                            report.samples.pop_front();
                        }
                        report.samples.push_back(dt);
                    }
                }
            }
        }

        if self.inner.visible.get_untracked() {
            self.inner
                .metrics
                .set(metrics_from_reports(&self.inner.reports.borrow()));
        }
    }

    pub(crate) fn view(&self) -> Overlay {
        let visible = self.inner.visible;
        let metrics = self.inner.metrics;
        let hud_canvas = canvas(move |cx, size| {
            if visible.get() {
                draw_hud(cx, size, &metrics.get());
            }
        })
        .style(move |s| {
            let metrics = metrics.get();
            s.width(HUD_WIDTH)
                .height(hud_height_for_layer_count(metrics.layers.len()))
        });
        self.inner
            .content_element_id
            .set(Some(hud_canvas.id().get_element_id()));
        let stack = hud_canvas.scroll().style(move |s| {
            s.position(Position::Absolute)
                .inset_top(12.0)
                .inset_right(12.0)
                .width(HUD_WIDTH)
                .max_height(HUD_MAX_HEIGHT)
                .z_index(1000)
                .pointer_events_none()
                .wants_layer(true)
        });
        self.inner.element_id.set(Some(stack.id().get_element_id()));
        Overlay::new(stack)
    }
}

fn hud_height_for_layer_count(layer_count: usize) -> f64 {
    if layer_count == 0 {
        HUD_HEADER_HEIGHT
    } else {
        HUD_HEADER_HEIGHT + 4.0 + layer_count as f64 * (HUD_LAYER_HEIGHT + 4.0) - 4.0
    }
}

fn metrics_from_reports(reports: &BTreeMap<u32, LayerReport>) -> HudMetrics {
    let mut layers = reports
        .values()
        .map(metrics_from_report)
        .collect::<Vec<_>>();
    layers.sort_by_key(|layer| layer.layer_id);
    HudMetrics { layers }
}

fn metrics_from_report(report: &LayerReport) -> LayerMetrics {
    let target_ms = report
        .target_interval
        .map(|interval| interval.as_secs_f64() * 1000.0);
    let mut metrics = LayerMetrics {
        layer_id: report.layer_id,
        name: report.name.clone(),
        frame_count: report.present_count,
        missed_deadlines: report.missed_deadlines,
        target_ms,
        ..LayerMetrics::default()
    };

    let mut min_ms = f64::INFINITY;
    let mut max_ms: f64 = 0.0;
    let mut total_ms = 0.0;
    let missed_present_threshold_ms = target_ms.map(|ms| ms * 1.5).unwrap_or(25.0);
    for sample in &report.samples {
        let ms = sample.as_secs_f64() * 1000.0;
        min_ms = min_ms.min(ms);
        max_ms = max_ms.max(ms);
        total_ms += ms;
        if ms > missed_present_threshold_ms {
            metrics.missed_presents += 1;
        }
    }
    if !report.samples.is_empty() {
        metrics.avg_ms = total_ms / report.samples.len() as f64;
        metrics.fps = 1000.0 / metrics.avg_ms.max(0.001);
        metrics.min_ms = min_ms;
        metrics.max_ms = max_ms;
    }

    let graph_offset = report.samples.len().saturating_sub(GRAPH_COUNT);
    for (index, sample) in report.samples.iter().skip(graph_offset).enumerate() {
        metrics.frame_ms[index] = (sample.as_secs_f64() * 1000.0) as f32;
    }
    metrics
}

fn draw_hud(cx: &mut PaintCx<'_>, size: Size, metrics: &HudMetrics) {
    draw_hud_header(
        cx,
        Size::new(size.width, HUD_HEADER_HEIGHT.min(size.height)),
        metrics,
    );
    let mut y = HUD_HEADER_HEIGHT + 4.0;
    for layer in &metrics.layers {
        draw_layer_canvas(
            cx,
            Rect::new(0.0, y, size.width, y + HUD_LAYER_HEIGHT),
            layer,
        );
        y += HUD_LAYER_HEIGHT + 4.0;
    }
}

fn draw_hud_header(cx: &mut PaintCx<'_>, size: Size, metrics: &HudMetrics) {
    let bounds = Rect::ZERO.with_size(size);
    let panel = bounds.to_rounded_rect(11.0);
    cx.painter
        .fill(panel, Color::from_rgba8(6, 10, 8, 218))
        .draw();
    cx.painter
        .stroke(
            panel,
            &Stroke::new(1.0),
            Color::from_rgba8(113, 255, 167, 82),
        )
        .draw();

    draw_text(
        cx,
        Point::new(10.0, 7.0),
        "FLOEM HUD",
        9.0,
        FontWeight::BOLD,
        Color::from_rgba8(122, 255, 176, 235),
    );
    draw_monospace_text(
        cx,
        Point::new(10.0, 24.0),
        "dl   = missed compositor deadline",
        9.0,
        FontWeight::NORMAL,
        Color::from_rgba8(184, 255, 205, 210),
    );
    draw_monospace_text(
        cx,
        Point::new(10.0, 38.0),
        "miss = missed present cadence",
        9.0,
        FontWeight::NORMAL,
        Color::from_rgba8(184, 255, 205, 210),
    );
    if metrics.layers.is_empty() {
        draw_text(
            cx,
            Point::new(10.0, 47.0),
            "Waiting for layer presents",
            9.0,
            FontWeight::NORMAL,
            Color::from_rgba8(184, 255, 205, 220),
        );
        return;
    }
}

fn draw_layer_canvas(cx: &mut PaintCx<'_>, bounds: Rect, metrics: &LayerMetrics) {
    cx.painter
        .fill(
            bounds.to_rounded_rect(9.0),
            Color::from_rgba8(6, 10, 8, 218),
        )
        .draw();
    cx.painter
        .stroke(
            bounds.to_rounded_rect(9.0),
            &Stroke::new(1.0),
            Color::from_rgba8(113, 255, 167, 62),
        )
        .draw();
    draw_layer_report(
        cx,
        Rect::new(
            bounds.x0 + 4.0,
            bounds.y0 + 3.0,
            bounds.x1 - 4.0,
            bounds.y1 - 3.0,
        ),
        metrics,
    );
}

fn draw_layer_report(cx: &mut PaintCx<'_>, rect: Rect, metrics: &LayerMetrics) {
    let graph_rect = Rect::new(rect.x1 - 76.0, rect.y0 + 7.0, rect.x1 - 7.0, rect.y1 - 7.0);
    let text_width = (graph_rect.x0 - rect.x0 - 13.0).max(1.0);
    cx.painter
        .fill(
            rect.to_rounded_rect(7.0),
            Color::from_rgba8(11, 24, 18, 168),
        )
        .draw();
    let mut name = metrics.name.chars().take(20).collect::<String>();
    if name.len() < metrics.name.len() {
        name.push_str("...");
    }
    draw_text_with_width(
        cx,
        Point::new(rect.x0 + 7.0, rect.y0 + 4.0),
        &name,
        9.0,
        FontWeight::BOLD,
        Color::from_rgba8(122, 255, 176, 235),
        text_width,
    );
    draw_text_with_width(
        cx,
        Point::new(rect.x0 + 7.0, rect.y0 + 17.0),
        &format!("{:>5.1} FPS", metrics.fps),
        15.0,
        FontWeight::BOLD,
        Color::from_rgb8(226, 255, 235),
        76.0,
    );
    let target = metrics
        .target_ms
        .map(|ms| format!("{ms:.1}ms"))
        .unwrap_or_else(|| "--".to_owned());
    draw_text_with_width(
        cx,
        Point::new(rect.x0 + 86.0, rect.y0 + 18.0),
        &format!("{:>4.1}ms", metrics.avg_ms),
        9.0,
        FontWeight::BOLD,
        Color::from_rgba8(184, 255, 205, 230),
        text_width - 79.0,
    );
    draw_text_with_width(
        cx,
        Point::new(rect.x0 + 7.0, rect.y0 + 32.0),
        &format!(
            "target {target} miss {} dl {} #{}",
            metrics.missed_presents, metrics.missed_deadlines, metrics.frame_count
        ),
        8.0,
        FontWeight::NORMAL,
        Color::from_rgba8(135, 176, 153, 230),
        text_width,
    );
    draw_graph(cx, graph_rect, metrics);
}

fn draw_graph(cx: &mut PaintCx<'_>, rect: Rect, metrics: &LayerMetrics) {
    cx.painter
        .fill(
            rect.to_rounded_rect(4.0),
            Color::from_rgba8(13, 32, 22, 190),
        )
        .draw();

    if let Some(target_ms) = metrics.target_ms {
        let target_y = rect.y1 - (target_ms / 35.0 * rect.height()).min(rect.height());
        cx.painter
            .stroke(
                Rect::new(rect.x0, target_y, rect.x1, target_y),
                &Stroke::new(1.0),
                Color::from_rgba8(210, 240, 126, 88),
            )
            .draw();
    }

    let target_ms = metrics.target_ms.unwrap_or(16.666);
    let yellow_threshold = target_ms * 1.1;
    let red_threshold = target_ms * 1.5;
    let bar_width = rect.width() / GRAPH_COUNT as f64;
    for (index, ms) in metrics.frame_ms.iter().enumerate() {
        if *ms <= 0.0 {
            continue;
        }
        let ms = f64::from(*ms);
        let normalized = (ms / 35.0).min(1.0);
        let height = (rect.height() * normalized).max(1.0);
        let x0 = rect.x0 + index as f64 * bar_width + 0.5;
        let x1 = (x0 + bar_width - 1.0).min(rect.x1);
        let color = if ms <= yellow_threshold {
            Color::from_rgba8(92, 255, 151, 218)
        } else if ms <= red_threshold {
            Color::from_rgba8(242, 220, 105, 230)
        } else {
            Color::from_rgba8(255, 103, 92, 235)
        };
        cx.painter
            .fill(Rect::new(x0, rect.y1 - height, x1, rect.y1), color)
            .draw();
    }
}

fn draw_text(
    cx: &mut PaintCx<'_>,
    origin: Point,
    text: &str,
    font_size: f32,
    weight: FontWeight,
    color: Color,
) {
    draw_text_with_width(cx, origin, text, font_size, weight, color, 220.0);
}

fn draw_monospace_text(
    cx: &mut PaintCx<'_>,
    origin: Point,
    text: &str,
    font_size: f32,
    weight: FontWeight,
    color: Color,
) {
    let family = [FamilyOwned::Monospace];
    let mut layout = TextLayout::new_with_text(
        text,
        AttrsList::new(
            Attrs::new()
                .family(&family)
                .font_size(font_size)
                .weight(weight)
                .color(color),
        ),
        Some(Alignment::Start),
    );
    layout.set_size(220.0, 24.0);
    layout.draw(cx, origin);
}

fn draw_text_with_width(
    cx: &mut PaintCx<'_>,
    origin: Point,
    text: &str,
    font_size: f32,
    weight: FontWeight,
    color: Color,
    width: f64,
) {
    let mut layout = TextLayout::new_with_text(
        text,
        AttrsList::new(
            Attrs::new()
                .font_size(font_size)
                .weight(weight)
                .color(color),
        ),
        Some(Alignment::Start),
    );
    layout.set_size(width.max(1.0) as f32, 24.0);
    layout.draw(cx, origin);
}
