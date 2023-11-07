use floem_reactive::create_effect;
use floem_renderer::Renderer;
use kurbo::{Point, Size};
use peniko::Color;
use winit::keyboard::{Key, NamedKey};

use crate::{id, prop, prop_extracter, style_class, view::View, views::Decorators};

prop!(pub ToggleButtonBg: Option<Color> {} = None);
prop!(pub ToggleButtonFg: Option<Color> {} = None);

prop_extracter! {
    ToggleStyle {
        foreground: ToggleButtonFg,
        background: ToggleButtonBg,
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
    position: f64,
    held: ToggleState,
    width: f64,
    radius: f64,
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
            self.update_restrict_position(true);
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

                // if held and pointer up. toggle the position
                if self.held == ToggleState::Held {
                    if self.position > self.width / 2. {
                        self.position = 0.;
                    } else {
                        self.position = self.width;
                    }
                }
                // set the state based on the position of the slider
                if let Some(ontoggle) = &self.ontoggle {
                    if self.held == ToggleState::Held || self.held == ToggleState::Drag {
                        if self.state && self.position < self.width / 2. {
                            ontoggle(false);
                        } else if !self.state && self.position > self.width / 2. {
                            ontoggle(true);
                        }
                    }
                }
                self.held = ToggleState::Nothing;
            }
            crate::event::Event::PointerMove(event) => {
                if self.held == ToggleState::Held || self.held == ToggleState::Drag {
                    self.held = ToggleState::Drag;
                    cx.app_state_mut().request_layout(self.id());
                    self.position = event.pos.x;
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
        self.width = size.width as f64;
        let circle_radius = size.width.min(size.height) / 2. * 0.75;
        self.radius = circle_radius as f64;
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
        let rounded_rect = size.to_rounded_rect(size.min_side());
        let circle_point = Point::new(self.position, rounded_rect.center().y);
        let circle = crate::kurbo::Circle::new(circle_point, self.radius);
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
        let inset = self.width * 0.05;

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
