use std::{cell::RefCell, rc::Rc, time::Instant};

use floem::{
    ElementIdExt as _, ViewId,
    context::{EventCx, LayoutChanged, LayoutChangedListener, PaintCx},
    easing::Spring,
    event::{CustomEvent, Event, EventPropagation},
    kurbo::{Affine, Circle, Point, Rect, Shape, Size, Stroke},
    peniko::{
        Gradient, Mix,
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
                        cx.window_state.schedule_paint(cx.target_id.owning_id());
                    }
                    cx.fill(
                        &Rect::ZERO
                            .with_size(size)
                            .to_rounded_rect(border_radius.borrow().get()),
                        color.get(),
                        0.,
                    );
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

            cx.fill(&rect_path, base_color, 0.);
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
    cx.push_layer(Mix::Normal, 1.0, Affine::IDENTITY, clip_path);

    let cell_size = 8.0;
    let dark_color = css::LIGHT_GRAY;
    let light_color = css::WHITE;

    let cols = (size.width / cell_size).ceil() as usize;
    let rows = (size.height / cell_size).ceil() as usize;

    for row in 0..rows {
        for col in 0..cols {
            let is_dark = (row + col) % 2 == 0;
            let color = if is_dark { dark_color } else { light_color };

            let rect = Rect::new(
                col as f64 * cell_size,
                row as f64 * cell_size,
                (col + 1) as f64 * cell_size,
                (row + 1) as f64 * cell_size,
            );

            cx.fill(&rect, color, 0.0);
        }
    }

    cx.pop_layer();
}

pub struct SatValuePicker {
    id: ViewId,
    size: Size,
    current_color: AlphaColor<Hwb>,
    on_change: Option<Box<dyn Fn(Color)>>,
    point: Point,
    track: bool,
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
            track: false,
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
}
impl View for SatValuePicker {
    fn id(&self) -> ViewId {
        self.id
    }

    fn update(&mut self, _cx: &mut floem::context::UpdateCx, state: Box<dyn std::any::Any>) {
        if let Ok(color) = state.downcast::<Color>() {
            self.current_color = color.convert();
        }
    }

    fn event(&mut self, cx: &mut EventCx) -> EventPropagation {
        if let Some(new_layout) = LayoutChangedListener::extract(&cx.event) {
            self.post_layout(new_layout);
        }
        if let Some(on_change) = &self.on_change {
            match &cx.event {
                Event::Pointer(PointerEvent::Down(PointerButtonEvent {
                    state, pointer, ..
                })) => {
                    self.current_color = self.position_to_hwb(state.logical_point());
                    self.point = state.logical_point();
                    on_change(self.current_color.convert());
                    self.track = true;
                    if let Some(pointer_id) = pointer.pointer_id {
                        cx.request_pointer_capture(pointer_id);
                    }
                }
                Event::Pointer(PointerEvent::Up(PointerButtonEvent { state, .. })) => {
                    self.current_color = self.position_to_hwb(state.logical_point());
                    self.point = state.logical_point();
                    on_change(self.current_color.convert());
                    self.track = false;
                }
                Event::Pointer(PointerEvent::Move(pu)) => {
                    if self.track {
                        self.current_color = self.position_to_hwb(pu.current.logical_point());
                        self.point = pu.current.logical_point();
                        on_change(self.current_color.convert());
                    }
                }
                _ => {
                    return EventPropagation::Continue;
                }
            }
        }
        EventPropagation::Stop
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        let size = self.size;
        let rect_path = Rect::ZERO.with_size(size).to_rounded_rect(8.);
        let hue = self.current_color.components[0];

        // base
        cx.fill(&rect_path, css::WHITE, 0.);

        // saturation gradient
        let sat_gradient = Gradient::new_linear(Point::ZERO, Point::new(size.width, 0.0))
            .with_stops([
                (0.0, css::WHITE),
                (1.0, AlphaColor::<Hsl>::new([hue, 100., 50., 1.]).convert()),
            ])
            .with_interpolation_cs(ColorSpaceTag::LinearSrgb);

        cx.fill(&rect_path, &sat_gradient, 0.);

        // value gradient
        cx.push_layer(Mix::Multiply, 1.0, Affine::IDENTITY, &rect_path);

        let val_gradient = Gradient::new_linear(Point::ZERO, Point::new(0.0, size.height))
            .with_stops([(0.0, Color::from_rgba8(0, 0, 0, 0)), (1.0, css::BLACK)])
            .with_interpolation_cs(ColorSpaceTag::LinearSrgb);

        cx.fill(&rect_path, &val_gradient, 0.);

        cx.pop_layer();

        cx.clip(&rect_path);

        if size.width > 0.0 && size.height > 0.0 {
            // Larger indicator
            let outer_radius = 8.0;
            let inner_radius = 5.5;

            let outer = Circle::new(self.point, outer_radius);
            let inner = Circle::new(self.point, inner_radius);

            // fill center with the selected color
            cx.fill(&inner, self.current_color.convert::<Srgb>(), 0.);

            // white outline for contrast
            cx.stroke(&outer, css::WHITE, &Stroke::new(2.0));

            // subtle black inner stroke so it works on light backgrounds
            cx.stroke(&inner, css::BLACK, &Stroke::new(1.0));
        }

        cx.clear_clip();
    }
}

pub struct HuePicker {
    id: ViewId,
    size: Size,
    current_color: AlphaColor<Hsl>,
    on_change: Option<Box<dyn Fn(Color)>>,
    track: bool,
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
            track: false,
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
}

impl View for HuePicker {
    fn id(&self) -> ViewId {
        self.id
    }

    fn update(&mut self, _cx: &mut floem::context::UpdateCx, state: Box<dyn std::any::Any>) {
        if let Ok(color) = state.downcast::<Color>() {
            self.current_color = color.convert();
        }
    }

    fn event(&mut self, cx: &mut EventCx) -> EventPropagation {
        if let Some(new_layout) = LayoutChangedListener::extract(&cx.event) {
            self.post_layout(new_layout);
        }
        if let Some(on_change) = &self.on_change {
            match &cx.event {
                Event::Pointer(PointerEvent::Down(PointerButtonEvent {
                    state, pointer, ..
                })) => {
                    self.current_color = self.position_to_hsl(state.logical_point());
                    on_change(self.current_color.convert());
                    self.track = true;
                    if let Some(pointer_id) = pointer.pointer_id {
                        cx.request_pointer_capture(pointer_id);
                    }
                }
                Event::Pointer(PointerEvent::Up(PointerButtonEvent { state, .. })) => {
                    self.current_color = self.position_to_hsl(state.logical_point());
                    on_change(self.current_color.convert());
                    self.track = false;
                }
                Event::Pointer(PointerEvent::Move(pu)) => {
                    if self.track {
                        self.current_color = self.position_to_hsl(pu.current.logical_point());
                        on_change(self.current_color.convert());
                    }
                }
                _ => {
                    return EventPropagation::Continue;
                }
            }
        }
        EventPropagation::Stop
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        let size = self.size;
        let rect_path = Rect::ZERO.with_size(size).to_rounded_rect(8.);
        let hue_gradient = Gradient::new_linear(
            Point::new(0.0, size.height / 2.0),
            Point::new(size.width, size.height / 2.0),
        )
        .with_stops([
            (0.0, AlphaColor::<Hsl>::new([0.0, 100.0, 50.0, 1.0])),
            (1.0, AlphaColor::<Hsl>::new([360.0, 100.0, 50.0, 1.0])),
        ])
        .with_hue_direction(floem::peniko::color::HueDirection::Longer)
        .with_interpolation_cs(ColorSpaceTag::Hsl);

        cx.fill(&rect_path, &hue_gradient, 0.);
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

            cx.clip(&rect_path);
            cx.stroke(&indicator_rect, css::WHITE, &Stroke::new(2.0));
            cx.fill(&indicator_rect, css::BLACK, 0.);
            cx.clear_clip();
        }
    }
}

pub struct OpacityPicker {
    id: ViewId,
    size: Size,
    current_color: Color,
    on_change: Option<Box<dyn Fn(Color)>>,
    track: bool,
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
            track: false,
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
}

impl View for OpacityPicker {
    fn id(&self) -> ViewId {
        self.id
    }

    fn update(&mut self, _cx: &mut floem::context::UpdateCx, state: Box<dyn std::any::Any>) {
        if let Ok(color) = state.downcast::<Color>() {
            self.current_color = *color;
        }
    }

    fn event(&mut self, cx: &mut EventCx) -> EventPropagation {
        if let Some(new_layout) = LayoutChangedListener::extract(&cx.event) {
            self.post_layout(new_layout);
        }
        if let Some(on_change) = &self.on_change {
            match &cx.event {
                Event::Pointer(PointerEvent::Down(PointerButtonEvent {
                    state, pointer, ..
                })) => {
                    self.current_color = self.position_to_alpha(state.logical_point());
                    on_change(self.current_color);
                    self.track = true;
                    if let Some(pointer_id) = pointer.pointer_id {
                        cx.request_pointer_capture(pointer_id);
                    }
                }
                Event::Pointer(PointerEvent::Up(PointerButtonEvent { state, .. })) => {
                    self.current_color = self.position_to_alpha(state.logical_point());
                    on_change(self.current_color);
                    self.track = false;
                }
                Event::Pointer(PointerEvent::Move(pu)) => {
                    if self.track {
                        self.current_color = self.position_to_alpha(pu.current.logical_point());
                        on_change(self.current_color);
                    }
                }
                _ => {
                    return EventPropagation::Continue;
                }
            }
        }
        EventPropagation::Stop
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        let size = self.size;
        let rect_path = Rect::ZERO.with_size(size).to_rounded_rect(8.);

        draw_transparency_checkerboard(cx, size, &rect_path);

        let opacity_gradient = Gradient::new_linear(
            Point::new(0.0, size.height / 2.0),
            Point::new(size.width, size.height / 2.0),
        )
        .with_stops([
            (0.0, self.current_color.with_alpha(0.0)),
            (1.0, self.current_color.with_alpha(1.0)),
        ])
        .with_interpolation_cs(LinearSrgb);

        cx.fill(&rect_path, &opacity_gradient, 0.);

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

            cx.clip(&rect_path);
            cx.stroke(&indicator_rect, css::WHITE, &Stroke::new(2.0));
            cx.fill(&indicator_rect, css::BLACK, 0.);
            cx.clear_clip();
        }
    }
}
