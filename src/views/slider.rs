//! A toggle button widget. An example can be found in widget-gallery/button in the floem examples.

use floem_reactive::{create_effect, SignalGet, SignalUpdate};
use floem_renderer::Renderer;
use floem_winit::keyboard::{Key, NamedKey};
use peniko::kurbo::{Circle, Point, RoundedRect};
use peniko::{Brush, Color};

use crate::{
    event::EventPropagation,
    id::ViewId,
    prop, prop_extractor,
    style::{Background, BorderRadius, CustomStylable, Foreground, Height, Style},
    style_class,
    unit::{PxPct, PxPctAuto},
    view::View,
    views::Decorators,
};

pub fn slider(percent: impl Fn() -> f32 + 'static) -> Slider {
    Slider::new(percent)
}

enum SliderUpdate {
    Percent(f32),
}

prop!(pub EdgeAlign: bool {} = false);
prop!(pub HandleRadius: PxPct {} = PxPct::Pct(98.));

prop_extractor! {
    SliderStyle {
        foreground: Foreground,
        handle_radius: HandleRadius,
        edge_align: EdgeAlign,
    }
}
style_class!(pub SliderClass);
style_class!(pub BarClass);
style_class!(pub AccentBarClass);

prop_extractor! {
    BarStyle {
        border_radius: BorderRadius,
        color: Background,
        height: Height

    }
}

/// **A reactive slider.**
///
/// You can set the slider to a percent value between 0 and 100.
///
/// The slider is composed of four parts. The main view, the background bar, an accent bar and a handle.
/// The background bar is separate from the main view because it is shortened when [`EdgeAlign`] is set to false;
///
/// **Responding to events**:
/// You can respond to events by calling the [`Slider::on_change_pct`], and [`Slider::on_change_px`] methods on [`Slider`] and passing in a callback. Both of these callbacks are called whenever a change is effected by either clicking or by the arrow keys.
/// These callbacks will not be called on reactive updates, only on a mouse event or by using the arrow keys.
///
/// You can also disable event handling [`Decorators::disabled`]. If you want to use this slider as a progress bar this may be useful.
///
/// **Styling**:
/// You can set three properties on the slider (`SliderClass`): [`Foreground`] color and [`HandleRadius`], which both affect the handle, and [`EdgeAlign`].
/// You can set the [`HandleRadius`] to either be a pixel value or a percent value. If you set it to a percent it is relative to the main height of the view. 50% radius will make the handle fill the background.
/// If you set [`EdgeAlign`] to `true`, at 0% and 100% the edges of the handle will be within the bar.If you set it to `false` then the bars will be shortened and the handle will appear to have it's center at the ends of the bar.
///
/// You can set properties on the bars as well. The bar (`BarClass`) and accent bar (`AccentBarClass`) both have a [`BorderRadius`] and [`Background`] color. You can also set a height on the accent bar.
// The height of the main bar will bet set to the height of the main view.
///
/// Styling Example:
/// ```rust
/// # use floem::unit::UnitExt;
/// # use floem::peniko::{Color, Brush};
/// # use floem::style::Foreground;
/// # use floem::views::slider;
/// # use floem::views::empty;
/// # use floem::views::Decorators;
/// empty()
///     .style(|s|
///         s.class(slider::SliderClass, |s| {
///             s.set(Foreground, Brush::Solid(Color::WHITE))
///                 .set(slider::EdgeAlign, true)
///                 .set(slider::HandleRadius, 50.pct())
///         })
///         .class(slider::BarClass, |s| {
///             s.background(Color::BLACK)
///                 .border_radius(100.pct())
///         })
///         .class(slider::AccentBarClass, |s| {
///             s.background(Color::GREEN)
///                 .border_radius(100.pct())
///                 .height(100.pct())
///         })
///  );
///```
pub struct Slider {
    id: ViewId,
    onchangepx: Option<Box<dyn Fn(f32)>>,
    onchangepct: Option<Box<dyn Fn(f32)>>,
    held: bool,
    percent: f32,
    prev_percent: f32,
    base_bar_style: BarStyle,
    accent_bar_style: BarStyle,
    handle: Circle,
    base_bar: RoundedRect,
    accent_bar: RoundedRect,
    size: taffy::prelude::Size<f32>,
    style: SliderStyle,
}

impl View for Slider {
    fn id(&self) -> ViewId {
        self.id
    }

    fn update(&mut self, _cx: &mut crate::context::UpdateCx, state: Box<dyn std::any::Any>) {
        if let Ok(update) = state.downcast::<SliderUpdate>() {
            match *update {
                SliderUpdate::Percent(percent) => self.percent = percent,
            }
            self.id.request_layout();
        }
    }

    fn event_before_children(
        &mut self,
        cx: &mut crate::context::EventCx,
        event: &crate::event::Event,
    ) -> EventPropagation {
        let pos_changed = match event {
            crate::event::Event::PointerDown(event) => {
                cx.update_active(self.id());
                self.id.request_layout();
                self.held = true;
                self.percent = event.pos.x as f32 / self.size.width * 100.;
                true
            }
            crate::event::Event::PointerUp(event) => {
                self.id.request_layout();

                // set the state based on the position of the slider
                let changed = self.held;
                if self.held {
                    self.percent = event.pos.x as f32 / self.size.width * 100.;
                    self.update_restrict_position();
                }
                self.held = false;
                changed
            }
            crate::event::Event::PointerMove(event) => {
                self.id.request_layout();
                if self.held {
                    self.percent = event.pos.x as f32 / self.size.width * 100.;
                    true
                } else {
                    false
                }
            }
            crate::event::Event::FocusLost => {
                self.held = false;
                false
            }
            crate::event::Event::KeyDown(event) => {
                if event.key.logical_key == Key::Named(NamedKey::ArrowLeft) {
                    self.id.request_layout();
                    self.percent -= 10.;
                    true
                } else if event.key.logical_key == Key::Named(NamedKey::ArrowRight) {
                    self.id.request_layout();
                    self.percent += 10.;
                    true
                } else {
                    false
                }
            }
            _ => false,
        };

        self.update_restrict_position();

        if pos_changed && self.percent != self.prev_percent {
            if let Some(onchangepx) = &self.onchangepx {
                onchangepx(self.handle_center());
            }
            if let Some(onchangepct) = &self.onchangepct {
                onchangepct(self.percent)
            }
        }

        EventPropagation::Continue
    }

    fn style_pass(&mut self, cx: &mut crate::context::StyleCx<'_>) {
        let style = cx.style();
        let mut paint = false;

        let base_bar_style = style.clone().apply_class(BarClass);
        paint |= self.base_bar_style.read_style(cx, &base_bar_style);

        let accent_bar_style = style.apply_class(AccentBarClass);
        paint |= self.accent_bar_style.read_style(cx, &accent_bar_style);
        paint |= self.style.read(cx);
        if paint {
            cx.app_state_mut().request_paint(self.id);
        }
    }

    fn compute_layout(
        &mut self,
        _cx: &mut crate::context::ComputeLayoutCx,
    ) -> Option<peniko::kurbo::Rect> {
        self.update_restrict_position();
        let layout = self.id.get_layout().unwrap_or_default();

        self.size = layout.size;

        let circle_radius = match self.style.handle_radius() {
            PxPct::Px(px) => px as f32,
            PxPct::Pct(pct) => self.size.width.min(self.size.height) / 2. * (pct as f32 / 100.),
        };
        let width = self.size.width - circle_radius * 2.;
        let center = width * (self.percent / 100.) + circle_radius;
        let circle_point = Point::new(center as f64, (self.size.height / 2.) as f64);
        self.handle = crate::kurbo::Circle::new(circle_point, circle_radius as f64);

        let base_bar_height = match self.base_bar_style.height() {
            PxPctAuto::Px(px) => px,
            PxPctAuto::Pct(pct) => self.size.height as f64 * (pct / 100.),
            PxPctAuto::Auto => self.size.height as f64,
        };
        let accent_bar_height = match self.accent_bar_style.height() {
            PxPctAuto::Px(px) => px,
            PxPctAuto::Pct(pct) => self.size.height as f64 * (pct / 100.),
            PxPctAuto::Auto => self.size.height as f64,
        };

        let base_bar_radius = match self.base_bar_style.border_radius() {
            PxPct::Px(px) => px,
            PxPct::Pct(pct) => base_bar_height / 2. * (pct / 100.),
        };
        let accent_bar_radius = match self.accent_bar_style.border_radius() {
            PxPct::Px(px) => px,
            PxPct::Pct(pct) => accent_bar_height / 2. * (pct / 100.),
        };

        let mut base_bar_length = self.size.width as f64;
        if !self.style.edge_align() {
            base_bar_length -= self.handle.radius * 2.;
        }

        let base_bar_y_start = self.size.height as f64 / 2. - base_bar_height / 2.;
        let accent_bar_y_start = self.size.height as f64 / 2. - accent_bar_height / 2.;

        let bar_x_start = if self.style.edge_align() {
            0.
        } else {
            self.handle.radius
        };

        self.base_bar = peniko::kurbo::Rect::new(
            bar_x_start,
            base_bar_y_start,
            bar_x_start + base_bar_length,
            base_bar_y_start + base_bar_height,
        )
        .to_rounded_rect(base_bar_radius);
        self.accent_bar = peniko::kurbo::Rect::new(
            bar_x_start,
            accent_bar_y_start,
            self.handle_center() as f64,
            accent_bar_y_start + accent_bar_height,
        )
        .to_rounded_rect(accent_bar_radius);

        self.prev_percent = self.percent;

        None
    }

    fn paint(&mut self, cx: &mut crate::context::PaintCx) {
        cx.fill(
            &self.base_bar,
            &self.base_bar_style.color().unwrap_or(Color::BLACK.into()),
            0.,
        );
        cx.save();
        cx.clip(&self.base_bar);
        cx.fill(
            &self.accent_bar,
            &self
                .accent_bar_style
                .color()
                .unwrap_or(Color::TRANSPARENT.into()),
            0.,
        );
        cx.restore();

        if let Some(color) = self.style.foreground() {
            cx.fill(&self.handle, &color, 0.);
        }
    }
}
impl Slider {
    pub fn new(percent: impl Fn() -> f32 + 'static) -> Self {
        let id = ViewId::new();
        create_effect(move |_| {
            let percent = percent();
            id.update_state(SliderUpdate::Percent(percent));
        });
        Slider {
            id,
            onchangepx: None,
            onchangepct: None,
            held: false,
            percent: 0.0,
            prev_percent: 0.0,
            handle: Default::default(),
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

    pub fn new_get(percent: impl SignalGet<f32> + 'static) -> Self {
        Self::new(move || percent.get())
    }

    pub fn new_update(percent: impl SignalUpdate<f32> + 'static) -> Self {
        Self::new(move || 0.).on_change_pct(move |pct| percent.set(pct))
    }

    pub fn new_get_update(
        percent: impl SignalGet<f32> + SignalUpdate<f32> + Copy + 'static,
    ) -> Self {
        Self::new(move || percent.get()).on_change_pct(move |pct| percent.set(pct))
    }

    fn update_restrict_position(&mut self) {
        self.percent = self.percent.clamp(0., 100.);
    }

    fn handle_center(&self) -> f32 {
        let width = self.size.width - self.handle.radius as f32 * 2.;
        width * (self.percent / 100.) + self.handle.radius as f32
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

    /// Sets the custom style properties of the `Slider`.
    pub fn slider_style(
        self,
        style: impl Fn(SliderCustomStyle) -> SliderCustomStyle + 'static,
    ) -> Self {
        self.custom_style(style)
    }
}

#[derive(Debug, Default, Clone)]
pub struct SliderCustomStyle(Style);
impl From<SliderCustomStyle> for Style {
    fn from(val: SliderCustomStyle) -> Self {
        val.0
    }
}

impl CustomStylable<SliderCustomStyle> for Slider {
    type DV = Self;
}

impl SliderCustomStyle {
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the color of the slider handle.
    ///
    /// # Arguments
    /// * `color` - An optional `Color` that sets the handle's color. If `None` is provided, the handle color is not set.
    pub fn handle_color(mut self, color: impl Into<Option<Brush>>) -> Self {
        self = SliderCustomStyle(self.0.set(Foreground, color));
        self
    }

    /// Sets the edge alignment of the slider handle.
    ///
    /// # Arguments
    /// * `align` - A boolean value that determines the alignment of the handle. If `true`, the edges of the handle are within the bar at 0% and 100%. If `false`, the bars are shortened and the handle's center appears at the ends of the bar.
    pub fn edge_align(mut self, align: bool) -> Self {
        self = SliderCustomStyle(self.0.set(EdgeAlign, align));
        self
    }

    /// Sets the radius of the slider handle.
    ///
    /// # Arguments
    /// * `radius` - A `PxPct` value that sets the handle's radius. This can be a pixel value or a percent value relative to the main height of the view.
    pub fn handle_radius(mut self, radius: impl Into<PxPct>) -> Self {
        self = SliderCustomStyle(self.0.set(HandleRadius, radius));
        self
    }

    /// Sets the color of the slider's bar.
    ///
    /// # Arguments
    /// * `color` - A `StyleValue<Color>` that sets the bar's background color.
    pub fn bar_color(mut self, color: impl Into<Brush>) -> Self {
        self = SliderCustomStyle(self.0.class(BarClass, |s| s.background(color)));
        self
    }

    /// Sets the border radius of the slider's bar.
    ///
    /// # Arguments
    /// * `radius` - A `PxPct` value that sets the bar's border radius. This can be a pixel value or a percent value relative to the bar's height.
    pub fn bar_radius(mut self, radius: impl Into<PxPct>) -> Self {
        self = SliderCustomStyle(self.0.class(BarClass, |s| s.border_radius(radius)));
        self
    }

    /// Sets the height of the slider's bar.
    ///
    /// # Arguments
    /// * `height` - A `PxPctAuto` value that sets the bar's height. This can be a pixel value, a percent value relative to the view's height, or `Auto` to use the view's height.
    pub fn bar_height(mut self, height: impl Into<PxPctAuto>) -> Self {
        self = SliderCustomStyle(self.0.class(BarClass, |s| s.height(height)));
        self
    }

    /// Sets the color of the slider's accent bar.
    ///
    /// # Arguments
    /// * `color` - A `StyleValue<Color>` that sets the accent bar's background color.
    pub fn accent_bar_color(mut self, color: impl Into<Brush>) -> Self {
        self = SliderCustomStyle(self.0.class(AccentBarClass, |s| s.background(color)));
        self
    }

    /// Sets the border radius of the slider's accent bar.
    ///
    /// # Arguments
    /// * `radius` - A `PxPct` value that sets the accent bar's border radius. This can be a pixel value or a percent value relative to the accent bar's height.
    pub fn accent_bar_radius(mut self, radius: impl Into<PxPct>) -> Self {
        self = SliderCustomStyle(self.0.class(AccentBarClass, |s| s.border_radius(radius)));
        self
    }

    /// Sets the height of the slider's accent bar.
    ///
    /// # Arguments
    /// * `height` - A `PxPctAuto` value that sets the accent bar's height. This can be a pixel value, a percent value relative to the view's height, or `Auto` to use the view's height.
    pub fn accent_bar_height(mut self, height: impl Into<PxPctAuto>) -> Self {
        self = SliderCustomStyle(self.0.class(AccentBarClass, |s| s.height(height)));
        self
    }

    /// Apply regular style properties
    pub fn style(mut self, style: impl Fn(Style) -> Style + 'static) -> Self {
        self = Self(self.0.apply(style(Style::new())));
        self
    }
}
