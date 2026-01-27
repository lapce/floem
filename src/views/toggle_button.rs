#![deny(missing_docs)]
//! A toggle button widget. An example can be found in [widget-gallery/button](https://github.com/lapce/floem/tree/main/examples/widget-gallery)
//! in the floem examples.

use floem_reactive::{Effect, SignalGet, SignalUpdate};
use peniko::Brush;
use peniko::kurbo::Point;
use ui_events::keyboard::{Key, KeyState, KeyboardEvent, NamedKey};
use ui_events::pointer::PointerEvent;

use crate::event::{Event, FocusEvent};
use crate::{
    Renderer,
    event::EventPropagation,
    prop, prop_extractor,
    style::{self, Foreground, Style},
    style_class,
    unit::PxPct,
    view::View,
    view::ViewId,
    views::Decorators,
};

/// Controls the switching behavior of the switch.
/// The corresponding style prop is [`ToggleButtonBehavior`]
#[derive(Debug, Clone, PartialEq)]
pub enum ToggleHandleBehavior {
    /// The switch foreground item will follow the position of the cursor.
    /// The toggle event happens when the cursor passes the 50% threshold.
    Follow,
    /// The switch foreground item will "snap" from being toggled off/on
    /// when the cursor passes the 50% threshold.
    Snap,
}

impl style::StylePropValue for ToggleHandleBehavior {}

prop!(pub ToggleButtonInset: PxPct {} = PxPct::Px(0.));
prop!(pub ToggleButtonCircleRad: PxPct {} = PxPct::Pct(95.));
prop!(pub ToggleButtonBehavior: ToggleHandleBehavior {} = ToggleHandleBehavior::Snap);

prop_extractor! {
    ToggleStyle {
        foreground: Foreground,
        inset: ToggleButtonInset,
        circle_rad: ToggleButtonCircleRad,
        switch_behavior: ToggleButtonBehavior
    }
}
style_class!(
    /// A class for styling [ToggleButton] view.
    pub ToggleButtonClass
);

/// Represents [ToggleButton] toggle state.
#[derive(PartialEq, Eq)]
enum ToggleState {
    Nothing,
    Held,
    Drag,
}

/// A toggle button.
pub struct ToggleButton {
    id: ViewId,
    state: bool,
    ontoggle: Option<Box<dyn Fn(bool)>>,
    position: f64,
    held: ToggleState,
    style: ToggleStyle,
}

/// A reactive toggle button.
///
/// When the button is toggled by clicking or dragging the widget, an update will be
/// sent to the [`ToggleButton::on_toggle`] handler.
///
/// By default this toggle button has a style class of [`ToggleButtonClass`] applied
/// with a default style provided.
/// ### Examples
/// ```rust
/// # use floem::reactive::{SignalGet, SignalUpdate, RwSignal};
/// # use floem::views::toggle_button;
/// # use floem::prelude::{palette::css, ToggleHandleBehavior};
/// // An example using read-write signal
/// let state = RwSignal::new(true);
/// let toggle = toggle_button(move || state.get())
///     // Set action when button is toggled according to the toggle state provided.
///     .on_toggle(move |new_state| state.set(new_state));
///
/// // Use toggle button specific styles to control its look and behavior
/// let customized_toggle = toggle_button(move || state.get())
///     .on_toggle(move |new_state| state.set(new_state))
///     .toggle_style(|s| s
///         // Set toggle button accent color
///         .accent_color(css::REBECCA_PURPLE)
///         // Set toggle button circle radius
///         .circle_rad(5.)
///         // Set toggle button handle color
///         .handle_color(css::PURPLE)
///         // Set toggle button handle inset
///         .handle_inset(1.)
///         // Set toggle button behavior:
///         // - `Follow` - to follow the pointer movement
///         // - `Snap` - to snap once pointer passed 50% treshold
///         .behavior(ToggleHandleBehavior::Snap)
///     );
///```
/// ### Reactivity
/// This function is reactive and will reactively respond to changes.
#[deprecated]
pub fn toggle_button(state: impl Fn() -> bool + 'static) -> ToggleButton {
    ToggleButton::new(state)
}

impl View for ToggleButton {
    fn id(&self) -> ViewId {
        self.id
    }

    fn debug_name(&self) -> std::borrow::Cow<'static, str> {
        "Toggle Button".into()
    }

    fn update(&mut self, _cx: &mut crate::context::UpdateCx, state: Box<dyn std::any::Any>) {
        if let Ok(state) = state.downcast::<bool>() {
            self.state = *state;
            let size = self.id.get_layout_rect_local().size();
            let circle_radius = match self.style.circle_rad() {
                PxPct::Px(px) => px,
                PxPct::Pct(pct) => size.width.min(size.height) / 2. * (pct / 100.),
            };
            self.update_restrict_position(size.width, circle_radius);
            self.id.request_paint();
        }
    }

    fn event(&mut self, cx: &mut crate::context::EventCx) -> EventPropagation {
        if cx.phase != crate::event::Phase::Capture {
            return EventPropagation::Continue;
        }

        let size = self.id.get_content_rect_local().size();

        match &cx.event {
            Event::Pointer(PointerEvent::Down(e)) => {
                if let Some(pointer_id) = e.pointer.pointer_id {
                    cx.window_state.set_pointer_capture(pointer_id, self.id);
                }
                self.held = ToggleState::Held;
            }
            Event::Pointer(PointerEvent::Up { .. }) => {
                // if held and pointer up. toggle the position (toggle state drag already changed the position)
                if self.held == ToggleState::Held {
                    if self.position > size.width / 2. {
                        self.position = 0.;
                    } else {
                        self.position = size.width;
                    }
                }
                // set the state based on the position of the slider
                if self.held == ToggleState::Held {
                    if self.state && self.position < size.width / 2. {
                        self.state = false;
                        if let Some(ontoggle) = &self.ontoggle {
                            ontoggle(false);
                        }
                    } else if !self.state && self.position > size.width / 2. {
                        self.state = true;
                        if let Some(ontoggle) = &self.ontoggle {
                            ontoggle(true);
                        }
                    }
                }
                self.held = ToggleState::Nothing;
            }
            Event::Pointer(PointerEvent::Move(pu)) => {
                let point = pu.current.logical_point();
                if self.held == ToggleState::Held || self.held == ToggleState::Drag {
                    self.held = ToggleState::Drag;
                    match self.style.switch_behavior() {
                        ToggleHandleBehavior::Follow => {
                            self.position = point.x;
                            if self.position > size.width / 2. && !self.state {
                                self.state = true;
                                if let Some(ontoggle) = &self.ontoggle {
                                    ontoggle(true);
                                }
                            } else if self.position < size.width / 2. && self.state {
                                self.state = false;
                                if let Some(ontoggle) = &self.ontoggle {
                                    ontoggle(false);
                                }
                            }
                        }
                        ToggleHandleBehavior::Snap => {
                            if point.x > size.width / 2. && !self.state {
                                self.position = size.width;
                                self.id.request_layout();
                                self.state = true;
                                if let Some(ontoggle) = &self.ontoggle {
                                    ontoggle(true);
                                }
                            } else if point.x < size.width / 2. && self.state {
                                self.position = 0.;
                                // self.held = ToggleState::Nothing;
                                self.state = false;
                                if let Some(ontoggle) = &self.ontoggle {
                                    ontoggle(false);
                                }
                            }
                        }
                    }
                }
            }
            Event::Focus(FocusEvent::Lost) => {
                self.held = ToggleState::Nothing;
            }
            Event::Key(KeyboardEvent {
                state: KeyState::Down,
                key,
                ..
            }) => {
                if *key == Key::Named(NamedKey::Enter) {
                    if let Some(ontoggle) = &self.ontoggle {
                        ontoggle(!self.state);
                    }
                }
            }
            _ => {}
        }
        let size = self.id.get_layout_rect_local().size();
        let circle_radius = match self.style.circle_rad() {
            PxPct::Px(px) => px,
            PxPct::Pct(pct) => size.width.min(size.height) / 2. * (pct / 100.),
        };
        self.update_restrict_position(size.width, circle_radius);
        EventPropagation::Continue
    }

    fn style_pass(&mut self, cx: &mut crate::context::StyleCx<'_>) {
        if self.style.read(cx) {
            cx.window_state.request_paint(self.id);
        }
    }

    fn paint(&mut self, cx: &mut crate::context::PaintCx) {
        let size = self.id.get_layout_rect_local().size();
        let circle_radius = match self.style.circle_rad() {
            PxPct::Px(px) => px,
            PxPct::Pct(pct) => size.width.min(size.height) / 2. * (pct / 100.),
        };
        self.update_restrict_position(size.width, circle_radius);
        let circle_point = Point::new(self.position, size.to_rect().center().y);
        let circle = crate::kurbo::Circle::new(circle_point, circle_radius);
        if let Some(color) = self.style.foreground() {
            cx.fill(&circle, &color, 0.);
        }
    }
}

impl ToggleButton {
    fn update_restrict_position(&mut self, width: f64, radius: f64) {
        let inset = match self.style.inset() {
            PxPct::Px(px) => px,
            PxPct::Pct(pct) => (width * (pct / 100.)).min(width / 2.),
        };

        if self.held == ToggleState::Nothing {
            self.position = if self.state { width } else { 0. };
        }

        self.position = self
            .position
            .max(radius + inset)
            .min(width - radius - inset);
    }

    /// Create new [ToggleButton].
    ///
    /// When the button is toggled by clicking or dragging the widget, an update will be
    /// sent to the [`ToggleButton::on_toggle`] handler.
    ///
    /// By default this toggle button has a style class of [`ToggleButtonClass`] applied
    /// with a default style provided.
    /// ### Examples
    /// ```rust
    /// # use floem::reactive::{SignalGet, SignalUpdate, RwSignal};
    /// # use floem::views::toggle_button;
    /// # use floem::prelude::{palette::css, ToggleHandleBehavior};
    /// // An example using read-write signal
    /// let state = RwSignal::new(true);
    /// let toggle = toggle_button(move || state.get())
    ///     // Set action when button is toggled according to the toggle state provided.
    ///     .on_toggle(move |new_state| state.set(new_state));
    ///
    /// // Use toggle button specific styles to control its look and behavior
    /// let customized_toggle = toggle_button(move || state.get())
    ///     .on_toggle(move |new_state| state.set(new_state))
    ///     .toggle_style(|s| s
    ///         // Set toggle button accent color
    ///         .accent_color(css::REBECCA_PURPLE)
    ///         // Set toggle button circle radius
    ///         .circle_rad(5.)
    ///         // Set toggle button handle color
    ///         .handle_color(css::PURPLE)
    ///         // Set toggle button handle inset
    ///         .handle_inset(1.)
    ///         // Set toggle button behavior:
    ///         // - `Follow` - to follow the pointer movement
    ///         // - `Snap` - to snap once pointer passed 50% treshold
    ///         .behavior(ToggleHandleBehavior::Snap)
    ///     );
    ///```
    /// ### Reactivity
    /// This function is reactive and will reactively respond to changes.
    pub fn new(state: impl Fn() -> bool + 'static) -> Self {
        let id = ViewId::new();
        Effect::new(move |_| {
            let state = state();
            id.update_state(state);
        });

        Self {
            id,
            state: false,
            ontoggle: None,
            position: 0.0,
            held: ToggleState::Nothing,
            style: Default::default(),
        }
        .class(ToggleButtonClass)
    }

    /// Create new [ToggleButton] with read-write signal.
    /// ### Examples
    /// ```rust
    /// # use floem::prelude::*;
    /// # use floem::prelude::palette::css;
    /// // Create read-write signal that will hold toggle button state
    /// let state = RwSignal::new(false);
    /// // `.on_toggle()` is not needed as state is provided via signal
    /// // INFO: If you use it, the state will stop updating `state` signal.
    /// let simple = ToggleButton::new_rw(state);
    ///
    /// let complex = ToggleButton::new_rw(state)
    ///     // Set styles for the toggle
    ///     .toggle_style(move |s| s
    ///         // Apply some styles on self optionally (here on `state` update)
    ///         .apply_if(state.get(), |s| s
    ///             .accent_color(css::DARK_GRAY)
    ///             .handle_color(css::WHITE_SMOKE)
    ///         )
    ///         .behavior(ToggleHandleBehavior::Snap)
    ///     );
    /// ```
    /// ### Reactivity
    /// This funtion will update provided signal on toggle or will be updated if signal will change
    /// due to external signal update.
    pub fn new_rw(state: impl SignalGet<bool> + SignalUpdate<bool> + Copy + 'static) -> Self {
        Self::new(move || state.get()).on_toggle(move |ns| state.set(ns))
    }

    /// Add an event handler to be run when the button is toggled.
    ///
    /// This does not run if the state is changed because of an outside signal.
    /// ### Rectivity
    /// This handler is only called if this button is clicked or switched.
    pub fn on_toggle(mut self, ontoggle: impl Fn(bool) + 'static) -> Self {
        self.ontoggle = Some(Box::new(ontoggle));
        self
    }

    /// Set styles related to [ToggleButton]:
    /// - handle color
    /// - accent color
    /// - handle inset
    /// - circle radius
    /// - behavior of the switch (follow or snap)
    pub fn toggle_style(
        self,
        style: impl Fn(ToggleButtonCustomStyle) -> ToggleButtonCustomStyle + 'static,
    ) -> Self {
        self.style(move |s| s.apply_custom(style(Default::default())))
    }
}

/// Represents a custom style for a [ToggleButton].
#[derive(Debug, Default, Clone)]
pub struct ToggleButtonCustomStyle(Style);
impl From<ToggleButtonCustomStyle> for Style {
    fn from(value: ToggleButtonCustomStyle) -> Self {
        value.0
    }
}

impl ToggleButtonCustomStyle {
    /// Create new styles for [ToggleButton].
    pub fn new() -> Self {
        Self(Style::new())
    }

    /// Sets the color of the toggle handle.
    ///
    /// # Arguments
    /// **color** - A `Brush` that sets the handle's color.
    pub fn handle_color(mut self, color: impl Into<Brush>) -> Self {
        self = Self(self.0.set(Foreground, Some(color.into())));
        self
    }

    /// Sets the accent color of the toggle button.
    ///
    /// # Arguments
    /// **color** - A `Brush` that sets the toggle button's accent color.
    /// This is the same as the background color.
    pub fn accent_color(mut self, color: impl Into<Brush>) -> Self {
        self = Self(self.0.background(color));
        self
    }

    /// Sets the inset of the toggle handle.
    ///
    /// # Arguments
    /// **inset** - A `PxPct` value that defines the inset of the handle from
    /// the toggle button's edge.
    pub fn handle_inset(mut self, inset: impl Into<PxPct>) -> Self {
        self = Self(self.0.set(ToggleButtonInset, inset));
        self
    }

    /// Sets the radius of the toggle circle.
    ///
    /// # Arguments
    /// **rad** - A `PxPct` value that defines the radius of the toggle
    /// button's inner circle.
    pub fn circle_rad(mut self, rad: impl Into<PxPct>) -> Self {
        self = Self(self.0.set(ToggleButtonCircleRad, rad));
        self
    }

    /// Sets the switch behavior of the toggle button.
    ///
    /// # Arguments
    /// **switch** - A `ToggleHandleBehavior` that defines how the toggle
    /// handle behaves on interaction.
    ///
    /// On `Follow`, the handle will follow the mouse.
    /// On `Snap`, the handle will snap to the nearest side.
    pub fn behavior(mut self, switch: ToggleHandleBehavior) -> Self {
        self = Self(self.0.set(ToggleButtonBehavior, switch));
        self
    }

    /// Sets the styles of the toggle button if `true`.
    ///
    /// # Arguments
    /// **cond** - if resolves to `true` will apply styles from the closure.
    /// ```rust
    /// # use floem::prelude::{RwSignal, palette::css};
    /// # use crate::floem::prelude::SignalGet;
    /// # use floem::views::ToggleButton;
    /// let state = RwSignal::new(false);
    /// let toggle = ToggleButton::new_rw(state)
    ///     .toggle_style(move |s| s
    ///         .apply_if(state.get(), |s| s
    ///             .accent_color(css::DARK_GRAY)
    ///         )
    ///     );
    /// ```
    pub fn apply_if(self, cond: bool, f: impl FnOnce(Self) -> Self) -> Self {
        if cond { f(self) } else { self }
    }
}
