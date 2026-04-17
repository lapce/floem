#![deny(missing_docs)]
//! A toggle button widget. An example can be found in [widget-gallery/button](https://github.com/lapce/floem/tree/main/examples/widget-gallery)
//! in the floem examples.

use std::{cell::RefCell, rc::Rc, time::Duration};

use floem_reactive::{Effect, SignalGet, SignalUpdate};
use peniko::Brush;
use peniko::kurbo::{Point, Rect, Size};
use ui_events::pointer::PointerEvent;

use crate::context::Phases;
use crate::custom_event;
use crate::event::listener::EventListenerTrait;
use crate::{
    BoxTree, ElementId, Renderer,
    context::{EventCx, PaintCx, UpdateCx},
    easing::Linear,
    event::{
        DragConfig, DragEvent, DragSourceEvent, Event, EventPropagation, InteractionEvent, Phase,
        PointerCaptureEvent, listener::UpdatePhaseLayout,
    },
    prop, prop_extractor,
    style::{FontSize, Foreground, LineHeight, Style, StyleCustomExt},
    style_class,
    unit::Length,
    view::View,
    view::ViewId,
    views::Decorators,
};

prop!(pub ToggleButtonInset: Length {} = Length::Pt(0.));
prop!(pub ToggleButtonCircleRad: Length {} = Length::Pct(95.));

prop_extractor! {
    ToggleStyle {
        foreground: Foreground,
        inset: ToggleButtonInset,
        circle_rad: ToggleButtonCircleRad,
        font_size: FontSize,
        line_height: LineHeight,
    }
}

style_class!(
    /// A class for styling [ToggleButton] view.
    pub ToggleButtonClass
);

#[derive(Clone, Copy, Debug)]
/// Event fired when the toggle state changes
pub struct ToggleChanged(bool);
impl ToggleChanged {
    fn extract_inner(&self) -> &bool {
        &self.0
    }
}

custom_event!(ToggleChanged, bool, ToggleChanged::extract_inner);

struct Handle {
    element_id: ElementId,
    box_tree: Rc<RefCell<BoxTree>>,
    position: f64,
    parent_id: ViewId,
    dragged: bool,
    moved_on_down: bool,
}

impl Handle {
    fn new(parent_id: ViewId) -> Self {
        Self {
            parent_id,
            element_id: parent_id.create_child_element_id(1),
            box_tree: parent_id.box_tree(),
            position: 0.0,
            dragged: false,
            moved_on_down: false,
        }
    }

    fn restrict(&mut self, width: f64, radius: f64, inset: f64) {
        self.position = self
            .position
            .max(radius + inset)
            .min(width - radius - inset);
    }

    fn update_bounds(&self, size: Size, radius: f64) {
        let rect = Rect::new(
            self.position - radius,
            0.,
            self.position + radius,
            size.height,
        );
        let mut bt = self.box_tree.borrow_mut();
        bt.set_local_bounds(self.element_id.0, rect);
    }

    fn snap(&mut self, state: bool, size: Size, radius: f64, inset: f64) {
        self.position = if state { size.width } else { 0. };
        self.restrict(size.width, radius, inset);
        self.update_bounds(size, radius);
    }

    fn event(
        &mut self,
        cx: &mut EventCx,
        state: &mut bool,
        toggle_size: Size,
        radius: f64,
        inset: f64,
    ) {
        match &cx.event {
            Event::Pointer(PointerEvent::Down(e)) => {
                if let Some(pointer_id) = e.pointer.pointer_id {
                    cx.window_state
                        .set_pointer_capture(pointer_id, self.element_id);
                }
            }
            Event::PointerCapture(PointerCaptureEvent::Gained(drag)) => {
                self.dragged = false;
                cx.start_drag(*drag, DragConfig::new(1., Duration::ZERO, Linear), false);
            }
            Event::PointerCapture(PointerCaptureEvent::Lost(_)) => {
                let new_state = self.position >= toggle_size.width / 2.;
                self.position = if new_state { toggle_size.width } else { 0. };
                self.restrict(toggle_size.width, radius, inset);
                self.update_bounds(toggle_size, radius);
                if new_state != *state {
                    *state = new_state;
                }
                cx.window_state.request_paint(self.parent_id);
            }
            Event::Drag(DragEvent::Source(DragSourceEvent::Move(dme))) => {
                self.dragged = true;
                self.position = dme.current_state.logical_point().x;
                self.restrict(toggle_size.width, radius, inset);
                *state = self.position >= toggle_size.width / 2.;
                self.update_bounds(toggle_size, radius);
                cx.window_state.request_paint(self.parent_id);
            }
            Event::Interaction(InteractionEvent::Click) => {
                if !self.dragged {
                    *state = !*state;
                    self.snap(*state, toggle_size, radius, inset);
                }
            }

            _ => {}
        }
    }

    fn paint(&self, cx: &mut PaintCx, color: Option<Brush>, size: Size, radius: f64) {
        let circle_point = Point::new(self.position, size.to_rect().center().y);
        let circle = crate::kurbo::Circle::new(circle_point, radius);
        if let Some(color) = color {
            cx.fill(&circle, &color, 0.);
        }
    }
}

/// A toggle button.
pub struct ToggleButton {
    id: ViewId,
    state: bool,
    handle: Handle,
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
/// // An example using read-write signal
/// let state = RwSignal::new(true);
/// let toggle = toggle_button(move || state.get())
///     .on_toggle(move |new_state| state.set(new_state));
/// ```
/// ### Reactivity
/// This function is reactive and will reactively respond to changes.
#[deprecated]
pub fn toggle_button(state: impl Fn() -> bool + 'static) -> ToggleButton {
    ToggleButton::new(state)
}

impl ToggleButton {
    fn length_resolve_cx(&self) -> crate::style::FontSizeCx {
        let font_size = self.style.font_size();
        let line_height = match self.style.line_height() {
            crate::text::LineHeightValue::Pt(value) => f64::from(value),
            crate::text::LineHeightValue::Normal(value) => font_size * f64::from(value),
        };
        crate::style::FontSizeCx::new(font_size, line_height)
    }

    fn circle_radius(&self, size: Size) -> f64 {
        self.style
            .circle_rad()
            .resolve(size.width.min(size.height) / 2.0, &self.length_resolve_cx())
    }

    fn inset(&self, width: f64) -> f64 {
        self.style
            .inset()
            .resolve(width, &self.length_resolve_cx())
            .min(width / 2.0)
    }

    fn post_layout(&mut self) {
        let size = self.id.get_layout_rect_local().size();
        let radius = self.circle_radius(size);
        let inset = self.inset(size.width);
        self.handle.restrict(size.width, radius, inset);
        self.handle.update_bounds(size, radius);
    }

    fn snap(&mut self) {
        let size = self.id.get_layout_rect_local().size();
        let radius = self.circle_radius(size);
        let inset = self.inset(size.width);
        self.handle.snap(self.state, size, radius, inset);
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
    /// // An example using read-write signal
    /// let state = RwSignal::new(true);
    /// let toggle = toggle_button(move || state.get())
    ///     .on_toggle(move |new_state| state.set(new_state));
    /// ```
    /// ### Reactivity
    /// This function is reactive and will reactively respond to changes.
    pub fn new(state: impl Fn() -> bool + 'static) -> Self {
        let id = ViewId::new();
        id.register_listener(UpdatePhaseLayout::listener_key());

        Effect::new(move |_| {
            let state = state();
            id.update_state(state);
        });

        Self {
            id,
            state: false,
            handle: Handle::new(id),
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
    /// let simple = ToggleButton::new_rw(state);
    /// ```
    /// ### Reactivity
    /// This function will update provided signal on toggle or will be updated if signal changes
    /// due to external signal update.
    pub fn new_rw(state: impl SignalGet<bool> + SignalUpdate<bool> + Copy + 'static) -> Self {
        Self::new(move || state.get())
            .on_event_stop(ToggleChanged::listener(), move |_cx, ns| state.set(*ns))
    }

    /// Add an event handler to be run when the button is toggled.
    ///
    /// This does not run if the state is changed because of an outside signal.
    #[deprecated(note = "use .on_event_stop(ToggleChanged::listener(), |_, _|) directly instead")]
    pub fn on_toggle(self, ontoggle: impl Fn(bool) + 'static) -> Self {
        self.on_event_stop(ToggleChanged::listener(), move |_cx, e| ontoggle(*e))
    }

    /// Set styles related to [ToggleButton]:
    /// - handle color
    /// - accent color
    /// - handle inset
    /// - circle radius
    pub fn toggle_style(
        self,
        style: impl Fn(ToggleButtonCustomStyle) -> ToggleButtonCustomStyle + 'static,
    ) -> Self {
        self.style(move |s| s.apply_custom(style(Default::default())))
    }
}

impl View for ToggleButton {
    fn id(&self) -> ViewId {
        self.id
    }

    fn debug_name(&self) -> std::borrow::Cow<'static, str> {
        "Toggle Button".into()
    }

    fn view_style(&self) -> Option<Style> {
        Some(Style::new().keyboard_navigable())
    }

    fn update(&mut self, _cx: &mut UpdateCx, state: Box<dyn std::any::Any>) {
        if let Ok(state) = state.downcast::<bool>() {
            self.state = *state;
            self.snap();
            self.id.request_paint();
        }
    }

    fn event(&mut self, cx: &mut EventCx) -> EventPropagation {
        if UpdatePhaseLayout::extract(&cx.event).is_some() {
            self.post_layout();
            return EventPropagation::Stop;
        }

        if cx.phase != Phase::Target {
            return EventPropagation::Continue;
        }

        let toggle_size = self.id.get_layout_rect_local().size();
        let radius = self.circle_radius(toggle_size);
        let inset = self.inset(toggle_size.width);

        // Click without active capture — simple toggle (pointer click or keyboard activation)

        if cx.target == self.handle.element_id {
            let old = self.state;
            self.handle
                .event(cx, &mut self.state, toggle_size, radius, inset);
            if self.state != old {
                self.id.route_event_with_caused_by(
                    Event::new_custom(ToggleChanged(self.state)),
                    crate::event::RouteKind::Directed {
                        target: self.id.get_element_id(),
                        phases: Phases::TARGET,
                    },
                    Some(cx.event.clone()),
                );
            }
        } else {
            // Click on the track — move handle to click position then capture
            if let Event::Pointer(PointerEvent::Down(pbe)) = &cx.event {
                let old_state = self.state;
                self.handle.position = pbe.state.logical_point().x;
                self.handle.restrict(toggle_size.width, radius, inset);
                self.handle.update_bounds(toggle_size, radius);
                let new_state = self.handle.position >= toggle_size.width / 2.;
                self.handle.moved_on_down = new_state != old_state;
                if let Some(pointer_id) = pbe.pointer.pointer_id {
                    cx.window_state
                        .set_pointer_capture(pointer_id, self.handle.element_id);
                }
                self.id.request_paint();
            }
            if let Event::Interaction(InteractionEvent::Click) = &cx.event {
                if cx.triggered_by.is_some_and(|e| e.is_keyboard_trigger())
                    || (!self.handle.dragged && !self.handle.moved_on_down)
                {
                    self.state = !self.state;
                    self.id.route_event(
                        Event::new_custom(ToggleChanged(self.state)),
                        crate::event::RouteKind::Directed {
                            target: self.id.get_element_id(),
                            phases: Phases::TARGET,
                        },
                    );
                    self.snap();
                    self.id.request_paint();
                    return EventPropagation::Stop;
                }
                return EventPropagation::Continue;
            }
        }

        EventPropagation::Continue
    }

    fn style_pass(&mut self, cx: &mut crate::context::StyleCx<'_>) {
        if self.style.read(cx) {
            cx.window_state.request_paint(self.id.into());
        }
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        self.id.request_layout();
        if cx.target_id == self.handle.element_id {
            let size = self.id.get_layout_rect_local().size();
            let radius = self.circle_radius(size);
            self.handle.paint(cx, self.style.foreground(), size, radius);
        }
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
    pub fn handle_color(mut self, color: impl Into<Brush>) -> Self {
        self = Self(self.0.set(Foreground, Some(color.into())));
        self
    }

    /// Sets the accent color of the toggle button (same as background color).
    pub fn accent_color(mut self, color: impl Into<Brush>) -> Self {
        self = Self(self.0.background(color));
        self
    }

    /// Sets the inset of the toggle handle from the edge.
    pub fn handle_inset(mut self, inset: impl Into<Length>) -> Self {
        self = Self(self.0.set(ToggleButtonInset, inset));
        self
    }

    /// Sets the radius of the toggle circle.
    pub fn circle_rad(mut self, rad: impl Into<Length>) -> Self {
        self = Self(self.0.set(ToggleButtonCircleRad, rad));
        self
    }

    /// Sets the styles of the toggle button if `cond` is `true`.
    pub fn apply_if(self, cond: bool, f: impl FnOnce(Self) -> Self) -> Self {
        if cond { f(self) } else { self }
    }
}
