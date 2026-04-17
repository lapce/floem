use std::{cell::RefCell, collections::HashMap, rc::Rc, time::Duration, time::Instant};

use floem::imaging::record::Scene;
use floem::imaging::{ImageBrush, Painter, SceneImage};
use floem::peniko::color::Oklch;
use floem::{
    ViewId,
    context::{EventCx, LayoutChanged, LayoutChangedListener, PaintCx},
    easing::{Linear, Spring},
    event::{
        CustomEvent, DragConfig, DragEvent, DragSourceEvent, Event, EventPropagation,
        PointerCaptureEvent,
    },
    kurbo::{Circle, Point, Rect, Shape, Size, Stroke},
    peniko::{
        Brush, Compose, Extend, Gradient, ImageQuality, Mix,
        color::{
            AlphaColor,
            ColorSpaceTag::{self, LinearSrgb},
            Hsl, Hwb, Srgb,
        },
    },
    prelude::*,
    reactive::{Effect, UpdaterEffect},
    style::{DirectTransition, Transition},
    ui_events::pointer::{PointerButtonEvent, PointerEvent},
};
use palette::css;

use crate::form::{form, form_item};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct CheckerboardCacheKey {
    cell_size_bits: u64,
    light_color_bits: [u32; 4],
    dark_color_bits: [u32; 4],
}

thread_local! {
    static CHECKERBOARD_TILE_IMAGE_CACHE: RefCell<HashMap<CheckerboardCacheKey, SceneImage>> =
        RefCell::new(HashMap::new());
}

pub fn canvas_view() -> impl IntoView {
    let rounded = RwSignal::new(true);
    let color = RwSignal::new(css::AQUA);

    let border_radius = Rc::new(RefCell::new(DirectTransition::new(
        32.,
        Some(Transition::new(500.millis(), Spring::snappy())),
    )));

    let border_radius_ = border_radius.clone();
    Effect::new(move |_| {
        let rounded = rounded.get();
        border_radius_
            .borrow_mut()
            .transition_to(if rounded { 32. } else { 0. });
    });

    form((
        form_item(
            "Simple Canvas:",
            Stack::horizontal((
                canvas(move |cx, size| {
                    rounded.track();
                    let now = Instant::now();
                    if border_radius.borrow_mut().step(&now) {
                        cx.window_state.schedule_paint(cx.target_id);
                    }
                    let brush = Brush::Solid(color.get());
                    cx.painter
                        .fill(
                            Rect::ZERO
                                .with_size(size)
                                .to_rounded_rect(border_radius.borrow().get()),
                            &brush,
                        )
                        .draw();
                })
                .style(|s| s.size(300, 100)),
                Button::new("toggle rounded corners")
                    .action(move || rounded.update(|s| *s = !*s))
                    .style(|s| s.height(30)),
            ))
            .style(|s| s.gap(10).items_center()),
        ),
        form_item(
            "Complex Canvas:",
            color_picker(color).style(|s| s.size(500, 500)),
        ),
    ))
}

fn color_picker(color: RwSignal<Color>) -> impl IntoView {
    let hue_opocity = Stack::vertical((
        HuePicker::new(move || color.get())
            .on_change(move |c| color.set(c))
            .style(|s| s.size_full().border(1).border_radius(8)),
        OpacityPicker::new(move || color.get())
            .on_change(move |c| color.set(c))
            .style(|s| s.size_full().border(1).border_radius(8)),
    ))
    .style(|s| s.gap(5).size_full())
    .debug_name("hue opacity");

    let final_hue_op = Stack::horizontal((
        canvas(move |cx, size: Size| {
            let rect_path = Rect::ZERO.with_size(size).to_rounded_rect(8.);

            let base_color = color.get();

            draw_checkerboard(cx, size, &rect_path, 8.0, css::WHITE, css::LIGHT_GRAY);

            let brush = Brush::Solid(base_color);
            cx.painter.fill(rect_path, &brush).draw();
        })
        .style(move |s| s.height_full().aspect_ratio(1.).border(1).border_radius(8))
        .debug_name("final color"),
        hue_opocity,
    ))
    .style(|s| s.gap(5).width_full().height_pct(20.))
    .debug_name("final and hue/opacity");

    Stack::vertical((
        two_d_picker(color).style(|s| s.border(1).border_radius(8)),
        final_hue_op,
    ))
    .style(|s| s.gap(20))
    .debug_name("2d and others")
}

fn two_d_picker(color: RwSignal<Color>) -> impl IntoView {
    SatValuePicker::new(move || color.get())
        .on_change(move |c| color.set(c))
        .style(|s| s.width_full().aspect_ratio(3. / 2.))
        .debug_name("2d picker")
}

fn draw_checkerboard(
    cx: &mut PaintCx,
    size: Size,
    clip_path: &impl Shape,
    cell_size: f64,
    light_color: Color,
    dark_color: Color,
) {
    cx.painter.with_fill_clip(clip_path.to_path(0.1), |p| {
        let brush = checkerboard_tile_brush(cell_size, light_color, dark_color);
        p.fill(Rect::ZERO.with_size(size), &brush).draw();
    });
}

fn checkerboard_cache_key(
    cell_size: f64,
    light_color: Color,
    dark_color: Color,
) -> CheckerboardCacheKey {
    CheckerboardCacheKey {
        cell_size_bits: cell_size.to_bits(),
        light_color_bits: light_color.components.map(f32::to_bits),
        dark_color_bits: dark_color.components.map(f32::to_bits),
    }
}

fn checkerboard_tile_brush(
    cell_size: f64,
    light_color: Color,
    dark_color: Color,
) -> ImageBrush {
    let image = cached_checkerboard_tile_image(cell_size, light_color, dark_color);
    ImageBrush::new(image)
        .with_extend(Extend::Repeat)
        .with_quality(ImageQuality::Low)
}

fn cached_checkerboard_tile_image(
    cell_size: f64,
    light_color: Color,
    dark_color: Color,
) -> SceneImage {
    let key = checkerboard_cache_key(cell_size, light_color, dark_color);
    CHECKERBOARD_TILE_IMAGE_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        cache
            .entry(key)
            .or_insert_with(|| build_checkerboard_tile_image(cell_size, light_color, dark_color))
            .clone()
    })
}

fn build_checkerboard_tile_image(
    cell_size: f64,
    light_color: Color,
    dark_color: Color,
) -> SceneImage {
    let tile_size = (cell_size * 2.0).max(1.0).round();
    let mut scene = Scene::new();
    let mut painter = Painter::new(&mut scene);
    let dark_brush = Brush::Solid(dark_color);
    let light_brush = Brush::Solid(light_color);

    painter
        .fill(Rect::new(0.0, 0.0, tile_size, tile_size), &light_brush)
        .draw();
    painter
        .fill(Rect::new(0.0, 0.0, cell_size, cell_size), &dark_brush)
        .draw();
    painter
        .fill(
            Rect::new(cell_size, cell_size, tile_size, tile_size),
            &dark_brush,
        )
        .draw();

    SceneImage::new(scene, tile_size as u32, tile_size as u32)
}

fn build_sat_value_field(size: Size, hue: f32) -> Rc<Scene> {
    let rect_path = Rect::ZERO.with_size(size).to_rounded_rect(8.);
    let mut scene = Scene::new();
    let mut p = Painter::new(&mut scene);
    {
        let white = Brush::Solid(css::WHITE);
        p.fill(rect_path, &white).draw();

        let sat_gradient: Brush = Gradient::new_linear(Point::ZERO, Point::new(size.width, 0.0))
            .with_stops([
                (0.0, css::WHITE),
                (1.0, AlphaColor::<Hsl>::new([hue, 100., 50., 1.]).convert()),
            ])
            .with_interpolation_cs(ColorSpaceTag::LinearSrgb)
            .into();
        p.fill(rect_path, &sat_gradient).draw();

        let val_gradient: Brush = Gradient::new_linear(Point::ZERO, Point::new(0.0, size.height))
            .with_stops([(0.0, Color::from_rgba8(0, 0, 0, 0)), (1.0, css::BLACK)])
            .with_interpolation_cs(ColorSpaceTag::LinearSrgb)
            .into();
        let group = floem::imaging::GroupRef::new()
            .with_clip(floem::imaging::ClipRef::fill(rect_path))
            .with_composite(floem::imaging::Composite::new(
                floem::peniko::BlendMode {
                    mix: Mix::Multiply,
                    compose: Compose::SrcOver,
                },
                1.0,
            ));
        p.with_group(group, |painter| {
            painter.fill(rect_path, &val_gradient).draw();
        });
    }
    Rc::new(scene)
}

fn build_hue_picker_gradient(size: Size) -> Rc<Scene> {
    let rect_path = Rect::ZERO.with_size(size).to_rounded_rect(8.);
    let mut scene = Scene::new();
    let mut p = Painter::new(&mut scene);
    {
        let hue_gradient: Brush = Gradient::new_linear(
            Point::new(0.0, size.height / 2.0),
            Point::new(size.width, size.height / 2.0),
        )
        .with_stops([
            (0.0, AlphaColor::<Oklch>::new([0.7, 0.3, 0.0, 1.0])),
            (1.0, AlphaColor::<Oklch>::new([0.7, 0.3, 360.0, 1.0])),
        ])
        .with_hue_direction(floem::peniko::color::HueDirection::Longer)
        .with_interpolation_cs(ColorSpaceTag::Oklch)
        .into();

        p.fill(rect_path, &hue_gradient).draw();
    }
    Rc::new(scene)
}

pub struct SatValuePicker {
    id: ViewId,
    size: Size,
    current_color: AlphaColor<Hwb>,
    on_change: Option<Box<dyn Fn(Color)>>,
    point: Point,
    retained_draw: Option<Rc<Scene>>,
    retained_draw_hue_bits: Option<u32>,
}
impl SatValuePicker {
    pub fn new(color: impl Fn() -> Color + 'static) -> Self {
        let id = ViewId::new();
        id.register_listener(LayoutChanged::listener_key());
        let color = UpdaterEffect::new(color, move |c| id.update_state(c));
        Self {
            id,
            size: Size::ZERO,
            current_color: color.convert(),
            on_change: None,
            point: Point::ZERO,
            retained_draw: None,
            retained_draw_hue_bits: None,
        }
    }

    fn position_to_hwb(&self, pos: Point) -> AlphaColor<Hwb> {
        let hue = self.current_color.components[0];

        let s = (pos.x / self.size.width).clamp(f64::EPSILON, 1.0 - f64::EPSILON);

        let v = (1.0 - pos.y / self.size.height).clamp(f64::EPSILON, 1.0 - f64::EPSILON);

        let whiteness = (1.0 - s) * v;
        let blackness = 1.0 - v;

        let alpha = self.current_color.components[3];

        AlphaColor::<Hwb>::new([
            hue,
            (whiteness * 100.0) as f32,
            (blackness * 100.0) as f32,
            alpha,
        ])
    }

    pub fn on_change(mut self, on_change: impl Fn(Color) + 'static) -> Self {
        self.on_change = Some(Box::new(on_change));
        self
    }

    fn post_layout(&mut self, new_layout: &LayoutChanged) {
        self.size = new_layout.new_box.size();
        self.retained_draw = None;
    }

    fn set_from_point(&mut self, point: Point) -> Color {
        self.id.request_paint();
        self.current_color = self.position_to_hwb(point);
        self.point = point;
        self.current_color.convert()
    }
}
impl View for SatValuePicker {
    fn id(&self) -> ViewId {
        self.id
    }

    fn update(&mut self, _cx: &mut floem::context::UpdateCx, state: Box<dyn std::any::Any>) {
        if let Ok(color) = state.downcast::<Color>() {
            self.current_color = color.convert();
            self.id.request_paint();
        }
    }

    fn event(&mut self, cx: &mut EventCx) -> EventPropagation {
        if let Some(new_layout) = LayoutChangedListener::extract(&cx.event) {
            self.post_layout(new_layout);
        }
        let updated_color = match &cx.event {
            Event::Pointer(PointerEvent::Down(PointerButtonEvent { state, pointer, .. })) => {
                if let Some(pointer_id) = pointer.pointer_id {
                    cx.window_state.set_pointer_capture(pointer_id, self.id);
                }
                Some(self.set_from_point(state.logical_point()))
            }
            Event::PointerCapture(PointerCaptureEvent::Gained(drag)) => {
                cx.start_drag(*drag, DragConfig::new(0., Duration::ZERO, Linear), false);
                None
            }
            Event::Drag(DragEvent::Source(DragSourceEvent::Move(dme))) => {
                Some(self.set_from_point(dme.current_state.logical_point()))
            }
            Event::Drag(DragEvent::Source(DragSourceEvent::End(de))) => {
                Some(self.set_from_point(de.current_state.logical_point()))
            }
            Event::Drag(DragEvent::Source(DragSourceEvent::Cancel(dce))) => {
                Some(self.set_from_point(dce.current_state.logical_point()))
            }
            _ => {
                return EventPropagation::Continue;
            }
        };

        if let Some(color) = updated_color
            && let Some(on_change) = &self.on_change
        {
            on_change(color);
        }
        EventPropagation::Stop
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        let size = self.size;
        let rect_path = Rect::ZERO.with_size(size).to_rounded_rect(8.);
        let hue = self.current_color.components[0];
        let hue_bits = hue.to_bits();

        if self.retained_draw.is_none() || self.retained_draw_hue_bits != Some(hue_bits) {
            self.retained_draw = Some(build_sat_value_field(size, hue));
            self.retained_draw_hue_bits = Some(hue_bits);
        }

        if let Some(scene) = self.retained_draw.as_ref() {
            cx.painter.replay(scene.as_ref());
        }

        if size.width > 0.0 && size.height > 0.0 {
            // Larger indicator
            let outer_radius = 8.0;
            let inner_radius = 5.5;

            let outer = Circle::new(self.point, outer_radius);
            let inner = Circle::new(self.point, inner_radius);

            // fill center with the selected color
            let inner_fill = Brush::Solid(self.current_color.convert::<Srgb>());
            let white = Brush::Solid(css::WHITE);
            let black = Brush::Solid(css::BLACK);
            cx.painter.with_fill_clip(rect_path, |p| {
                p.fill(inner.to_path(0.1), &inner_fill).draw();
                p.stroke(outer.to_path(0.1), &Stroke::new(2.0), &white)
                    .draw();
                p.stroke(inner.to_path(0.1), &Stroke::new(1.0), &black)
                    .draw();
            });
        }
    }
}

pub struct HuePicker {
    id: ViewId,
    size: Size,
    current_color: AlphaColor<Oklch>,
    on_change: Option<Box<dyn Fn(Color)>>,
    retained_draw: Option<Rc<Scene>>,
}

impl HuePicker {
    pub fn new(color: impl Fn() -> Color + 'static) -> Self {
        let id = ViewId::new();
        id.register_listener(LayoutChanged::listener_key());
        let color = UpdaterEffect::new(color, move |c| id.update_state(c));
        Self {
            id,
            size: Size::ZERO,
            current_color: color.convert(),
            on_change: None,
            retained_draw: None,
        }
    }

    fn position_to_oklch(&self, pos: Point) -> AlphaColor<Oklch> {
        let hue = (pos.x / self.size.width * 360.0).clamp(0.0, 360.0) as f32;
        AlphaColor::<Oklch>::new([
            self.current_color.components[0], // preserve L
            self.current_color.components[1], // preserve C
            hue,
            self.current_color.components[3], // preserve alpha
        ])
    }

    pub fn on_change(mut self, on_change: impl Fn(Color) + 'static) -> Self {
        self.on_change = Some(Box::new(on_change));
        self
    }

    fn post_layout(&mut self, new_layout: &LayoutChanged) {
        self.size = new_layout.new_box.size();
        self.retained_draw = None;
    }

    fn set_from_point(&mut self, point: Point) -> Color {
        self.id.request_paint();
        self.current_color = self.position_to_oklch(point);
        self.current_color.convert()
    }
}

impl View for HuePicker {
    fn id(&self) -> ViewId {
        self.id
    }

    fn update(&mut self, _cx: &mut floem::context::UpdateCx, state: Box<dyn std::any::Any>) {
        if let Ok(color) = state.downcast::<Color>() {
            self.current_color = color.convert();
            self.id.request_paint();
        }
    }

    fn event(&mut self, cx: &mut EventCx) -> EventPropagation {
        if let Some(new_layout) = LayoutChangedListener::extract(&cx.event) {
            self.post_layout(new_layout);
        }
        let updated_color = match &cx.event {
            Event::Pointer(PointerEvent::Down(PointerButtonEvent { state, pointer, .. })) => {
                if let Some(pointer_id) = pointer.pointer_id {
                    cx.window_state.set_pointer_capture(pointer_id, self.id);
                }
                Some(self.set_from_point(state.logical_point()))
            }
            Event::PointerCapture(PointerCaptureEvent::Gained(drag)) => {
                cx.start_drag(*drag, DragConfig::new(0., Duration::ZERO, Linear), false);
                None
            }
            Event::Drag(DragEvent::Source(DragSourceEvent::Move(dme))) => {
                Some(self.set_from_point(dme.current_state.logical_point()))
            }
            Event::Drag(DragEvent::Source(DragSourceEvent::End(de))) => {
                Some(self.set_from_point(de.current_state.logical_point()))
            }
            Event::Drag(DragEvent::Source(DragSourceEvent::Cancel(dce))) => {
                Some(self.set_from_point(dce.current_state.logical_point()))
            }
            _ => {
                return EventPropagation::Continue;
            }
        };

        if let Some(color) = updated_color
            && let Some(on_change) = &self.on_change
        {
            on_change(color);
        }
        EventPropagation::Stop
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        let size = self.size;
        let rect_path = Rect::ZERO.with_size(size).to_rounded_rect(8.);

        if self.retained_draw.is_none() {
            self.retained_draw = Some(build_hue_picker_gradient(size));
        }

        if let Some(scene) = self.retained_draw.as_ref() {
            cx.painter.replay(scene.as_ref());
        }
        if size.width > 0.0 {
            let hue = self.current_color.components[2];
            let x_pos = hue as f64 / 360. * size.width;

            let indicator_width = 2.0;
            let indicator_rect = Rect::new(
                x_pos - indicator_width / 2.0,
                0.0,
                x_pos + indicator_width / 2.0,
                size.height,
            );

            let white = Brush::Solid(css::WHITE);
            let black = Brush::Solid(css::BLACK);
            cx.painter.with_fill_clip(rect_path, |p| {
                p.stroke(indicator_rect, &Stroke::new(2.0), &white).draw();
                p.fill(indicator_rect, &black).draw();
            });
        }
    }
}

pub struct OpacityPicker {
    id: ViewId,
    size: Size,
    current_color: Color,
    on_change: Option<Box<dyn Fn(Color)>>,
}

impl OpacityPicker {
    pub fn new(color: impl Fn() -> Color + 'static) -> Self {
        let id = ViewId::new();
        id.register_listener(LayoutChanged::listener_key());
        let color = UpdaterEffect::new(color, move |c| id.update_state(c));
        Self {
            id,
            size: Size::ZERO,
            current_color: color,
            on_change: None,
        }
    }

    fn position_to_alpha(&self, pos: Point) -> Color {
        let alpha = (pos.x / self.size.width).clamp(0.0, 1.0);

        self.current_color.with_alpha(alpha as f32)
    }

    pub fn on_change(mut self, on_change: impl Fn(Color) + 'static) -> Self {
        self.on_change = Some(Box::new(on_change));
        self
    }

    fn post_layout(&mut self, layout_changed: &LayoutChanged) {
        self.size = layout_changed.new_box.size();
    }

    fn set_from_point(&mut self, point: Point) -> Color {
        self.id.request_paint();
        self.current_color = self.position_to_alpha(point);
        self.current_color
    }
}

impl View for OpacityPicker {
    fn id(&self) -> ViewId {
        self.id
    }

    fn update(&mut self, _cx: &mut floem::context::UpdateCx, state: Box<dyn std::any::Any>) {
        if let Ok(color) = state.downcast::<Color>() {
            self.current_color = *color;
            self.id.request_paint();
        }
    }

    fn event(&mut self, cx: &mut EventCx) -> EventPropagation {
        if let Some(new_layout) = LayoutChangedListener::extract(&cx.event) {
            self.post_layout(new_layout);
        }
        let updated_color = match &cx.event {
            Event::Pointer(PointerEvent::Down(PointerButtonEvent { state, pointer, .. })) => {
                if let Some(pointer_id) = pointer.pointer_id {
                    cx.window_state.set_pointer_capture(pointer_id, self.id);
                }
                Some(self.set_from_point(state.logical_point()))
            }
            Event::PointerCapture(PointerCaptureEvent::Gained(drag)) => {
                cx.start_drag(*drag, DragConfig::new(0., Duration::ZERO, Linear), false);
                None
            }
            Event::Drag(DragEvent::Source(DragSourceEvent::Move(dme))) => {
                Some(self.set_from_point(dme.current_state.logical_point()))
            }
            Event::Drag(DragEvent::Source(DragSourceEvent::End(de))) => {
                Some(self.set_from_point(de.current_state.logical_point()))
            }
            Event::Drag(DragEvent::Source(DragSourceEvent::Cancel(dce))) => {
                Some(self.set_from_point(dce.current_state.logical_point()))
            }
            _ => {
                return EventPropagation::Continue;
            }
        };

        if let Some(color) = updated_color
            && let Some(on_change) = &self.on_change
        {
            on_change(color);
        }
        EventPropagation::Stop
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        let size = self.size;
        let rect_path = Rect::ZERO.with_size(size).to_rounded_rect(8.);

        draw_checkerboard(cx, size, &rect_path, 8.0, css::WHITE, css::LIGHT_GRAY);

        let opacity_gradient: Brush = Gradient::new_linear(
            Point::new(0.0, size.height / 2.0),
            Point::new(size.width, size.height / 2.0),
        )
        .with_stops([
            (0.0, self.current_color.with_alpha(0.0)),
            (1.0, self.current_color.with_alpha(1.0)),
        ])
        .with_interpolation_cs(LinearSrgb)
        .into();

        cx.painter.with_fill_clip(rect_path, |p| {
            p.fill(rect_path, &opacity_gradient).draw();

            if size.width > 0.0 {
                let alpha = self.current_color.components[3];
                let x_pos = alpha as f64 * size.width;

                let indicator_width = 2.0;
                let indicator_rect = Rect::new(
                    x_pos - indicator_width / 2.0,
                    0.0,
                    x_pos + indicator_width / 2.0,
                    size.height,
                );

                let white = Brush::Solid(css::WHITE);
                let black = Brush::Solid(css::BLACK);
                p.stroke(indicator_rect, &Stroke::new(2.0), &white).draw();
                p.fill(indicator_rect, &black).draw();
            }
        });
    }
}
