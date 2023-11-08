use floem_reactive::create_effect;
use floem_renderer::Renderer;
use kurbo::{Point, Size};
use winit::keyboard::{Key, NamedKey};

use crate::{
    id, prop, prop_extracter,
    style::{self, Background, BorderRadius, Foreground},
    style_class,
    unit::PxPct,
    view::View,
    views::Decorators,
};

#[derive(Debug, Clone, PartialEq)]
pub enum ToggleButtonBehavior {
    Follow,
    Switch,
}

impl style::StylePropValue for ToggleButtonBehavior {}

prop!(pub ToggleButtonInset: PxPct {} = PxPct::Px(0.));
prop!(pub ToggleButtonCircleRad: PxPct {} = PxPct::Pct(95.));
prop!(pub ToggleButtonSwitch: ToggleButtonBehavior {} = ToggleButtonBehavior::Switch);

prop_extracter! {
    ToggleStyle {
        foreground: Foreground,
        background: Background,
        border_radius: BorderRadius,
        inset: ToggleButtonInset,
        circle_rad: ToggleButtonCircleRad,
        switch_behavior: ToggleButtonSwitch
    }
}
style_class!(pub ToggleButtonClass);

#[derive(PartialEq, Eq)]
enum ToggleState {
    Nothing,
    Held,
    Drag,
}

pub struct ToggleButton {
    id: id::Id,
    state: bool,
    ontoggle: Option<Box<dyn Fn(bool)>>,
    position: f32,
    held: ToggleState,
    width: f32,
    radius: f32,
    style: ToggleStyle,
}
pub fn toggle_button(state: impl Fn() -> bool + 'static) -> ToggleButton {
    let id = crate::id::Id::next();
    create_effect(move |_| {
        let state = state();
        id.update_state(state, false);
    });

    ToggleButton {
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

impl View for ToggleButton {
    fn id(&self) -> crate::id::Id {
        self.id
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
    ) -> bool {
        match event {
            crate::event::Event::PointerDown(_event) => {
                cx.update_active(self.id);
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
                        ToggleButtonBehavior::Follow => {
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
                        ToggleButtonBehavior::Switch => {
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
        false
    }

    fn compute_layout(&mut self, cx: &mut crate::context::LayoutCx) -> Option<kurbo::Rect> {
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
            cx.app_state_mut().request_paint(self.id);
        }
    }

    fn paint(&mut self, cx: &mut crate::context::PaintCx) {
        let layout = cx.get_layout(self.id).unwrap();
        let size = Size::new(layout.size.width as f64, layout.size.height as f64);
        let border_radius = match self.style.border_radius() {
            PxPct::Px(px) => px,
            PxPct::Pct(pct) => size.min_side() * (pct / 100.),
        };
        let rounded_rect = size.to_rounded_rect(border_radius);
        let circle_point = Point::new(self.position as f64, rounded_rect.center().y);
        let circle = crate::kurbo::Circle::new(circle_point, self.radius as f64);
        // here fill default themes
        if let Some(color) = self.style.background() {
            cx.fill(&rounded_rect, color, 0.);
        }
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
    pub fn on_toggle(mut self, ontoggle: impl Fn(bool) + 'static) -> Self {
        self.ontoggle = Some(Box::new(ontoggle));
        self
    }
}
