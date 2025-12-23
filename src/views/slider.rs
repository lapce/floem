//! A toggle button widget. An example can be found in widget-gallery/button in the floem examples.

use std::ops::RangeInclusive;

use floem_reactive::{SignalGet, SignalUpdate, UpdaterEffect};
use peniko::Brush;
use peniko::color::palette;
use peniko::kurbo::{Circle, Point, RoundedRect, RoundedRectRadii};
use ui_events::keyboard::{Key, KeyState, KeyboardEvent, NamedKey};
use ui_events::pointer::{PointerButtonEvent, PointerEvent};

use crate::style::{BorderRadiusProp, CustomStyle};
use crate::unit::Pct;
use crate::{
    Renderer,
    event::EventPropagation,
    view::ViewId,
    prop, prop_extractor,
    style::{Background, CustomStylable, Foreground, Height, Style},
    style_class,
    unit::{PxPct, PxPctAuto},
    view::View,
    views::Decorators,
};

/// Creates a new [Slider] with a function that returns a percentage value.
/// See [Slider] for more documentation
pub fn slider<P: Into<Pct>>(percent: impl Fn() -> P + 'static) -> Slider {
    Slider::new(percent)
}

enum SliderUpdate {
    Percent(f64),
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
        border_radius: BorderRadiusProp,
        color: Background,
        height: Height

    }
}

fn border_radius(style: &BarStyle, size: f64) -> RoundedRectRadii {
    let border_radius = style.border_radius();
    RoundedRectRadii {
        top_left: crate::view::border_radius(
            border_radius.top_left.unwrap_or(PxPct::Px(0.0)),
            size,
        ),
        top_right: crate::view::border_radius(
            border_radius.top_right.unwrap_or(PxPct::Px(0.0)),
            size,
        ),
        bottom_left: crate::view::border_radius(
            border_radius.bottom_left.unwrap_or(PxPct::Px(0.0)),
            size,
        ),
        bottom_right: crate::view::border_radius(
            border_radius.bottom_right.unwrap_or(PxPct::Px(0.0)),
            size,
        ),
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
/// You can use the [`Slider::slider_style`] method to get access to a [`SliderCustomStyle`] which has convenient functions with documentation for styling all of the properties of the slider.
///
/// Styling Example:
/// ```rust
/// # use floem::prelude::*;
/// # use floem::peniko::Brush;
/// # use floem::style::Foreground;
/// slider::Slider::new(|| 40.pct())
///     .slider_style(|s| {
///         s.edge_align(true)
///             .handle_radius(50.pct())
///             .bar_color(palette::css::BLACK)
///             .bar_radius(100.pct())
///             .accent_bar_color(palette::css::GREEN)
///             .accent_bar_radius(100.pct())
///             .accent_bar_height(100.pct())
///     });
///```
pub struct Slider {
    id: ViewId,
    onchangepx: Option<Box<dyn Fn(f64)>>,
    onchangepct: Option<Box<dyn Fn(Pct)>>,
    onchangevalue: Option<Box<dyn Fn(f64)>>,
    onhover: Option<Box<dyn Fn(Pct)>>,
    held: bool,
    percent: f64,
    prev_percent: f64,
    base_bar_style: BarStyle,
    accent_bar_style: BarStyle,
    handle: Circle,
    base_bar: RoundedRect,
    accent_bar: RoundedRect,
    size: taffy::prelude::Size<f32>,
    style: SliderStyle,
    range: RangeInclusive<f64>,
    step: Option<f64>,
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
            crate::event::Event::Pointer(PointerEvent::Down(PointerButtonEvent {
                state, ..
            })) => {
                cx.update_active(self.id());
                self.id.request_layout();
                self.held = true;
                self.percent = self.mouse_pos_to_percent(state.logical_point().x);
                true
            }
            crate::event::Event::Pointer(PointerEvent::Up(PointerButtonEvent {
                state, ..
            })) => {
                self.id.request_layout();

                // set the state based on the position of the slider
                let changed = self.held;
                if self.held {
                    self.percent = self.mouse_pos_to_percent(state.logical_point().x);
                    self.update_restrict_position();
                }
                self.held = false;
                changed
            }
            crate::event::Event::Pointer(PointerEvent::Move(pu)) => {
                self.id.request_layout();
                if self.held {
                    self.percent = self.mouse_pos_to_percent(pu.current.logical_point().x);
                    true
                } else {
                    // Call hover callback with the percentage at the current position
                    if let Some(onhover) = &self.onhover {
                        let hover_percent = self.mouse_pos_to_percent(pu.current.logical_point().x);
                        onhover(Pct(hover_percent));
                    }
                    false
                }
            }
            crate::event::Event::FocusLost => {
                self.held = false;
                false
            }
            crate::event::Event::Key(KeyboardEvent {
                state: KeyState::Down,
                key,
                ..
            }) => {
                if *key == Key::Named(NamedKey::ArrowLeft) {
                    self.id.request_layout();
                    self.percent -= 10.;
                    true
                } else if *key == Key::Named(NamedKey::ArrowRight) {
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
                onchangepct(Pct(self.percent))
            }
            if let Some(onchangevalue) = &self.onchangevalue {
                let value_range = self.range.end() - self.range.start();
                let mut new_value = self.range.start() + (value_range * (self.percent / 100.0));

                if let Some(step) = self.step {
                    new_value = (new_value / step).round() * step;
                }

                onchangevalue(new_value);
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
            cx.window_state.request_paint(self.id);
        }
    }

    fn compute_layout(
        &mut self,
        _cx: &mut crate::context::ComputeLayoutCx,
    ) -> Option<peniko::kurbo::Rect> {
        self.update_restrict_position();
        let layout = self.id.get_layout().unwrap_or_default();

        self.size = layout.size;

        let circle_radius = self.calculate_handle_radius();
        let width = self.size.width as f64 - circle_radius * 2.;
        let center = width * (self.percent / 100.) + circle_radius;
        let circle_point = Point::new(center, (self.size.height / 2.) as f64);
        self.handle = crate::kurbo::Circle::new(circle_point, circle_radius);

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

        let base_bar_radii = border_radius(&self.base_bar_style, base_bar_height / 2.);
        let accent_bar_radii = border_radius(&self.accent_bar_style, accent_bar_height / 2.);

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
        .to_rounded_rect(base_bar_radii);
        self.accent_bar = peniko::kurbo::Rect::new(
            bar_x_start,
            accent_bar_y_start,
            self.handle_center(),
            accent_bar_y_start + accent_bar_height,
        )
        .to_rounded_rect(accent_bar_radii);

        self.prev_percent = self.percent;

        None
    }

    fn paint(&mut self, cx: &mut crate::context::PaintCx) {
        cx.fill(
            &self.base_bar,
            &self
                .base_bar_style
                .color()
                .unwrap_or(palette::css::BLACK.into()),
            0.,
        );
        cx.save();
        // this clip doesn't currently work because clipping only clips to the bounds of a rectangle, not including border radius.
        cx.clip(&self.base_bar);
        cx.fill(
            &self.accent_bar,
            &self
                .accent_bar_style
                .color()
                .unwrap_or(palette::css::TRANSPARENT.into()),
            0.,
        );
        cx.restore();

        if let Some(color) = self.style.foreground() {
            cx.fill(&self.handle, &color, 0.);
        }
    }
}
impl Slider {
    /// Create a new reactive slider.
    ///
    /// This does **not** automatically hook up any `on_update` logic.
    /// You will need to manually call [`Slider::on_change_pct`] or [`Slider::on_change_px`] in order to respond to updates from the slider.
    ///
    /// You might want to use the simpler constructor [`Slider::new_rw`] which will automatically hook up the `on_update` logic for updating a signal directly.
    ///
    /// # Example
    /// ```rust
    /// # use floem::prelude::*;
    /// let percent = RwSignal::new(40.pct());
    ///
    /// slider::Slider::new(move || percent.get())
    ///     .on_change_pct(move |new_percent| percent.set(new_percent))
    ///     .slider_style(|s| {
    ///         s.handle_radius(0)
    ///             .bar_radius(25.pct())
    ///             .accent_bar_radius(25.pct())
    ///     })
    ///     .style(|s| s.width(200));
    /// ```
    pub fn new<P: Into<Pct>>(percent: impl Fn() -> P + 'static) -> Self {
        let id = ViewId::new();
        let percent = UpdaterEffect::new(
            move || {
                let percent = percent().into();
                percent.0
            },
            move |percent| {
                id.update_state(SliderUpdate::Percent(percent));
            },
        );
        Slider {
            id,
            onchangepx: None,
            onchangepct: None,
            onchangevalue: None,
            onhover: None,
            held: false,
            percent,
            prev_percent: 0.0,
            handle: Default::default(),
            base_bar_style: Default::default(),
            accent_bar_style: Default::default(),
            base_bar: Default::default(),
            accent_bar: Default::default(),
            size: Default::default(),
            style: Default::default(),
            range: 0.0..=100.0,
            step: None,
        }
        .class(SliderClass)
    }

    /// Create a new reactive slider.
    ///
    /// This automatically hooks up the `on_update` logic and keeps the signal up to date.
    ///
    /// If you need more control over the getting and setting of the value you will want to use [`Slider::new`] which gives you more control but does not automatically keep a signal up to date.
    ///
    /// # Example
    /// ```rust
    /// # use floem::prelude::*;
    /// let percent = RwSignal::new(40.pct());
    ///
    /// slider::Slider::new_rw(percent)
    ///     .slider_style(|s| {
    ///         s.handle_radius(0)
    ///             .bar_radius(25.pct())
    ///             .accent_bar_radius(25.pct())
    ///     })
    ///     .style(|s| s.width(200));
    /// ```
    pub fn new_rw(percent: impl SignalGet<Pct> + SignalUpdate<Pct> + Copy + 'static) -> Self {
        Self::new(move || percent.get()).on_change_pct(move |pct| percent.set(pct))
    }

    /// Create a new reactive, ranged slider.
    ///
    /// This does **not** automatically hook up any `on_update` logic.
    /// You will need to manually call [`Slider::on_change_value`] in order to respond to updates from the slider.
    ///
    /// # Example
    /// ```rust
    /// # use floem::prelude::*;
    /// let value = RwSignal::new(-25.0);
    /// let range = -50.0..=100.0;
    ///
    /// slider::Slider::new_ranged(move || value.get(), range)
    ///     .step(5.0)
    ///     .on_change_value(move |new_value| value.set(new_value))
    ///     .slider_style(|s| {
    ///         s.handle_radius(0)
    ///             .bar_radius(25.pct())
    ///             .accent_bar_radius(25.pct())
    ///     })
    ///     .style(|s| s.width(200));
    /// ```
    pub fn new_ranged(value: impl Fn() -> f64 + 'static, range: RangeInclusive<f64>) -> Self {
        let id = ViewId::new();

        let cloned_range = range.clone();

        let percent = UpdaterEffect::new(
            move || {
                let value_range = range.end() - range.start();
                ((value() - range.start()) / value_range) * 100.0
            },
            move |percent| {
                id.update_state(SliderUpdate::Percent(percent));
            },
        );
        Slider {
            id,
            onchangepx: None,
            onchangepct: None,
            onchangevalue: None,
            onhover: None,
            held: false,
            percent,
            prev_percent: 0.0,
            handle: Default::default(),
            base_bar_style: Default::default(),
            accent_bar_style: Default::default(),
            base_bar: Default::default(),
            accent_bar: Default::default(),
            size: Default::default(),
            style: Default::default(),
            range: cloned_range,
            step: None,
        }
        .class(SliderClass)
    }

    fn update_restrict_position(&mut self) {
        self.percent = self.percent.clamp(0., 100.);
    }

    fn handle_center(&self) -> f64 {
        let width = self.size.width as f64 - self.handle.radius * 2.;
        width * (self.percent / 100.) + self.handle.radius
    }

    /// Calculate the handle radius based on current size and style
    fn calculate_handle_radius(&self) -> f64 {
        match self.style.handle_radius() {
            PxPct::Px(px) => px,
            PxPct::Pct(pct) => self.size.width.min(self.size.height) as f64 / 2. * (pct / 100.),
        }
    }

    /// Convert mouse x position to percentage, taking handle radius into account
    fn mouse_pos_to_percent(&self, mouse_x: f64) -> f64 {
        if self.size.width == 0.0 {
            return 0.0;
        }

        let handle_radius = self.calculate_handle_radius();

        // Clamp mouse position to handle center bounds
        let clamped_x = mouse_x.clamp(handle_radius, self.size.width as f64 - handle_radius);

        // Convert to percentage within the available range
        let available_width = self.size.width as f64 - handle_radius * 2.;
        if available_width <= 0.0 {
            return 0.0;
        }

        let relative_pos = clamped_x - handle_radius;
        (relative_pos / available_width * 100.0).clamp(0.0, 100.0)
    }

    /// Add an event handler to be run when the slider is moved.
    ///
    /// Only one callback of pct can be set on this view.
    /// Calling it again will clear the previously set callback.
    ///
    /// You can set [`Slider::on_change_px`], [`Slider::on_change_value`]  and `on_change_pct` callbacks at the same time and both will be called on change.
    pub fn on_change_pct(mut self, onchangepct: impl Fn(Pct) + 'static) -> Self {
        self.onchangepct = Some(Box::new(onchangepct));
        self
    }
    /// Add an event handler to be run when the slider is moved.
    ///
    /// Only one callback of px can be set on this view.
    /// Calling it again will clear the previously set callback.
    ///
    /// You can set [`Slider::on_change_pct`], [`Slider::on_change_value`]  and `on_change_px` callbacks at the same time and both will be called on change.
    pub fn on_change_px(mut self, onchangepx: impl Fn(f64) + 'static) -> Self {
        self.onchangepx = Some(Box::new(onchangepx));
        self
    }

    /// Add an event handler to be run when the slider is moved.
    ///
    /// This will emit the actual value of the slider according to the current range and step.
    ///
    /// Only one callback of value can be set on this view.
    /// Calling it again will clear the previously set callback.
    ///
    /// You can set [`Slider::on_change_pct`], [`Slider::on_change_px`]  and `on_change_value` callbacks at the same time and both will be called on change.
    pub fn on_change_value(mut self, onchangevalue: impl Fn(f64) + 'static) -> Self {
        self.onchangevalue = Some(Box::new(onchangevalue));
        self
    }

    /// Add an event handler to be run when the mouse hovers over the slider.
    ///
    /// The callback receives the percentage value at the current hover position.
    /// Only one hover callback can be set on this view.
    /// Calling it again will clear the previously set callback.
    pub fn on_hover(mut self, onhover: impl Fn(Pct) + 'static) -> Self {
        self.onhover = Some(Box::new(onhover));
        self
    }

    /// Sets the custom style properties of the `Slider`.
    pub fn slider_style(
        self,
        style: impl Fn(SliderCustomStyle) -> SliderCustomStyle + 'static,
    ) -> Self {
        self.custom_style(style)
    }

    /// Sets the step spacing of the `Slider`.
    pub fn step(mut self, step: f64) -> Self {
        self.step = Some(step);
        self
    }
}

#[derive(Debug, Default, Clone)]
pub struct SliderCustomStyle(Style);
impl From<SliderCustomStyle> for Style {
    fn from(val: SliderCustomStyle) -> Self {
        val.0
    }
}
impl From<Style> for SliderCustomStyle {
    fn from(val: Style) -> Self {
        Self(val)
    }
}
impl CustomStyle for SliderCustomStyle {
    type StyleClass = SliderClass;
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
}

#[cfg(test)]
mod test {

    use dpi::PhysicalPosition;
    use ui_events::pointer::{
        PointerButton, PointerButtonEvent, PointerInfo, PointerState, PointerType, PointerUpdate,
    };

    use crate::{
        WindowState,
        context::{EventCx, UpdateCx},
        event::Event,
    };

    use super::*;

    // Test helper to create a minimal WindowState
    fn create_test_window_state(view_id: ViewId) -> WindowState {
        WindowState::new(view_id, None)
    }

    // Test helper to create UpdateCx
    fn create_test_update_cx(view_id: ViewId) -> UpdateCx<'static> {
        UpdateCx {
            window_state: Box::leak(Box::new(create_test_window_state(view_id))),
        }
    }

    // Test helper to create EventCx
    fn create_test_event_cx(view_id: ViewId) -> EventCx<'static> {
        EventCx {
            window_state: Box::leak(Box::new(create_test_window_state(view_id))),
            skip_children_for: None,
        }
    }

    // Helper to directly update slider value
    fn update_slider_value(slider: &mut Slider, value: f64) {
        let mut cx = create_test_update_cx(slider.id());
        let state = Box::new(SliderUpdate::Percent(value));
        slider.update(&mut cx, state);
    }

    #[test]
    fn test_slider_initial_value() {
        let percent = 53.0;
        let slider = Slider::new(move || percent);
        assert_eq!(slider.percent, percent as f64);
    }

    #[test]
    fn test_slider_bounds() {
        let mut slider = Slider::new(|| 0.0);

        // Test upper bound
        update_slider_value(&mut slider, 150.0);
        slider.update_restrict_position();
        assert_eq!(slider.percent, 100.0);

        // Test lower bound
        update_slider_value(&mut slider, -50.0);
        slider.update_restrict_position();
        assert_eq!(slider.percent, 0.0);
    }

    #[test]
    fn test_slider_pointer_events() {
        let mut slider = Slider::new(|| 0.0);
        let mut cx = create_test_event_cx(slider.id());

        // Set initial size for pointer calculations
        slider.size = taffy::prelude::Size {
            width: 100.0,
            height: 20.0,
        };

        let mouse_x = 75.;

        // Test pointer down at 75%
        let pointer_down = Event::Pointer(PointerEvent::Down(PointerButtonEvent {
            state: PointerState {
                position: dpi::PhysicalPosition::new(mouse_x, 10.0),
                count: 1,
                ..Default::default()
            },
            button: Some(PointerButton::Primary),
            pointer: PointerInfo {
                pointer_id: None,
                persistent_device_id: None,
                pointer_type: PointerType::Mouse,
            },
        }));

        slider.event_before_children(&mut cx, &pointer_down);
        slider.update_restrict_position();

        // Calculate expected percentage using the same logic as the slider
        let handle_radius = slider.calculate_handle_radius();
        let available_width = slider.size.width as f64 - handle_radius * 2.0;
        let clamped_x = mouse_x.clamp(handle_radius, slider.size.width as f64 - handle_radius);
        let relative_pos = clamped_x - handle_radius;
        let expected_percent = (relative_pos / available_width * 100.0).clamp(0.0, 100.0);

        assert_eq!(slider.percent, expected_percent);
        assert!(slider.held);
        assert_eq!(cx.window_state.active, Some(slider.id()));
    }

    #[test]
    fn test_slider_drag_state() {
        let mut slider = Slider::new(|| 50.0);
        let mut cx = create_test_event_cx(slider.id());

        slider.size = taffy::prelude::Size {
            width: 100.0,
            height: 20.0,
        };

        let move_mouse_x = 75.;

        // Start drag
        let pointer_down = Event::Pointer(PointerEvent::Down(PointerButtonEvent {
            state: PointerState {
                position: PhysicalPosition::new(50.0, 10.0),
                count: 1,
                ..Default::default()
            },
            button: Some(PointerButton::Primary),
            pointer: PointerInfo {
                pointer_id: None,
                persistent_device_id: None,
                pointer_type: PointerType::Mouse,
            },
        }));

        slider.event_before_children(&mut cx, &pointer_down);
        assert!(slider.held);
        assert_eq!(cx.window_state.active, Some(slider.id()));

        // Move while dragging
        let pointer_move = Event::Pointer(PointerEvent::Move(PointerUpdate {
            pointer: PointerInfo {
                pointer_id: None,
                persistent_device_id: None,
                pointer_type: PointerType::Mouse,
            },
            current: PointerState {
                position: PhysicalPosition::new(move_mouse_x, 10.0),
                count: 1,
                ..Default::default()
            },
            coalesced: Vec::new(),
            predicted: Vec::new(),
        }));
        slider.event_before_children(&mut cx, &pointer_move);

        // Calculate expected percentage using the same logic as the slider
        let handle_radius = slider.calculate_handle_radius();
        let available_width = slider.size.width as f64 - handle_radius * 2.0;
        let clamped_x = move_mouse_x.clamp(handle_radius, slider.size.width as f64 - handle_radius);
        let relative_pos = clamped_x - handle_radius;
        let expected_percent = (relative_pos / available_width * 100.0).clamp(0.0, 100.0);

        assert_eq!(slider.percent, expected_percent);

        // End drag
        let pointer_up = Event::Pointer(PointerEvent::Up(PointerButtonEvent {
            state: PointerState {
                position: PhysicalPosition::new(75.0, 10.0),
                count: 1,
                ..Default::default()
            },
            button: Some(PointerButton::Primary),
            pointer: PointerInfo {
                pointer_id: None,
                persistent_device_id: None,
                pointer_type: PointerType::Mouse,
            },
        }));

        slider.event_before_children(&mut cx, &pointer_up);
        assert!(!slider.held);
    }

    #[test]
    fn test_callback_handling() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, Ordering};

        let callback_called = Arc::new(AtomicBool::new(false));
        let callback_called_clone = callback_called.clone();

        let mut slider = Slider::new(|| 0.0).on_change_pct(move |_| {
            callback_called_clone.store(true, Ordering::SeqCst);
        });

        let mut cx = create_test_event_cx(slider.id());

        slider.size = taffy::prelude::Size {
            width: 100.0,
            height: 20.0,
        };

        let pointer_event = Event::Pointer(PointerEvent::Down(PointerButtonEvent {
            state: PointerState {
                position: PhysicalPosition::new(60.0, 10.0),
                count: 1,
                ..Default::default()
            },
            button: Some(PointerButton::Primary),
            pointer: PointerInfo {
                pointer_id: None,
                persistent_device_id: None,
                pointer_type: PointerType::Mouse,
            },
        }));

        slider.event_before_children(&mut cx, &pointer_event);
        slider.update_restrict_position();

        assert!(callback_called.load(Ordering::SeqCst));
    }

    // #[test]
    // FIXME
    // fn test_handle_positioning_edge_cases() {
    //     let mut slider = Slider::new(|| 0.0);
    //     let mut cx = create_test_event_cx(slider.id());

    //     slider.size = taffy::prelude::Size {
    //         width: 100.0,
    //         height: 20.0,
    //     };

    //     let handle_radius = slider.calculate_handle_radius();

    //     // Test mouse at far left (should result in 0%)
    //     let pointer_left = Event::Pointer(PointerEvent::Down(PointerButtonEvent{
    //         pos: Point::new(0.0, 10.0),
    //         button: PointerButton::Mouse(MouseButton::Primary),
    //         count: 1,
    //         modifiers: Default::default(),
    //     });

    //     slider.event_before_children(&mut cx, &pointer_left);
    //     assert_eq!(slider.percent, 0.0);

    //     // Test mouse at far right (should result in 100%)
    //     let pointer_right = Event::PointerDown(PointerInputEvent {
    //         pos: Point::new(100.0, 10.0),
    //         button: PointerButton::Mouse(MouseButton::Primary),
    //         count: 1,
    //         modifiers: Default::default(),
    //     });

    //     slider.event_before_children(&mut cx, &pointer_right);
    //     assert_eq!(slider.percent, 100.0);

    //     // Test mouse exactly at handle radius (should result in 0%)
    //     let pointer_at_radius = Event::PointerDown(PointerInputEvent {
    //         pos: Point::new(handle_radius, 10.0),
    //         button: PointerButton::Mouse(MouseButton::Primary),
    //         count: 1,
    //         modifiers: Default::default(),
    //     });

    //     slider.event_before_children(&mut cx, &pointer_at_radius);
    //     assert_eq!(slider.percent, 0.0);

    //     // Test mouse at width - handle_radius (should result in 100%)
    //     let pointer_at_end = Event::PointerDown(PointerInputEvent {
    //         pos: Point::new(slider.size.width as f64 - handle_radius, 10.0),
    //         button: PointerButton::Mouse(MouseButton::Primary),
    //         count: 1,
    //         modifiers: Default::default(),
    //     });

    //     slider.event_before_children(&mut cx, &pointer_at_end);
    //     assert_eq!(slider.percent, 100.0);
    // }
}
