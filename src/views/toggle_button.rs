use floem_reactive::{create_effect, RwSignal};
use floem_renderer::Renderer;
use kurbo::{Point, Size};
use winit::keyboard::{Key, NamedKey};

use crate::{id, prop, style_class, view::View};

use super::Decorators;

prop!(pub ToggleBg: Option<peniko::Color> {} = None);
prop!(pub ToggleFg: Option<peniko::Color> {} = None);
style_class!(pub ToggleClass);

#[derive(PartialEq, Eq)]
enum ToggleState {
    Nothing,
    Held,
    Drag,
}

pub struct ToggleButton {
    id: id::Id,
    state: RwSignal<bool>,
    position: f64,
    held: ToggleState,
    width: f64,
    radius: f64,
}
impl View for ToggleButton {
    fn id(&self) -> crate::id::Id {
        self.id
    }

    fn update(&mut self, cx: &mut crate::context::UpdateCx, state: Box<dyn std::any::Any>) {
        if let Ok(_state) = state.downcast::<bool>() {
            self.update_restrict_position(true);
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
                cx.app_state_mut().request_paint(self.id());

                // if held and pointer up. toggle the position
                if self.held == ToggleState::Held {
                    if self.position > self.width / 2. {
                        self.position = 0.;
                    } else {
                        self.position = self.width;
                    }
                }
                // set the state based on the position of the slider
                if self.held == ToggleState::Held || self.held == ToggleState::Drag {
                    let state = self.state.get();
                    if state && self.position < self.width / 2. {
                        self.state.set(false);
                    } else if !state && self.position > self.width / 2. {
                        self.state.set(true);
                    }
                }
                self.held = ToggleState::Nothing;
            }
            crate::event::Event::PointerMove(event) => {
                if self.held == ToggleState::Held || self.held == ToggleState::Drag {
                    self.held = ToggleState::Drag;
                    cx.app_state_mut().request_paint(self.id());
                    self.position = event.pos.x;
                }
            }
            crate::event::Event::FocusLost => {
                self.held = ToggleState::Nothing;
            }
            crate::event::Event::KeyDown(event) => {
                if event.key.logical_key == Key::Named(NamedKey::Enter) {
                    self.state.update(|val| *val = !*val);
                }
            }
            _ => {}
        };
        false
    }

    fn paint(&mut self, cx: &mut crate::context::PaintCx) {
        let layout = cx.get_layout(self.id).unwrap();
        let size = Size::new(layout.size.width as f64, layout.size.height as f64);

        self.width = size.width;
        self.update_restrict_position(false);
        let rect = size.to_rounded_rect(size.min_side());
        let computed_style = cx.get_computed_style(self.id());
        let background = computed_style
            .get(ToggleBg)
            .unwrap_or(crate::peniko::Color::BLACK);
        let circle_radius = size.min_side() / 2. * 0.75;
        self.radius = circle_radius;
        let circle_point = Point::new(self.position, rect.center().y);
        let circle = crate::kurbo::Circle::new(circle_point, circle_radius);
        let fg = computed_style
            .get(ToggleFg)
            .unwrap_or(crate::peniko::Color::WHITE);
        cx.fill(&rect, background, 0.);
        cx.fill(&circle, fg, 0.);
    }
}
impl ToggleButton {
    fn update_restrict_position(&mut self, end_pos: bool) {
        let inset = self.width * 0.05;

        if self.held == ToggleState::Nothing || end_pos {
            self.position = if self.state.get_untracked() {
                self.width
            } else {
                0.
            };
        }

        self.position = self
            .position
            .max(self.radius + inset)
            .min(self.width - self.radius - inset);
    }
}
pub fn toggle_button(state: RwSignal<bool>) -> ToggleButton {
    let id = crate::id::Id::next();
    create_effect(move |_| {
        let state = state.get();
        id.update_state(state, false);
    });

    ToggleButton {
        id,
        state,
        position: 0.0,
        held: ToggleState::Nothing,
        width: 0.,
        radius: 0.,
    }
    .class(ToggleClass)
    .keyboard_navigatable()
}
