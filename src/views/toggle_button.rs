//! A toggle button widget. An example can be found in widget-gallery/button in the floem examples.

use floem_reactive::{create_effect, SignalGet, SignalUpdate};
use floem_winit::keyboard::{Key, NamedKey};
use peniko::kurbo::{Point, Size};
use peniko::Brush;

use crate::{
    event::EventPropagation,
    id::ViewId,
    prop, prop_extractor,
    style::{self, Foreground, Style},
    style_class,
    unit::PxPct,
    view::View,
    views::Decorators,
    Renderer,
};

/// Controls the switching behavior of the switch. The corresponding style prop is [ToggleButtonBehavior]
#[derive(Debug, Clone, PartialEq)]
pub enum ToggleHandleBehavior {
    /// The switch foreground item will follow the position of the cursor. The toggle event happens when the cursor passes the 50% threshold.
    Follow,
    /// The switch foreground item will "snap" from being toggled off/on when the cursor passes the 50% threshold.
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
style_class!(pub ToggleButtonClass);

#[derive(PartialEq, Eq)]
enum ToggleState {
    Nothing,
    Held,
    Drag,
}

/// A toggle button
pub struct ToggleButton {
    id: ViewId,
    state: bool,
    ontoggle: Option<Box<dyn Fn(bool)>>,
    position: f32,
    held: ToggleState,
    width: f32,
    radius: f32,
    style: ToggleStyle,
}

/// A reactive toggle button
///
/// When the button is toggled by clicking or dragging the widget an update will be
/// sent to the [ToggleButton::on_toggle] handler.
/// See also [ToggleButtonClass], [ToggleHandleBehavior] and the other toggle button styles that can be applied.
///
/// By default this toggle button has a style class of [ToggleButtonClass] applied with a default style provided.
///
/// Styles:
/// background color: [style::Background]
///
/// Toggle color: [style::Foreground]
///
/// inner switch inset: [ToggleButtonInset]
///
/// inner switch (circle) size/radius: [ToggleButtonCircleRad]
///
/// toggle button switch behavior: [ToggleButtonBehavior] / [ToggleHandleBehavior]
///
/// An example using [`RwSignal`](floem_reactive::RwSignal):
/// ```rust
/// use floem::reactive::{SignalGet, SignalUpdate};
///
/// let state = floem::reactive::create_rw_signal(true);
/// floem::views::toggle_button(move || state.get())
///         .on_toggle(move |new_state| state.set(new_state));
///```
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
            if self.held == ToggleState::Nothing {
                self.update_restrict_position(true);
            }
            self.state = *state;
            self.id.request_layout();
        }
    }

    fn event_before_children(
        &mut self,
        cx: &mut crate::context::EventCx,
        event: &crate::event::Event,
    ) -> EventPropagation {
        match event {
            crate::event::Event::PointerDown(_event) => {
                cx.update_active(self.id);
                self.held = ToggleState::Held;
            }
            crate::event::Event::PointerUp(_event) => {
                self.id.request_layout();

                // if held and pointer up. toggle the position (toggle state drag already changed the position)
                if self.held == ToggleState::Held {
                    if self.position > self.width / 2. {
                        self.position = 0.;
                    } else {
                        self.position = self.width;
                    }
                }
                // set the state based on the position of the slider
                if self.held == ToggleState::Held {
                    if self.state && self.position < self.width / 2. {
                        self.state = false;
                        if let Some(ontoggle) = &self.ontoggle {
                            ontoggle(false);
                        }
                    } else if !self.state && self.position > self.width / 2. {
                        self.state = true;
                        if let Some(ontoggle) = &self.ontoggle {
                            ontoggle(true);
                        }
                    }
                }
                self.held = ToggleState::Nothing;
            }
            crate::event::Event::PointerMove(event) => {
                if self.held == ToggleState::Held || self.held == ToggleState::Drag {
                    self.held = ToggleState::Drag;
                    match self.style.switch_behavior() {
                        ToggleHandleBehavior::Follow => {
                            self.position = event.pos.x as f32;
                            if self.position > self.width / 2. && !self.state {
                                self.state = true;
                                if let Some(ontoggle) = &self.ontoggle {
                                    ontoggle(true);
                                }
                            } else if self.position < self.width / 2. && self.state {
                                self.state = false;
                                if let Some(ontoggle) = &self.ontoggle {
                                    ontoggle(false);
                                }
                            }
                            self.id.request_layout();
                        }
                        ToggleHandleBehavior::Snap => {
                            if event.pos.x as f32 > self.width / 2. && !self.state {
                                self.position = self.width;
                                self.id.request_layout();
                                self.state = true;
                                if let Some(ontoggle) = &self.ontoggle {
                                    ontoggle(true);
                                }
                            } else if (event.pos.x as f32) < self.width / 2. && self.state {
                                self.position = 0.;
                                // self.held = ToggleState::Nothing;
                                self.id.request_layout();
                                self.state = false;
                                if let Some(ontoggle) = &self.ontoggle {
                                    ontoggle(false);
                                }
                            }
                        }
                    }
                }
            }
            crate::event::Event::FocusLost => {
                self.held = ToggleState::Nothing;
            }
            crate::event::Event::KeyDown(event) => {
                if event.key.logical_key == Key::Named(NamedKey::Enter) {
                    if let Some(ontoggle) = &self.ontoggle {
                        ontoggle(!self.state);
                    }
                }
            }
            _ => {}
        };
        EventPropagation::Continue
    }

    fn compute_layout(
        &mut self,
        _cx: &mut crate::context::ComputeLayoutCx,
    ) -> Option<peniko::kurbo::Rect> {
        let layout = self.id.get_layout().unwrap_or_default();
        let size = layout.size;
        self.width = size.width;
        let circle_radius = match self.style.circle_rad() {
            PxPct::Px(px) => px as f32,
            PxPct::Pct(pct) => size.width.min(size.height) / 2. * (pct as f32 / 100.),
        };
        self.radius = circle_radius;
        self.update_restrict_position(false);

        None
    }

    fn style_pass(&mut self, cx: &mut crate::context::StyleCx<'_>) {
        if self.style.read(cx) {
            cx.app_state_mut().request_paint(self.id);
        }
    }

    fn paint(&mut self, cx: &mut crate::context::PaintCx) {
        let layout = self.id.get_layout().unwrap_or_default();
        let size = Size::new(layout.size.width as f64, layout.size.height as f64);
        let circle_point = Point::new(self.position as f64, size.to_rect().center().y);
        let circle = crate::kurbo::Circle::new(circle_point, self.radius as f64);
        if let Some(color) = self.style.foreground() {
            cx.fill(&circle, &color, 0.);
        }
    }
}
impl ToggleButton {
    fn update_restrict_position(&mut self, end_pos: bool) {
        let inset = match self.style.inset() {
            PxPct::Px(px) => px as f32,
            PxPct::Pct(pct) => (self.width * (pct as f32 / 100.)).min(self.width / 2.),
        };

        if self.held == ToggleState::Nothing || end_pos {
            self.position = if self.state { self.width } else { 0. };
        }

        self.position = self
            .position
            .max(self.radius + inset)
            .min(self.width - self.radius - inset);
    }

    pub fn new(state: impl Fn() -> bool + 'static) -> Self {
        let id = ViewId::new();
        create_effect(move |_| {
            let state = state();
            id.update_state(state);
        });

        Self {
            id,
            state: false,
            ontoggle: None,
            position: 0.0,
            held: ToggleState::Nothing,
            width: 0.,
            radius: 0.,
            style: Default::default(),
        }
        .class(ToggleButtonClass)
        .keyboard_navigatable()
    }

    pub fn new_rw(state: impl SignalGet<bool> + SignalUpdate<bool> + Copy + 'static) -> Self {
        Self::new(move || state.get()).on_toggle(move |ns| state.set(ns))
    }

    /// Add an event handler to be run when the button is toggled.
    ///
    ///This does not run if the state is changed because of an outside signal.
    /// This handler is only called if this button is clicked or switched
    pub fn on_toggle(mut self, ontoggle: impl Fn(bool) + 'static) -> Self {
        self.ontoggle = Some(Box::new(ontoggle));
        self
    }

    pub fn toggle_style(
        self,
        style: impl Fn(ToggleButtonCustomStyle) -> ToggleButtonCustomStyle + 'static,
    ) -> Self {
        self.style(move |s| s.apply_custom(style(Default::default())))
    }
}

/// Represents a custom style for a `ToggleButton`.
#[derive(Debug, Default, Clone)]
pub struct ToggleButtonCustomStyle(Style);
impl From<ToggleButtonCustomStyle> for Style {
    fn from(value: ToggleButtonCustomStyle) -> Self {
        value.0
    }
}

impl ToggleButtonCustomStyle {
    pub fn new() -> Self {
        Self(Style::new())
    }

    /// Sets the color of the toggle handle.
    ///
    /// # Arguments
    /// * `color` - An `Option<Color>` that sets the handle's color. `None` will remove the color.
    pub fn handle_color(mut self, color: impl Into<Brush>) -> Self {
        self = Self(self.0.set(Foreground, Some(color.into())));
        self
    }

    /// Sets the accent color of the toggle button.
    ///
    /// # Arguments
    /// * `color` - A `StyleValue<Color>` that sets the toggle button's accent color. This is the same as the background color.
    pub fn accent_color(mut self, color: impl Into<Brush>) -> Self {
        self = Self(self.0.background(color));
        self
    }

    /// Sets the inset of the toggle handle.
    ///
    /// # Arguments
    /// * `inset` - A `PxPct` value that defines the inset of the handle from the toggle button's edge.
    pub fn handle_inset(mut self, inset: impl Into<PxPct>) -> Self {
        self = Self(self.0.set(ToggleButtonInset, inset));
        self
    }

    /// Sets the radius of the toggle circle.
    ///
    /// # Arguments
    /// * `rad` - A `PxPct` value that defines the radius of the toggle button's inner circle.
    pub fn circle_rad(mut self, rad: impl Into<PxPct>) -> Self {
        self = Self(self.0.set(ToggleButtonCircleRad, rad));
        self
    }

    /// Sets the switch behavior of the toggle button.
    ///
    /// # Arguments
    /// * `switch` - A `ToggleHandleBehavior` that defines how the toggle handle behaves on interaction.
    ///
    /// On `Follow`, the handle will follow the mouse.
    /// On `Snap`, the handle will snap to the nearest side.
    pub fn behavior(mut self, switch: ToggleHandleBehavior) -> Self {
        self = Self(self.0.set(ToggleButtonBehavior, switch));
        self
    }
}
