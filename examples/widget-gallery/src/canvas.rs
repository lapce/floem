use std::{cell::RefCell, rc::Rc, time::Duration, time::Instant};

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
        Brush, Compose, Gradient, Mix,
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

            draw_transparency_checkerboard(cx, size, &rect_path);

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

fn draw_transparency_checkerboard(cx: &mut PaintCx, size: Size, clip_path: &impl Shape) {
    cx.painter.with_fill_clip(clip_path.to_path(0.1), |p| {
        let cell_size = 8.0;
        let dark_color = Brush::Solid(css::LIGHT_GRAY);
        let light_color = Brush::Solid(css::WHITE);

        let cols = (size.width / cell_size).ceil() as usize;
        let rows = (size.height / cell_size).ceil() as usize;

        for row in 0..rows {
            for col in 0..cols {
                let brush = if (row + col) % 2 == 0 {
                    &dark_color
                } else {
                    &light_color
                };

                let rect = Rect::new(
                    col as f64 * cell_size,
                    row as f64 * cell_size,
                    (col + 1) as f64 * cell_size,
                    (row + 1) as f64 * cell_size,
                );

                p.fill(rect, brush).draw();
            }
        }
    });
}

pub struct SatValuePicker {
    id: ViewId,
    size: Size,
    current_color: AlphaColor<Hwb>,
    on_change: Option<Box<dyn Fn(Color)>>,
    point: Point,
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

        // base
        let white = Brush::Solid(css::WHITE);
        cx.painter.fill(rect_path, &white).draw();

        // saturation gradient
        let sat_gradient: Brush = Gradient::new_linear(Point::ZERO, Point::new(size.width, 0.0))
            .with_stops([
                (0.0, css::WHITE),
                (1.0, AlphaColor::<Hsl>::new([hue, 100., 50., 1.]).convert()),
            ])
            .with_interpolation_cs(ColorSpaceTag::LinearSrgb)
            .into();

        cx.painter.fill(rect_path, &sat_gradient).draw();

        // value gradient
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
        cx.painter.with_group(group, |p| {
            p.fill(rect_path, &val_gradient).draw();
        });

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
    current_color: AlphaColor<Hsl>,
    on_change: Option<Box<dyn Fn(Color)>>,
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
        }
    }

    fn position_to_hsl(&self, pos: Point) -> AlphaColor<Hsl> {
        let hue = (pos.x / self.size.width * 360.0).clamp(0.0, 360.0);

        let saturation = self.current_color.components[1];
        let value = self.current_color.components[2];
        let alpha = self.current_color.components[3];

        AlphaColor::<Hsl>::new([hue as f32, saturation, value, alpha])
    }

    pub fn on_change(mut self, on_change: impl Fn(Color) + 'static) -> Self {
        self.on_change = Some(Box::new(on_change));
        self
    }

    fn post_layout(&mut self, new_layout: &LayoutChanged) {
        self.size = new_layout.new_box.size();
    }

    fn set_from_point(&mut self, point: Point) -> Color {
        self.id.request_paint();
        self.current_color = self.position_to_hsl(point);
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
        let hue_gradient: Brush = Gradient::new_linear(
            Point::new(0.0, size.height / 2.0),
            Point::new(size.width, size.height / 2.0),
        )
        .with_stops([
            (0.0, AlphaColor::<Hsl>::new([0.0, 100.0, 50.0, 1.0])),
            (1.0, AlphaColor::<Hsl>::new([360.0, 100.0, 50.0, 1.0])),
        ])
        .with_hue_direction(floem::peniko::color::HueDirection::Longer)
        .with_interpolation_cs(ColorSpaceTag::Hsl)
        .into();

        cx.painter.fill(rect_path, &hue_gradient).draw();
        if size.width > 0.0 {
            let hue = self.current_color.components[0];
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

        draw_transparency_checkerboard(cx, size, &rect_path);

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
