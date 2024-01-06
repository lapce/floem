//! A toggle button widget. An example can be found in widget-gallery/button in the floem examples.

use floem_reactive::create_effect;
use floem_renderer::Renderer;
use kurbo::{Circle, Point, RoundedRect};
use peniko::Color;
use winit::keyboard::{Key, NamedKey};

use crate::{
    prop,
    prop_extracter,
    style::{Background, BorderRadius, Foreground},
    style_class,
    unit::PxPct,
    view::{View, ViewData},
    views::Decorators,
    EventPropagation,
    // EventPropagation,
};

prop!(pub CircleRad: PxPct {} = PxPct::Pct(98.));
prop!(pub BarExtends: bool {} = false);
prop!(pub Thickness: PxPct {} = PxPct::Pct(30.));

prop_extracter! {
    SliderStyle {
        foreground: Foreground,
        circle_rad: CircleRad,
        bar_extends: BarExtends,
    }
}
style_class!(pub SliderClass);
style_class!(pub Bar);
style_class!(pub AccentBar);

prop_extracter! {
    BarStyle {
        border_radius: BorderRadius,
        color: Background,
        thickness: Thickness,

    }
}

/// A slider
pub struct Slider {
    data: ViewData,
    onchangepx: Option<Box<dyn Fn(f32)>>,
    onchangepct: Option<Box<dyn Fn(f32)>>,
    held: bool,
    position: f32,
    prev_position: f32,
    base_bar_style: BarStyle,
    accent_bar_style: BarStyle,
    circle: Circle,
    base_bar: RoundedRect,
    accent_bar: RoundedRect,
    size: taffy::prelude::Size<f32>,
    style: SliderStyle,
}

/// A reactive slider.
pub fn slider(state: impl Fn() -> f32 + 'static) -> Slider {
    let id = crate::id::Id::next();
    create_effect(move |_| {
        let state = state();
        id.update_state(state, false);
    });
    Slider {
        data: ViewData::new(id),
        onchangepx: None,
        onchangepct: None,
        held: false,
        position: 0.0,
        prev_position: 0.0,
        circle: Default::default(),
        base_bar_style: Default::default(),
        accent_bar_style: Default::default(),
        base_bar: Default::default(),
        accent_bar: Default::default(),
        size: Default::default(),
        style: Default::default(),
    }
    .class(SliderClass)
    .keyboard_navigatable()
}

impl View for Slider {
    fn view_data(&self) -> &ViewData {
        &self.data
    }

    fn view_data_mut(&mut self) -> &mut ViewData {
        &mut self.data
    }

    fn update(&mut self, cx: &mut crate::context::UpdateCx, state: Box<dyn std::any::Any>) {
        if let Ok(position) = state.downcast::<f32>() {
            self.position = *position;
            cx.request_layout(self.id());
        }
    }

    fn event(
        &mut self,
        cx: &mut crate::context::EventCx,
        _id_path: Option<&[crate::id::Id]>,
        event: crate::event::Event,
    ) -> EventPropagation {
        match event {
            crate::event::Event::PointerDown(event) => {
                cx.update_active(self.id());
                cx.app_state_mut().request_layout(self.id());
                self.held = true;
                self.position = event.pos.x as f32;
            }
            crate::event::Event::PointerUp(event) => {
                cx.app_state_mut().request_layout(self.id());

                // set the state based on the position of the slider
                if self.held {
                    self.position = event.pos.x as f32;
                    self.update_restrict_position();
                }
                self.held = false;
            }
            crate::event::Event::PointerMove(event) => {
                cx.app_state_mut().request_layout(self.id());
                if self.held {
                    self.position = event.pos.x as f32;
                    self.update_restrict_position();
                }
            }
            crate::event::Event::FocusLost => {
                self.held = false;
            }
            crate::event::Event::KeyDown(event) => {
                if event.key.logical_key == Key::Named(NamedKey::ArrowLeft) {
                    cx.app_state_mut().request_layout(self.id());
                    self.position -= (self.size.width - self.circle.radius as f32 * 2.) * 0.1;
                    self.update_restrict_position();
                } else if event.key.logical_key == Key::Named(NamedKey::ArrowRight) {
                    cx.app_state_mut().request_layout(self.id());
                    self.position += (self.size.width - self.circle.radius as f32 * 2.) * 0.1;
                    self.update_restrict_position();
                }
            }
            _ => {}
        };
        EventPropagation::Continue
    }

    fn style(&mut self, cx: &mut crate::context::StyleCx<'_>) {
        let style = cx.style();
        let mut paint = false;

        let base_bar_style = style.clone().apply_class(Bar);
        paint |= self.base_bar_style.read_style(cx, &base_bar_style);

        let accent_bar_style = style.apply_class(AccentBar);
        paint |= self.accent_bar_style.read_style(cx, &accent_bar_style);
        paint |= self.style.read(cx);
        if paint {
            cx.app_state_mut().request_paint(self.data.id());
        }
    }

    fn compute_layout(&mut self, cx: &mut crate::context::ComputeLayoutCx) -> Option<kurbo::Rect> {
        let layout = cx.get_layout(self.id()).unwrap();

        self.size = layout.size;

        let circle_radius = match self.style.circle_rad() {
            PxPct::Px(px) => px as f32,
            PxPct::Pct(pct) => self.size.width.min(self.size.height) / 2. * (pct as f32 / 100.),
        };
        let circle_point = Point::new(self.position as f64, (self.size.height / 2.) as f64);
        self.circle = crate::kurbo::Circle::new(circle_point, circle_radius as f64);

        let base_bar_thickness = match self.base_bar_style.thickness() {
            PxPct::Px(px) => px,
            PxPct::Pct(pct) => self.size.height as f64 * (pct / 100.),
        };
        let accent_bar_thickness = match self.accent_bar_style.thickness() {
            PxPct::Px(px) => px,
            PxPct::Pct(pct) => self.size.height as f64 * (pct / 100.),
        };

        let base_bar_radius = match self.base_bar_style.border_radius() {
            PxPct::Px(px) => px,
            PxPct::Pct(pct) => base_bar_thickness / 2. * (pct / 100.),
        };
        let accent_bar_radius = match self.accent_bar_style.border_radius() {
            PxPct::Px(px) => px,
            PxPct::Pct(pct) => accent_bar_thickness / 2. * (pct / 100.),
        };

        let mut base_bar_length = self.size.width as f64;
        if !self.style.bar_extends() {
            base_bar_length -= self.circle.radius * 2.;
        }

        let base_bar_y_start = self.size.height as f64 / 2. - base_bar_thickness / 2.;
        let accent_bar_y_start = self.size.height as f64 / 2. - accent_bar_thickness / 2.;

        let bar_x_start = if self.style.bar_extends() {
            0.
        } else {
            self.circle.radius
        };

        self.base_bar = kurbo::Rect::new(
            bar_x_start,
            base_bar_y_start,
            bar_x_start + base_bar_length,
            base_bar_y_start + base_bar_thickness,
        )
        .to_rounded_rect(base_bar_radius);
        self.accent_bar = kurbo::Rect::new(
            bar_x_start,
            accent_bar_y_start,
            self.position as f64,
            accent_bar_y_start + accent_bar_thickness,
        )
        .to_rounded_rect(accent_bar_radius);

        if self.position != self.prev_position {
            if let Some(onchangepx) = &self.onchangepx {
                onchangepx(self.position);
            }
            if let Some(onchangepct) = &self.onchangepct {
                onchangepct(
                    (self.position - circle_radius) / (self.size.width - circle_radius * 2.),
                )
            }
        }
        self.prev_position = self.position;

        None
    }

    fn paint(&mut self, cx: &mut crate::context::PaintCx) {
        cx.fill(
            &self.base_bar,
            self.base_bar_style.color().unwrap_or(Color::BLACK),
            0.,
        );
        cx.clip(&self.base_bar);
        cx.fill(
            &self.accent_bar,
            self.accent_bar_style.color().unwrap_or(Color::GREEN),
            0.,
        );

        if let Some(color) = self.style.foreground() {
            cx.clear_clip();
            cx.fill(&self.circle, color, 0.);
        }
    }
}
impl Slider {
    fn update_restrict_position(&mut self) {
        self.position = self
            .position
            .max(self.circle.radius as f32)
            .min(self.size.width - self.circle.radius as f32);
    }

    /// Add an event handler to be run when the button is toggled.
    ///
    ///This does not run if the state is changed because of an outside signal.
    /// This handler is only called if this button is clicked or switched
    pub fn on_change_pct(mut self, onchangepct: impl Fn(f32) + 'static) -> Self {
        self.onchangepct = Some(Box::new(onchangepct));
        self
    }
    pub fn on_change_px(mut self, onchangepx: impl Fn(f32) + 'static) -> Self {
        self.onchangepx = Some(Box::new(onchangepx));
        self
    }
}
