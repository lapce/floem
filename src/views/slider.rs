//! A toggle button widget. An example can be found in widget-gallery/button in the floem examples.

use floem_reactive::{create_updater, SignalGet, SignalUpdate};
use peniko::kurbo::{Circle, Point, RoundedRect};
use peniko::{Brush, Color};
use winit::keyboard::{Key, NamedKey};

use crate::unit::Pct;
use crate::{
    event::EventPropagation,
    id::ViewId,
    prop, prop_extractor,
    style::{Background, BorderRadius, CustomStylable, Foreground, Height, Style},
    style_class,
    unit::{PxPct, PxPctAuto},
    view::View,
    views::Decorators,
    Renderer,
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
/// You can use the [Slider::slider_style] method to get access to a [SliderCustomStyle] which has convenient functions with documentation for styling all of the properties of the slider.
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
///             .bar_color(Color::BLACK)
///             .bar_radius(100.pct())
///             .accent_bar_color(Color::GREEN)
///             .accent_bar_radius(100.pct())
///             .accent_bar_height(100.pct())
///     });
///```
pub struct Slider {
    id: ViewId,
    onchangepx: Option<Box<dyn Fn(f64)>>,
    onchangepct: Option<Box<dyn Fn(Pct)>>,
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
                self.percent = event.pos.x / self.size.width as f64 * 100.;
                true
            }
            crate::event::Event::PointerUp(event) => {
                self.id.request_layout();

                // set the state based on the position of the slider
                let changed = self.held;
                if self.held {
                    self.percent = event.pos.x / self.size.width as f64 * 100.;
                    self.update_restrict_position();
                }
                self.held = false;
                changed
            }
            crate::event::Event::PointerMove(event) => {
                self.id.request_layout();
                if self.held {
                    self.percent = event.pos.x / self.size.width as f64 * 100.;
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
                onchangepct(Pct(self.percent))
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
            PxPct::Px(px) => px,
            PxPct::Pct(pct) => self.size.width.min(self.size.height) as f64 / 2. * (pct / 100.),
        };
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
            self.handle_center(),
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
        // this clip doesn't currently work because clipping only clips to the bounds of a rectangle, not including border radius.
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
    /// Create a new reactive slider.
    ///
    /// This does **not** automatically hook up any `on_update` logic.
    /// You will need to manually call [Slider::on_change_pct] or [Slider::on_change_px] in order to respond to updates from the slider.
    ///
    /// You might want to use the simpler constructor [Slider::new_rw] which will automatically hook up the on_update logic for updating a signal directly.
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
        let percent = create_updater(
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
        }
        .class(SliderClass)
        .keyboard_navigable()
    }

    /// Create a new reactive slider.
    ///
    /// This automatically hooks up the `on_update` logic and keeps the signal up to date.
    ///
    /// If you need more control over the getting and setting of the value you will want to use [Slider::new] which gives you more control but does not automatically keep a signal up to date.
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

    fn update_restrict_position(&mut self) {
        self.percent = self.percent.clamp(0., 100.);
    }

    fn handle_center(&self) -> f64 {
        let width = self.size.width as f64 - self.handle.radius * 2.;
        width * (self.percent / 100.) + self.handle.radius
    }

    /// Add an event handler to be run when the slider is moved.
    ///
    /// Only one callback of pct can be set on this view.
    /// Calling it again will clear the previously set callback.
    ///
    /// You can set both an `on_change_pct` and [Slider::on_change_px] callbacks at the same time and both will be called on change.
    pub fn on_change_pct(mut self, onchangepct: impl Fn(Pct) + 'static) -> Self {
        self.onchangepct = Some(Box::new(onchangepct));
        self
    }
    /// Add an event handler to be run when the slider is moved.
    ///
    /// Only one callback of px can be set on this view.
    /// Calling it again will clear the previously set callback.
    ///
    /// You can set both an [Slider::on_change_pct] and `on_change_px` callbacks at the same time and both will be called on change.
    pub fn on_change_px(mut self, onchangepx: impl Fn(f64) + 'static) -> Self {
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
}

#[cfg(test)]
mod test {

    use crate::{
        context::{EventCx, UpdateCx},
        event::Event,
        pointer::{PointerButton, PointerInputEvent, PointerMoveEvent},
        AppState,
    };

    use super::*;

    // Test helper to create a minimal AppState
    fn create_test_app_state(view_id: ViewId) -> AppState {
        AppState::new(view_id)
    }

    // Test helper to create UpdateCx
    fn create_test_update_cx(view_id: ViewId) -> UpdateCx<'static> {
        UpdateCx {
            app_state: Box::leak(Box::new(create_test_app_state(view_id))),
        }
    }

    // Test helper to create EventCx
    fn create_test_event_cx(view_id: ViewId) -> EventCx<'static> {
        EventCx {
            app_state: Box::leak(Box::new(create_test_app_state(view_id))),
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

        // Test pointer down at 75%
        let pointer_down = Event::PointerDown(PointerInputEvent {
            count: 1,
            pos: Point::new(75.0, 10.0),
            button: PointerButton::Primary,
            modifiers: Default::default(),
        });

        slider.event_before_children(&mut cx, &pointer_down);
        slider.update_restrict_position();

        assert_eq!(slider.percent, 75.0);
        assert!(slider.held);
        assert_eq!(cx.app_state.active, Some(slider.id()));
    }

    #[test]
    fn test_slider_drag_state() {
        let mut slider = Slider::new(|| 50.0);
        let mut cx = create_test_event_cx(slider.id());

        slider.size = taffy::prelude::Size {
            width: 100.0,
            height: 20.0,
        };

        // Start drag
        let pointer_down = Event::PointerDown(PointerInputEvent {
            pos: Point::new(50.0, 10.0),
            button: PointerButton::Primary,
            count: 1,
            modifiers: Default::default(),
        });

        slider.event_before_children(&mut cx, &pointer_down);
        assert!(slider.held);
        assert_eq!(cx.app_state.active, Some(slider.id()));

        // Move while dragging
        let pointer_move = Event::PointerMove(PointerMoveEvent {
            pos: Point::new(75.0, 10.0),
            modifiers: Default::default(),
        });

        slider.event_before_children(&mut cx, &pointer_move);
        assert_eq!(slider.percent, 75.0);

        // End drag
        let pointer_up = Event::PointerUp(PointerInputEvent {
            pos: Point::new(75.0, 10.0),
            button: PointerButton::Primary,
            count: 1,
            modifiers: Default::default(),
        });

        slider.event_before_children(&mut cx, &pointer_up);
        assert!(!slider.held);
    }

    #[test]
    fn test_callback_handling() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;

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

        let pointer_event = Event::PointerDown(PointerInputEvent {
            pos: Point::new(60.0, 10.0),
            button: PointerButton::Primary,
            count: 1,
            modifiers: Default::default(),
        });

        slider.event_before_children(&mut cx, &pointer_event);
        slider.update_restrict_position();

        assert!(callback_called.load(Ordering::SeqCst));
    }
}
