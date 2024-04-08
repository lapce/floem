//! A toggle button widget. An example can be found in widget-gallery/button in the floem examples.

use floem_peniko::Color;
use floem_reactive::{create_effect, create_updater};
use floem_renderer::Renderer;
use floem_winit::keyboard::{Key, NamedKey};
use kurbo::{Point, Size};

use crate::{
    prop, prop_extractor,
    style::{self, Foreground, Style, StyleValue},
    style_class,
    unit::PxPct,
    view::{View, ViewData, Widget},
    views::Decorators,
    EventPropagation,
};

/// Controls the switching behavior of the switch. The cooresponding style prop is [ToggleButtonBehavior]
#[derive(Debug, Clone, PartialEq)]
pub enum ToggleHandleBehavior {
    /// The switch foreground item will follow the position of the cursor. The toggle event happens when the cursor passes teh 50% threshhold.
    Follow,
    /// The switch foreground item will "snap" from being toggled off/on when the cursor passes the 50% threshhold.
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
    data: ViewData,
    state: bool,
    ontoggle: Option<Box<dyn Fn(bool)>>,
    position: f32,
    held: ToggleState,
    width: f32,
    radius: f32,
    style: ToggleStyle,
}

/// A reactive toggle button. When the button is toggled by clicking or dragging the widget an update will be
/// sent to the [`ToggleButton::on_toggle`](crate::widgets::toggle_button::ToggleButton::on_toggle) handler.
/// See also [ToggleButtonClass], [ToggleHandleBehavior] and the other toggle button styles that can be applied.
///
/// By default this toggle button has a style class of [ToggleButtonClass] applied with a default style provided.
///
/// Styles:
/// background color: [style::Background]
/// foreground color: [style::Foreground]
/// inner switch inset: [ToggleButtonInset]
/// inner switch (circle) size/radius: [ToggleButtonCircleRad]
/// toggle button switch behavior: [ToggleButtonBehavior] / [ToggleHandleBehavior]
///
/// An example using [`RwSignal`](floem_reactive::RwSignal):
/// ```rust
/// let state = floem::reactive::create_rw_signal(true);
/// floem::widgets::toggle_button(move || state.get())
///         .on_toggle(move |new_state| state.set(new_state));
///```
pub fn toggle_button(state: impl Fn() -> bool + 'static) -> ToggleButton {
    let id = crate::id::Id::next();
    create_effect(move |_| {
        let state = state();
        id.update_state(state);
    });

    ToggleButton {
        data: ViewData::new(id),
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

impl View for ToggleButton {
    fn view_data(&self) -> &ViewData {
        &self.data
    }

    fn view_data_mut(&mut self) -> &mut ViewData {
        &mut self.data
    }

    fn build(self) -> Box<dyn Widget> {
        Box::new(self)
    }
}

impl Widget for ToggleButton {
    fn view_data(&self) -> &ViewData {
        &self.data
    }

    fn view_data_mut(&mut self) -> &mut ViewData {
        &mut self.data
    }

    fn debug_name(&self) -> std::borrow::Cow<'static, str> {
        "Toggle Button".into()
    }

    fn update(&mut self, cx: &mut crate::context::UpdateCx, state: Box<dyn std::any::Any>) {
        if let Ok(state) = state.downcast::<bool>() {
            if self.held == ToggleState::Nothing {
                self.update_restrict_position(true);
            }
            self.state = *state;
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
            crate::event::Event::PointerDown(_event) => {
                cx.update_active(self.id());
                self.held = ToggleState::Held;
            }
            crate::event::Event::PointerUp(_event) => {
                cx.app_state_mut().request_layout(self.id());

                // if held and pointer up. toggle the position (toggle state drag alrady changed the position)
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
                            cx.app_state_mut().request_layout(self.id());
                        }
                        ToggleHandleBehavior::Snap => {
                            if event.pos.x as f32 > self.width / 2. && !self.state {
                                self.position = self.width;
                                cx.app_state_mut().request_layout(self.id());
                                self.state = true;
                                if let Some(ontoggle) = &self.ontoggle {
                                    ontoggle(true);
                                }
                            } else if (event.pos.x as f32) < self.width / 2. && self.state {
                                self.position = 0.;
                                // self.held = ToggleState::Nothing;
                                cx.app_state_mut().request_layout(self.id());
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

    fn compute_layout(&mut self, cx: &mut crate::context::ComputeLayoutCx) -> Option<kurbo::Rect> {
        let layout = cx.get_layout(self.id()).unwrap();
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

    fn style(&mut self, cx: &mut crate::context::StyleCx<'_>) {
        if self.style.read(cx) {
            cx.app_state_mut().request_paint(self.id());
        }
    }

    fn paint(&mut self, cx: &mut crate::context::PaintCx) {
        let layout = cx.get_layout(self.id()).unwrap();
        let size = Size::new(layout.size.width as f64, layout.size.height as f64);
        let circle_point = Point::new(self.position as f64, size.to_rect().center().y);
        let circle = crate::kurbo::Circle::new(circle_point, self.radius as f64);
        if let Some(color) = self.style.foreground() {
            cx.fill(&circle, color, 0.);
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

    /// Add an event handler to be run when the button is toggled.
    ///
    ///This does not run if the state is changed because of an outside signal.
    /// This handler is only called if this button is clicked or switched
    pub fn on_toggle(mut self, ontoggle: impl Fn(bool) + 'static) -> Self {
        self.ontoggle = Some(Box::new(ontoggle));
        self
    }

    pub fn toggle_style(
        mut self,
        style: impl Fn(ToggleButtonCustomStyle) -> ToggleButtonCustomStyle + 'static,
    ) -> Self {
        let id = self.id();
        let offset = Widget::view_data_mut(&mut self).style.next_offset();
        let style = create_updater(
            move || style(ToggleButtonCustomStyle(Style::new())),
            move |style| id.update_style(style.0, offset),
        );
        Widget::view_data_mut(&mut self).style.push(style.0);
        self
    }
}

/// Represents a custom style for a `ToggleButton`.
pub struct ToggleButtonCustomStyle(Style);

impl ToggleButtonCustomStyle {
    /// Sets the color of the toggle handle.
    ///
    /// # Arguments
    /// * `color` - An `Option<Color>` that sets the handle's color. `None` will remove the color.
    pub fn handle_color(mut self, color: impl Into<Option<Color>>) -> Self {
        self = Self(self.0.set(Foreground, color));
        self
    }

    /// Sets the accent color of the toggle button.
    ///
    /// # Arguments
    /// * `color` - A `StyleValue<Color>` that sets the toggle button's accent color. This is the same as the background color.
    pub fn accent_color(mut self, color: impl Into<StyleValue<Color>>) -> Self {
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
    /// On `Follow`, the handle will follow the mouse.
    /// On `Snap`, the handle will snap to the nearest side.
    pub fn behavior(mut self, switch: ToggleHandleBehavior) -> Self {
        self = Self(self.0.set(ToggleButtonBehavior, switch));
        self
    }

    /// Apply regular style properties
    pub fn style(mut self, style: impl Fn(Style) -> Style + 'static) -> Self {
        self = Self(self.0.apply(style(Style::new())));
        self
    }
}
