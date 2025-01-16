use std::{hash::Hasher, sync::atomic::AtomicU64, time::Instant};

use floem::{
    action::{debounce_action, exec_after},
    easing::Spring,
    event::{Event, EventListener},
    keyboard::{Modifiers, NamedKey},
    kurbo::Stroke,
    menu::menu,
    muda::{self, NativeIcon},
    prelude::*,
    reactive::{create_effect, create_memo, Trigger},
    style::{BoxShadowProp, CursorStyle, MinHeight, Transition},
    taffy::AlignItems,
    views::Checkbox,
    AnyView,
};

use crate::{todo_state::TODOS_STATE, AppCommand, OS_MOD};

/// this state macro is unnecessary but convenient. It just produces the original struct and a new struct ({StructName}State) with all of the same fields but wrapped in Signal types.
#[derive(Clone)]
pub struct Todo {
    pub db_id: Option<i64>,
    pub unique_id: u64,
    pub done: bool,
    pub description: String,
}
static UNIQUE_COUNTER: AtomicU64 = AtomicU64::new(0);
impl Todo {
    pub fn new_from_db(db_id: i64, done: bool, description: impl Into<String>) -> Self {
        Self {
            db_id: Some(db_id),
            unique_id: UNIQUE_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            done,
            description: description.into(),
        }
    }
    pub fn new(done: bool, description: impl Into<String>) -> Self {
        Self {
            db_id: None,
            unique_id: UNIQUE_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            done,
            description: description.into(),
        }
    }
}
#[derive(Clone, Copy, Eq, Debug)]
pub struct TodoState {
    pub db_id: RwSignal<Option<i64>>,
    pub unique_id: u64,
    pub done: RwSignal<bool>,
    pub description: RwSignal<String>,
}
impl From<Todo> for TodoState {
    fn from(value: Todo) -> Self {
        Self {
            db_id: RwSignal::new(value.db_id),
            unique_id: value.unique_id,
            done: RwSignal::new(value.done),
            description: RwSignal::new(value.description),
        }
    }
}
impl IntoView for TodoState {
    type V = AnyView;

    fn into_view(self) -> Self::V {
        // when the done status changes, commit the change to the db
        debounce_action(self.done, 300.millis(), move || {
            AppCommand::UpdateDone(self).execute()
        });

        // when the description changes, debounce a commit to the db
        debounce_action(self.description, 300.millis(), move || {
            AppCommand::UpdateDescription(self).execute()
        });

        let (active, selected) = TODOS_STATE.with(|s| (s.active, s.selected));
        let is_active = create_memo(move |_| active.with(|a| (a.active == Some(self))));
        let is_selected = create_memo(move |_| selected.with(|s| s.contains(&self)));

        let todo_action_menu = move || {
            // would be better to have actions that can operate on multiple selections.
            AppCommand::Escape.execute();
            AppCommand::InsertSelected(self).execute();

            let done = self.done.get();
            let action_name = if done {
                "Mark as Incomplete"
            } else {
                "Mark as Complete"
            };

            menu()
                .item(action_name, |i| {
                    i.native_icon(if done {
                        muda::NativeIcon::Remove
                    } else {
                        NativeIcon::Add
                    })
                    .action(move || {
                        if done {
                            self.done.set(false);
                        } else {
                            self.done.set(true);
                        }
                    })
                })
                .separator()
                .item("Delete", |i| {
                    i.native_icon(NativeIcon::TrashEmpty).action(move || {
                        AppCommand::Delete(&[self]).execute();
                    })
                })
        };

        let input_focused = Trigger::new();
        let done_check = Checkbox::new_rw(self.done)
            .style(|s| {
                s.flex_shrink(0.)
                    .max_height_pct(70.)
                    .aspect_ratio(1.)
                    .border(
                        Stroke::new(1.)
                            .with_dashes(0.2, [1., 2.])
                            .with_caps(floem::kurbo::Cap::Round),
                    )
                    .class(SvgClass, |s| s.size_pct(50., 50.))
            })
            .on_key_down(
                floem::keyboard::Key::Named(NamedKey::Enter),
                |_| true,
                |_| {},
            )
            .on_event_stop(EventListener::PointerDown, move |_| {});

        let input = text_input(self.description)
            .placeholder("New To-Do")
            .into_view();
        let input_id = input.id();
        let input = input
            .disable_default_event(move || (EventListener::PointerDown, !is_active))
            .style(move |s| {
                s.width_full()
                    .apply_if(!is_active.get(), |s| s.cursor(CursorStyle::Default))
                    .background(palette::css::TRANSPARENT)
                    .transition_background(Transition::ease_in_out(600.millis()))
                    .border(0)
                    .hover(|s| s.background(palette::css::TRANSPARENT))
                    .focus(|s| {
                        s.hover(|s| s.background(palette::css::TRANSPARENT))
                            .border(0.)
                            .border_color(palette::css::TRANSPARENT)
                    })
                    .disabled(|s| {
                        s.background(palette::css::TRANSPARENT)
                            .color(palette::css::BLACK)
                    })
                    .class(PlaceholderTextClass, |s| s.color(palette::css::GRAY))
            })
            .on_key_down(
                floem::keyboard::Key::Named(NamedKey::Enter),
                |m| m.is_empty(),
                move |_| {
                    AppCommand::Escape.execute();
                },
            )
            .on_event_stop(EventListener::PointerDown, move |_| {
                AppCommand::SetSelected(self).execute();
            })
            .on_event_stop(EventListener::DoubleClick, move |_| {
                AppCommand::SetActive(self).execute();
                input_id.request_focus();
            })
            .on_event_stop(EventListener::FocusGained, move |_| {
                AppCommand::SetActive(self).execute();
                input_focused.notify();
            });

        // if this todo is being created after the app has already been initialized, focus the input
        if Instant::now().duration_since(TODOS_STATE.with(|s| s.time_stated)) > 50.millis() {
            input_id.request_focus();
        }
        create_effect(move |_| {
            if is_active.get() {
                input_id.request_focus();
            }
        });

        let main_controls = (done_check, input)
            .h_stack()
            .debug_name("Todo Checkbox and text input (main controls)")
            .style(|s| s.gap(10).width_full().items_center())
            .container()
            .on_double_click_stop(move |_| {
                AppCommand::SetActive(self).execute();
            })
            .on_click_stop(move |e| {
                let Event::PointerUp(e) = e else {
                    return;
                };
                if e.modifiers == OS_MOD {
                    AppCommand::ToggleSelected(self).execute();
                } else if e.modifiers.contains(Modifiers::SHIFT) {
                    AppCommand::SelectRange(self).execute();
                } else {
                    AppCommand::SetSelected(self).execute();
                }
            })
            .style(|s| s.width_full().align_items(Some(AlignItems::FlexStart)));

        let container = main_controls.container();
        let final_view_id = container.id();
        create_effect(move |_| {
            input_focused.track();
            // this is a super ugly hack...
            // We should really figure out a way to make sure than an item that is focused
            // can be scrolled to and then kept in view if it has an animation/transition
            exec_after(25.millis(), move |_| final_view_id.scroll_to(None));
            exec_after(50.millis(), move |_| final_view_id.scroll_to(None));
            exec_after(75.millis(), move |_| final_view_id.scroll_to(None));
            exec_after(100.millis(), move |_| final_view_id.scroll_to(None));
            exec_after(125.millis(), move |_| final_view_id.scroll_to(None));
            exec_after(150.millis(), move |_| final_view_id.scroll_to(None));
            exec_after(170.millis(), move |_| final_view_id.scroll_to(None));
            exec_after(200.millis(), move |_| final_view_id.scroll_to(None));
        });

        container
            .style(move |s| {
                s.width_full()
                    .min_height(0.)
                    .padding(5)
                    .border_radius(5.)
                    .transition(MinHeight, Transition::new(600.millis(), Spring::snappy()))
                    .box_shadow_blur(0.)
                    .box_shadow_color(palette::css::BLACK.with_alpha(0.0))
                    .box_shadow_h_offset(0.)
                    .box_shadow_v_offset(0.)
                    .background(palette::css::TRANSPARENT)
                    .apply_if(is_selected.get(), |s| {
                        s.background(palette::css::LIGHT_BLUE.with_alpha(0.7))
                    })
                    .apply_if(is_active.get(), |s| {
                        s.min_height(100)
                            .background(palette::css::WHITE_SMOKE)
                            .box_shadow_blur(2.)
                            .box_shadow_color(palette::css::BLACK.with_alpha(0.7))
                            .box_shadow_h_offset(1.)
                            .box_shadow_v_offset(2.)
                            .transition(
                                BoxShadowProp,
                                Transition::new(600.millis(), Spring::snappy()),
                            )
                    })
            })
            .context_menu(todo_action_menu)
            .into_any()
    }
}
impl IntoView for Todo {
    type V = <TodoState as IntoView>::V;

    fn into_view(self) -> Self::V {
        let todo_state: TodoState = self.into();
        todo_state.into_view()
    }
}

impl PartialEq for TodoState {
    fn eq(&self, other: &Self) -> bool {
        self.unique_id == other.unique_id
    }
}

impl std::hash::Hash for TodoState {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.unique_id.hash(state);
    }
}
