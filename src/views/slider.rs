//! A toggle button widget. An example can be found in widget-gallery/button in the floem examples.

use std::ops::RangeInclusive;

use floem_reactive::{SignalGet, SignalUpdate, UpdaterEffect};
use peniko::Brush;
use peniko::color::palette;
use peniko::kurbo::{Circle, Point, RoundedRect, RoundedRectRadii};
use ui_events::keyboard::{Key, KeyState, KeyboardEvent, NamedKey};
use ui_events::pointer::{PointerButtonEvent, PointerEvent};

use crate::custom_event;
use crate::event::CustomEvent;
use crate::{
    Renderer,
    context::{LayoutChanged, LayoutChangedListener},
    event::{Event, EventPropagation, FocusEvent},
    prelude::*,
    prop, prop_extractor,
    style::{
        Background, BorderBottomLeftRadius, BorderBottomRightRadius, BorderTopLeftRadius,
        BorderTopRightRadius, CustomStylable, CustomStyle, Foreground, Height, Style,
    },
    style_class,
    unit::{Pct, PxPct, PxPctAuto},
    view::{View, ViewId},
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
        border_top_left_radius: BorderTopLeftRadius,
        border_top_right_radius: BorderTopRightRadius,
        border_bottom_left_radius: BorderBottomLeftRadius,
        border_bottom_right_radius: BorderBottomRightRadius,
        color: Background,
        height: Height

    }
}

impl BarStyle {
    fn border_radius(&self) -> crate::style::BorderRadius {
        crate::style::BorderRadius {
            top_left: Some(self.border_top_left_radius()),
            top_right: Some(self.border_top_right_radius()),
            bottom_left: Some(self.border_bottom_left_radius()),
            bottom_right: Some(self.border_bottom_right_radius()),
        }
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

/// State of a slider at a point in time
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct SliderState {
    /// The value in pixels from the start of the slider
    pub px: f64,
    /// The value as a percentage (0.0 to 100.0)
    pub pct: Pct,
    /// The value mapped to the slider's configured range
    pub value: f64,
}

impl SliderState {
    /// Create a new slider state from a percentage
    pub fn from_percent(
        percent: f64,
        range: &RangeInclusive<f64>,
        step: Option<f64>,
        px: f64,
    ) -> Self {
        let value_range = range.end() - range.start();
        let mut value = range.start() + (value_range * (percent / 100.0));

        if let Some(step) = step {
            value = (value / step).round() * step;
        }

        Self {
            px,
            pct: Pct(percent),
            value,
        }
    }
}

impl SliderChanged {
    // we need these functions instead of closures because we need rust to know that the lifetimes of the references are the same
    fn extract_state(event: &SliderChanged) -> &SliderState {
        &event.state
    }
}

impl SliderHover {
    // we need these functions instead of closures because we need rust to know that the lifetimes of the references are the same
    fn extract_state(event: &SliderHover) -> &SliderState {
        &event.state
    }
}

/// Event fired when a slider's value changes
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SliderChanged {
    /// The new state of the slider
    pub state: SliderState,
}
custom_event!(SliderChanged, SliderState, SliderChanged::extract_state);

/// Event fired that has what the state would be at the current mouse position when hovering over a slider
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SliderHover {
    /// The state of the slider
    pub state: SliderState,
}
custom_event!(SliderHover, SliderState, SliderHover::extract_state);

/// **A reactive slider.**
///
/// You can set the slider to a percent value between 0 and 100.
///
/// The slider is composed of four parts: the main view, the background bar, an accent bar, and a handle.
/// The background bar is separate from the main view because it is shortened when `EdgeAlign` is set to false.
///
/// **Responding to events**:
/// You can respond to slider changes by listening to `SliderChanged` events:
/// ```rust
/// # use floem::event::EventPropagation;
/// # use floem::prelude::*;
/// # use floem::views::slider::{self, SliderChanged};
/// slider::Slider::new(|| 40.pct())
///     .on_event(SliderChanged::listener(), |cx, state| {
///         println!("Value: {}", state.value);
///         println!("Percent: {:?}", state.pct);
///         println!("Pixels: {}", state.px);
///         EventPropagation::Continue
///     });
/// ```
///
/// You can also listen to `SliderHover` events to respond when the user hovers over the slider:
/// ```rust
/// # use floem::event::EventPropagation;
/// # use floem::prelude::*;
/// # use floem::views::slider::{self, SliderHover};
/// slider::Slider::new(|| 40.pct())
///     .on_event(SliderHover::listener(), |cx, state| {
///         println!("Hovering at: {:?}", state.pct);
///         EventPropagation::Continue
///     });
/// ```
///
/// These events are only fired on user interaction (mouse events or arrow keys), not on reactive updates.
///
/// You can also disable event handling with `Decorators::disabled`. This is useful if you want to use
/// the slider as a progress bar.
///
/// **Styling**:
/// You can use the `Slider::slider_style` method to get access to a `SliderCustomStyle` which has
/// convenient functions with documentation for styling all of the properties of the slider.
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
    held: bool,
    state: SliderState,
    prev_percent: f64,
    base_bar_style: BarStyle,
    accent_bar_style: BarStyle,
    handle: Circle,
    base_bar: RoundedRect,
    accent_bar: RoundedRect,
    style: SliderStyle,
    range: RangeInclusive<f64>,
    step: Option<f64>,
    layout: LayoutChanged,
}

impl View for Slider {
    fn id(&self) -> ViewId {
        self.id
    }

    fn update(&mut self, _cx: &mut crate::context::UpdateCx, state: Box<dyn std::any::Any>) {
        if let Ok(update) = state.downcast::<SliderUpdate>() {
            match *update {
                SliderUpdate::Percent(percent) => {
                    self.state = SliderState::from_percent(
                        percent,
                        &self.range,
                        self.step,
                        self.handle_center_for_percent(percent),
                    );
                }
            }
            self.update_shapes();
        }
    }

    fn event(&mut self, cx: &mut crate::context::EventCx) -> EventPropagation {
        if let Some(new_layout) = LayoutChangedListener::extract(&cx.event) {
            self.post_layout(new_layout);
        }

        let pos_changed = match &cx.event {
            Event::Pointer(PointerEvent::Down(PointerButtonEvent { state, pointer, .. })) => {
                if let Some(pointer_id) = pointer.pointer_id {
                    cx.window_state.set_pointer_capture(pointer_id, self.id);
                }
                self.held = true;
                self.update_state_from_mouse_pos(state.logical_point().x);
                true
            }
            Event::Pointer(PointerEvent::Up(PointerButtonEvent { state, .. })) => {
                let changed = self.held;
                if self.held {
                    self.update_state_from_mouse_pos(state.logical_point().x);
                    self.clamp_percent();
                }
                self.held = false;
                changed
            }
            Event::Pointer(PointerEvent::Move(pu)) => {
                if self.held {
                    self.update_state_from_mouse_pos(pu.current.logical_point().x);
                    true
                } else {
                    // Dispatch hover event with state at current position
                    let hover_state = self.state_from_mouse_pos(pu.current.logical_point().x);
                    self.id.route_event(
                        Event::new_custom(SliderHover { state: hover_state }),
                        crate::event::RouteKind::Directed {
                            target: self.id.get_element_id(),
                            phases: crate::context::Phases::TARGET,
                        },
                    );
                    false
                }
            }
            Event::Focus(FocusEvent::Lost) => {
                self.held = false;
                false
            }
            Event::Key(KeyboardEvent {
                state: KeyState::Down,
                key,
                ..
            }) => {
                if *key == Key::Named(NamedKey::ArrowLeft) {
                    let new_percent = (self.state.pct.0 - 10.).clamp(0., 100.);
                    self.update_state_from_percent(new_percent);
                    true
                } else if *key == Key::Named(NamedKey::ArrowRight) {
                    let new_percent = (self.state.pct.0 + 10.).clamp(0., 100.);
                    self.update_state_from_percent(new_percent);
                    true
                } else {
                    false
                }
            }
            _ => false,
        };

        self.clamp_percent();

        if pos_changed && self.state.pct.0 != self.prev_percent {
            self.id.route_event(
                Event::new_custom(SliderChanged { state: self.state }),
                crate::event::RouteKind::Directed {
                    target: self.id.get_element_id(),
                    phases: crate::context::Phases::TARGET,
                },
            );
            self.update_shapes();
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

    fn paint(&mut self, cx: &mut crate::context::PaintCx) {
        cx.fill(
            &self.base_bar,
            &self
                .base_bar_style
                .color()
                .unwrap_or(palette::css::BLACK.into()),
            0.,
        );
        // Apply temporary clip for accent bar
        cx.clip(&self.base_bar);
        cx.fill(
            &self.accent_bar,
            &self
                .accent_bar_style
                .color()
                .unwrap_or(palette::css::TRANSPARENT.into()),
            0.,
        );
        cx.clear_clip();

        if let Some(color) = self.style.foreground() {
            cx.fill(&self.handle, &color, 0.);
        }
    }
}

impl Slider {
    /// Create a new reactive slider.
    ///
    /// Listen to slider changes using the `SliderChanged` event:
    ///
    /// # Example
    /// ```rust
    /// # use floem::event::EventPropagation;
    /// # use floem::prelude::*;
    /// # use floem::views::slider::{self, SliderChanged};
    /// let percent = RwSignal::new(40.pct());
    ///
    /// slider::Slider::new(move || percent.get())
    ///     .on_event(SliderChanged::listener(), move |cx, event| {
    ///         percent.set(event.pct);
    ///         EventPropagation::Continue
    ///     })
    ///     .slider_style(|s| {
    ///         s.handle_radius(0)
    ///             .bar_radius(25.pct())
    ///             .accent_bar_radius(25.pct())
    ///     })
    ///     .style(|s| s.width(200));
    /// ```
    pub fn new<P: Into<Pct>>(percent: impl Fn() -> P + 'static) -> Self {
        let id = ViewId::new();
        id.register_listener(LayoutChanged::listener_key());
        let initial_percent = UpdaterEffect::new(
            move || {
                let percent = percent().into();
                percent.0
            },
            move |percent| {
                id.update_state(SliderUpdate::Percent(percent));
            },
        );

        let state = SliderState {
            px: 0.0,
            pct: Pct(initial_percent),
            value: initial_percent,
        };

        Slider {
            id,
            held: false,
            state,
            prev_percent: 0.0,
            handle: Default::default(),
            base_bar_style: Default::default(),
            accent_bar_style: Default::default(),
            base_bar: Default::default(),
            accent_bar: Default::default(),
            layout: LayoutChanged {
                new_box: Default::default(),
                new_content_box: Default::default(),
                new_window_origin: Default::default(),
            },
            style: Default::default(),
            range: 0.0..=100.0,
            step: None,
        }
        .class(SliderClass)
    }

    /// Create a new reactive slider.
    ///
    /// This automatically hooks up the event logic and keeps the signal up to date.
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
        Self::new(move || percent.get()).on_event(SliderChanged::listener(), move |_cx, state| {
            percent.set(state.pct);
            EventPropagation::Continue
        })
    }

    /// Create a new reactive, ranged slider.
    ///
    /// Listen to value changes using the `SliderChanged` event and read `event.state.value`.
    ///
    /// # Example
    /// ```rust
    /// # use floem::event::EventPropagation;
    /// # use floem::prelude::*;
    /// # use floem::views::slider::{self, SliderChanged};
    /// let value = RwSignal::new(-25.0);
    /// let range = -50.0..=100.0;
    ///
    /// slider::Slider::new_ranged(move || value.get(), range)
    ///     .step(5.0)
    ///     .on_event(SliderChanged::listener(), move |cx, event| {
    ///         value.set(event.value);
    ///         EventPropagation::Continue
    ///     })
    ///     .slider_style(|s| {
    ///         s.handle_radius(0)
    ///             .bar_radius(25.pct())
    ///             .accent_bar_radius(25.pct())
    ///     })
    ///     .style(|s| s.width(200));
    /// ```
    pub fn new_ranged(value: impl Fn() -> f64 + 'static, range: RangeInclusive<f64>) -> Self {
        let id = ViewId::new();
        id.register_listener(LayoutChanged::listener_key());

        let cloned_range = range.clone();

        let initial_percent = UpdaterEffect::new(
            move || {
                let value_range = range.end() - range.start();
                ((value() - range.start()) / value_range) * 100.0
            },
            move |percent| {
                id.update_state(SliderUpdate::Percent(percent));
            },
        );

        let state = SliderState::from_percent(initial_percent, &cloned_range, None, 0.0);

        Slider {
            id,
            held: false,
            state,
            prev_percent: 0.0,
            handle: Default::default(),
            base_bar_style: Default::default(),
            accent_bar_style: Default::default(),
            base_bar: Default::default(),
            accent_bar: Default::default(),
            layout: LayoutChanged {
                new_box: Default::default(),
                new_content_box: Default::default(),
                new_window_origin: Default::default(),
            },
            style: Default::default(),
            range: cloned_range,
            step: None,
        }
        .class(SliderClass)
    }

    fn post_layout(&mut self, layout_changed: &LayoutChanged) {
        self.layout = *layout_changed;
        self.update_shapes();
    }

    fn clamp_percent(&mut self) {
        let clamped = self.state.pct.0.clamp(0., 100.);
        if clamped != self.state.pct.0 {
            self.update_state_from_percent(clamped);
        }
    }

    fn handle_center(&self) -> f64 {
        self.handle_center_for_percent(self.state.pct.0)
    }

    fn handle_center_for_percent(&self, percent: f64) -> f64 {
        let width = self.layout.new_box.size().width - self.handle.radius * 2.;
        width * (percent / 100.) + self.handle.radius
    }

    /// Update the slider state from a mouse position
    fn update_state_from_mouse_pos(&mut self, mouse_x: f64) {
        let percent = self.mouse_pos_to_percent(mouse_x);
        self.update_state_from_percent(percent);
    }

    /// Update the slider state from a percentage
    fn update_state_from_percent(&mut self, percent: f64) {
        self.state = SliderState::from_percent(
            percent,
            &self.range,
            self.step,
            self.handle_center_for_percent(percent),
        );
    }

    /// Create a slider state from a mouse position without updating self
    fn state_from_mouse_pos(&self, mouse_x: f64) -> SliderState {
        let percent = self.mouse_pos_to_percent(mouse_x);
        SliderState::from_percent(
            percent,
            &self.range,
            self.step,
            self.handle_center_for_percent(percent),
        )
    }

    fn update_shapes(&mut self) {
        self.clamp_percent();
        let size = self.layout.box_local().size();

        let circle_radius = self.calculate_handle_radius();
        let width = size.width - circle_radius * 2.;
        let center = width * (self.state.pct.0 / 100.) + circle_radius;
        let circle_point = Point::new(center, size.height / 2.);
        self.handle = crate::kurbo::Circle::new(circle_point, circle_radius);

        let base_bar_height = match self.base_bar_style.height() {
            PxPctAuto::Px(px) => px,
            PxPctAuto::Pct(pct) => size.height * (pct / 100.),
            PxPctAuto::Auto => size.height,
        };
        let accent_bar_height = match self.accent_bar_style.height() {
            PxPctAuto::Px(px) => px,
            PxPctAuto::Pct(pct) => size.height * (pct / 100.),
            PxPctAuto::Auto => size.height,
        };

        let base_bar_radii = border_radius(&self.base_bar_style, base_bar_height / 2.);
        let accent_bar_radii = border_radius(&self.accent_bar_style, accent_bar_height / 2.);

        let mut base_bar_length = size.width;
        if !self.style.edge_align() {
            base_bar_length -= self.handle.radius * 2.;
        }

        let base_bar_y_start = size.height / 2. - base_bar_height / 2.;
        let accent_bar_y_start = size.height / 2. - accent_bar_height / 2.;

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

        self.prev_percent = self.state.pct.0;
        self.id.request_paint();
    }

    /// Calculate the handle radius based on current size and style
    fn calculate_handle_radius(&self) -> f64 {
        match self.style.handle_radius() {
            PxPct::Px(px) => px,
            PxPct::Pct(pct) => {
                let size = self.layout.new_box.size();
                size.width.min(size.height) / 2. * (pct / 100.)
            }
        }
    }

    /// Convert mouse x position to percentage, taking handle radius into account
    fn mouse_pos_to_percent(&self, mouse_x: f64) -> f64 {
        let size = self.layout.new_box.size();
        if size.width == 0.0 {
            return 0.0;
        }

        let handle_radius = self.calculate_handle_radius();

        // Clamp mouse position to handle center bounds
        let clamped_x = mouse_x.clamp(handle_radius, size.width - handle_radius);

        // Convert to percentage within the available range
        let available_width = size.width - handle_radius * 2.;
        if available_width <= 0.0 {
            return 0.0;
        }

        let relative_pos = clamped_x - handle_radius;
        (relative_pos / available_width * 100.0).clamp(0.0, 100.0)
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
    /// * `color` - An optional `Brush` that sets the handle's color.
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
    /// * `color` - An optional `Brush` that sets the bar's background color.
    pub fn bar_color(mut self, color: impl Into<Option<Brush>>) -> Self {
        let color = color.into();
        self = SliderCustomStyle(self.0.class(BarClass, move |s| {
            s.set(Background, color.clone())
        }));
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
    /// * `color` - A `Brush` that sets the accent bar's background color.
    pub fn accent_bar_color(mut self, color: impl Into<Brush>) -> Self {
        let color = Some(color.into());
        self = SliderCustomStyle(self.0.class(AccentBarClass, move |s| {
            s.set(Background, color.clone())
        }));
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
