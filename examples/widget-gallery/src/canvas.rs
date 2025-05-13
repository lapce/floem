use floem::{
    context::PaintCx,
    event::{Event, EventPropagation, PointerEvent},
    kurbo::{Affine, Circle, Point, Rect, Shape, Size, Stroke},
    peniko::{
        color::{AlphaColor, ColorSpaceTag::LinearSrgb, Hsl},
        Gradient, Mix,
    },
    prelude::*,
    reactive::create_updater,
    ViewId,
};
use palette::css;

use crate::form::{form, form_item};

pub fn canvas_view() -> impl IntoView {
    let rounded = RwSignal::new(true);

    form((
        form_item(
            "Simple Canvas:",
            h_stack((
                canvas(move |cx, size| {
                    cx.fill(
                        &Rect::ZERO
                            .with_size(size)
                            .to_rounded_rect(if rounded.get() { 8. } else { 0. }),
                        css::PURPLE,
                        0.,
                    );
                })
                .style(|s| s.size(100, 300)),
                button("toggle")
                    .action(move || rounded.update(|s| *s = !*s))
                    .style(|s| s.height(30)),
            ))
            .style(|s| s.gap(10).items_center()),
        ),
        form_item(
            "Complex Canvas:",
            color_picker().style(|s| s.size(500, 500)),
        ),
    ))
}

fn color_picker() -> impl IntoView {
    let color = RwSignal::new(css::AQUA);

    let hue_opocity = v_stack((
        HuePicker::new(move || color.get())
            .on_change(move |c| color.set(c))
            .style(|s| s.size_full().border(1).border_radius(8)),
        OpacityPicker::new(move || color.get())
            .on_change(move |c| color.set(c))
            .style(|s| s.size_full().border(1).border_radius(8)),
    ))
    .style(|s| s.gap(5).size_full())
    .debug_name("hue opacity");

    let final_hue_op = h_stack((
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

    v_stack((
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
    current_color: AlphaColor<Hsl>,
    on_change: Option<Box<dyn Fn(Color)>>,
    track: bool,
}
impl SatValuePicker {
    pub fn new(color: impl Fn() -> Color + 'static) -> Self {
        let id = ViewId::new();
        let color = create_updater(color, move |c| id.update_state(c));
        Self {
            id,
            size: Size::ZERO,
            current_color: color.convert(),
            on_change: None,
            track: false,
        }
    }

    fn position_to_hsl(&self, pos: Point) -> AlphaColor<Hsl> {
        let hue = self.current_color.components[0];

        let saturation =
            (pos.x / self.size.width * 100.).clamp(0.0 + f64::EPSILON, 100.0 - f64::EPSILON);

        let value = ((1.0 - (pos.y / self.size.height)) * 100.)
            .clamp(0.0 + f64::EPSILON, 100.0 - f64::EPSILON);

        let alpha = self.current_color.components[3];

        AlphaColor::<Hsl>::new([hue, saturation as f32, value as f32, alpha])
    }

    pub fn on_change(mut self, on_change: impl Fn(Color) + 'static) -> Self {
        self.on_change = Some(Box::new(on_change));
        self
    }
}
impl View for SatValuePicker {
    fn id(&self) -> ViewId {
        self.id
    }

    fn compute_layout(&mut self, _cx: &mut floem::context::ComputeLayoutCx) -> Option<Rect> {
        self.size = self.id.get_size().unwrap_or_default();
        None
    }

    fn update(&mut self, _cx: &mut floem::context::UpdateCx, state: Box<dyn std::any::Any>) {
        if let Ok(color) = state.downcast::<Color>() {
            self.current_color = color.convert();
        }
    }

    fn event_before_children(
        &mut self,
        _cx: &mut floem::context::EventCx,
        event: &Event,
    ) -> EventPropagation {
        if let Some(on_change) = &self.on_change {
            match event {
                Event::Pointer(PointerEvent::Down { state, .. }) => {
                    self.current_color = self.position_to_hsl(state.position);
                    on_change(self.current_color.convert());
                    self.track = true;
                    self.id.request_active();
                }
                Event::Pointer(PointerEvent::Up { state, .. }) => {
                    self.current_color = self.position_to_hsl(state.position);
                    on_change(self.current_color.convert());
                    self.id.clear_active();
                    self.track = false;
                }
                Event::Pointer(PointerEvent::Move(pu)) => {
                    if self.track {
                        self.current_color = self.position_to_hsl(pu.current.position);
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

        let lightness_gradient = Gradient::new_linear(Point::ZERO, Point::new(0.0, size.height))
            .with_stops([(0.0, css::WHITE), (1.0, css::BLACK)]);
        cx.fill(&rect_path, &lightness_gradient, 0.);

        cx.push_layer(Mix::Color, 1.0, Affine::IDENTITY, &rect_path);

        let saturation_gradient = Gradient::new_linear(Point::ZERO, Point::new(size.width, 0.0))
            .with_stops([
                (0.0, AlphaColor::<Hsl>::new([hue, 0., 50., 1.])),
                (1.0, AlphaColor::<Hsl>::new([hue, 100., 50., 1.])),
            ]);
        cx.fill(&rect_path, &saturation_gradient, 0.);

        cx.pop_layer();

        if size.width > 0.0 && size.height > 0.0 {
            let saturation = self.current_color.components[1];
            let value = self.current_color.components[2];

            let x_pos = saturation as f64 / 100.0 * size.width;
            let y_pos = (1.0 - value as f64 / 100.0) * size.height;

            let indicator_radius = 6.0;
            let indicator_circle = Circle::new(Point::new(x_pos, y_pos), indicator_radius);

            cx.stroke(&indicator_circle, css::WHITE, &Stroke::new(2.0));
        }
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
        let color = create_updater(color, move |c| id.update_state(c));
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
}

impl View for HuePicker {
    fn id(&self) -> ViewId {
        self.id
    }

    fn compute_layout(&mut self, _cx: &mut floem::context::ComputeLayoutCx) -> Option<Rect> {
        self.size = self.id.get_size().unwrap_or_default();
        None
    }

    fn update(&mut self, _cx: &mut floem::context::UpdateCx, state: Box<dyn std::any::Any>) {
        if let Ok(color) = state.downcast::<Color>() {
            self.current_color = color.convert();
        }
    }

    fn event_before_children(
        &mut self,
        _cx: &mut floem::context::EventCx,
        event: &Event,
    ) -> EventPropagation {
        if let Some(on_change) = &self.on_change {
            match event {
                Event::Pointer(PointerEvent::Down { state, .. }) => {
                    self.current_color = self.position_to_hsl(state.position);
                    on_change(self.current_color.convert());
                    self.track = true;
                    self.id.request_active();
                }
                Event::Pointer(PointerEvent::Up { state, .. }) => {
                    self.current_color = self.position_to_hsl(state.position);
                    on_change(self.current_color.convert());
                    self.id.clear_active();
                    self.track = false;
                }
                Event::Pointer(PointerEvent::Move(pu)) => {
                    if self.track {
                        self.current_color = self.position_to_hsl(pu.current.position);
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
            (0.0, Color::from_rgba8(255, 0, 0, 255)),
            (0.1, Color::from_rgba8(255, 154, 0, 255)),
            (0.2, Color::from_rgba8(208, 222, 33, 255)),
            (0.3, Color::from_rgba8(79, 220, 74, 255)),
            (0.4, Color::from_rgba8(63, 218, 216, 255)),
            (0.5, Color::from_rgba8(47, 201, 226, 255)),
            (0.6, Color::from_rgba8(28, 127, 238, 255)),
            (0.7, Color::from_rgba8(95, 21, 242, 255)),
            (0.8, Color::from_rgba8(186, 12, 248, 255)),
            (0.9, Color::from_rgba8(251, 7, 217, 255)),
            (1.0, Color::from_rgba8(255, 0, 0, 255)),
        ]);

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

            cx.stroke(&indicator_rect, css::WHITE, &Stroke::new(2.0));
            cx.fill(&indicator_rect, css::BLACK, 0.);
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
        let color = create_updater(color, move |c| id.update_state(c));
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
}

impl View for OpacityPicker {
    fn id(&self) -> ViewId {
        self.id
    }

    fn compute_layout(&mut self, _cx: &mut floem::context::ComputeLayoutCx) -> Option<Rect> {
        self.size = self.id.get_size().unwrap_or_default();
        None
    }

    fn update(&mut self, _cx: &mut floem::context::UpdateCx, state: Box<dyn std::any::Any>) {
        if let Ok(color) = state.downcast::<Color>() {
            self.current_color = *color;
        }
    }

    fn event_before_children(
        &mut self,
        _cx: &mut floem::context::EventCx,
        event: &Event,
    ) -> EventPropagation {
        if let Some(on_change) = &self.on_change {
            match event {
                Event::Pointer(PointerEvent::Down { state, .. }) => {
                    self.current_color = self.position_to_alpha(state.position);
                    on_change(self.current_color);
                    self.track = true;
                    self.id.request_active();
                }
                Event::Pointer(PointerEvent::Up { state, .. }) => {
                    self.current_color = self.position_to_alpha(state.position);
                    on_change(self.current_color);
                    self.id.clear_active();
                    self.track = false;
                }
                Event::Pointer(PointerEvent::Move(pu)) => {
                    if self.track {
                        self.current_color = self.position_to_alpha(pu.current.position);
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

            cx.stroke(&indicator_rect, css::WHITE, &Stroke::new(2.0));
            cx.fill(&indicator_rect, css::BLACK, 0.);
        }
    }
}
