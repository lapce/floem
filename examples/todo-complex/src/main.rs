use std::time::Instant;

use floem::{
    action::{exec_after, inspect},
    keyboard::{Key, NamedKey},
    prelude::*,
    ui_events::keyboard::Modifiers,
};
use todo::TodoState;
use todo_state::TODOS_STATE;
mod app_config;
mod todo;
mod todo_state;

pub const OS_MOD: Modifiers = if cfg!(target_os = "macos") {
    Modifiers::META
} else {
    Modifiers::CONTROL
};

fn todos() -> impl IntoView {
    let todos = TODOS_STATE.with(|s| s.todos);
    dyn_stack(move || todos.get(), |todo| *todo, |todo| todo)
        .style(|s| s.flex_col().min_size(0, 0))
        .debug_name("Todos Stack")
}

fn app_view() -> impl IntoView {
    let todos_scroll = todos()
        .style(|s| s.max_width_full().width_full())
        .scroll()
        .style(|s| s.padding(10).padding_right(14))
        .scroll_style(|s| s.shrink_to_fit().handle_thickness(8));

    let new_button = button("New To-Do")
        .action(|| AppCommand::NewTodo.execute())
        .style(|s| s.margin_horiz(10));

    (todos_scroll, new_button)
        .v_stack()
        .debug_name("Todos scroll list and new button")
        .style(|s| {
            s.gap(10)
                .max_width(75.pct())
                .width(75.pct())
                .max_height_pct(75.)
        })
        .container()
        .debug_name("App container view (for center the items list and new button)")
        .style(|s| {
            s.items_center()
                .justify_center()
                .size_full()
                .max_width_full()
                .font_size(15.)
        })
        .on_click_stop(move |_| {
            AppCommand::Escape.execute();
        })
        .on_key_down(
            Key::Named(NamedKey::F11),
            |m| m.is_empty(),
            |_| {
                inspect();
            },
        )
        .on_key_down(
            Key::Named(NamedKey::Escape),
            |m| m.is_empty(),
            move |_| {
                AppCommand::Escape.execute();
            },
        )
        .on_key_down(
            Key::Character("n".into()),
            |m| m == OS_MOD,
            move |_| AppCommand::NewTodo.execute(),
        )
        .on_key_down(
            Key::Character("r".into()),
            |m| m == OS_MOD,
            move |_| AppCommand::RefreshDB.execute(),
        )
        .on_key_down(
            Key::Character("a".into()),
            |m| m == OS_MOD,
            move |_| AppCommand::SelectAll.execute(),
        )
        .on_key_down(
            Key::Named(NamedKey::Enter),
            |m| m.is_empty(),
            |_| {
                AppCommand::AppAction.execute();
            },
        )
        .on_key_down(
            Key::Character(" ".into()),
            |m| m.is_empty(),
            |_| {
                AppCommand::AppAction.execute();
            },
        )
        .on_key_down(
            Key::Named(NamedKey::Backspace),
            // empty, shift, or OS_MOD or OS_MOD + shift
            move |m| !m.intersects((OS_MOD | Modifiers::SHIFT).complement()),
            move |_| AppCommand::DeleteSelected.execute(),
        )
        .on_key_down(
            Key::Named(NamedKey::ArrowDown),
            |m| m == Modifiers::empty(),
            |_| {
                AppCommand::SelectDown.execute();
            },
        )
        .on_key_down(
            Key::Named(NamedKey::ArrowUp),
            |m| m == Modifiers::empty(),
            |_| {
                AppCommand::SelectUp.execute();
            },
        )
}

fn main() {
    app_config::launch_with_track(app_view);
}

#[must_use]
enum AppCommand<'a> {
    Delete(&'a [TodoState]),
    DeleteSelected,
    SelectAll,
    SetActive(TodoState),
    #[allow(unused)]
    ChangeActive(TodoState),
    Escape,
    #[allow(unused)]
    FocusLost,
    RefreshDB,
    NewTodo,
    UpdateDone(TodoState),
    UpdateDescription(TodoState),
    CommitTodo(TodoState),
    InsertSelected(TodoState),
    ToggleSelected(TodoState),
    SelectRange(TodoState),
    /// an action for when the user does something that should
    /// update the UI but what happens is dependent on context
    AppAction,
    SetSelected(TodoState),
    SelectUp,
    SelectDown,
}
impl AppCommand<'_> {
    fn execute(self) {
        let (active, selected, todos) = TODOS_STATE.with(|s| (s.active, s.selected, s.todos));
        TODOS_STATE.with(|s| {
            match self {
                AppCommand::Delete(vec) => {
                    let mut deleted_selected = false;
                    TODOS_STATE.with(|s| {
                        for todo in vec {
                            s.delete(todo.db_id.get_untracked(), todo.unique_id);
                            selected.update(|s| {
                                deleted_selected |= s.remove(todo).is_some();
                            });
                        }
                    });
                    if deleted_selected {
                        AppCommand::SelectUp.execute();
                    }
                }
                AppCommand::DeleteSelected => {
                    let initial_selected_len = selected.with(|sel| sel.len());
                    s.selected.with(|sel| {
                        for todo in sel.iter() {
                            s.delete(todo.db_id.get_untracked(), todo.unique_id);
                        }
                    });
                    s.selected.update(|sel| sel.clear());
                    if initial_selected_len == 1 {
                        AppCommand::SelectUp.execute();
                    }
                }
                AppCommand::SelectAll => {
                    todos.with(|t| {
                        for todo in t.into_iter() {
                            selected.update(|s| {
                                s.insert(*todo);
                            });
                        }
                    });
                }
                AppCommand::SetActive(todo_state) => {
                    active.update(|a| a.set(Some(todo_state)));
                    selected.update(|s| s.clear());
                }
                AppCommand::ChangeActive(todo_state) => {
                    if active.get().active.is_some() {
                        active.update(|a| a.set(Some(todo_state)));
                        selected.update(|s| s.clear());
                    }
                }
                AppCommand::FocusLost => {
                    // some of this would be less complicated to
                    // handle if we add more control over what is allowed to steal focus per view
                    let active_todo = active.get();
                    // handle the case where it was just set, don't clear anything
                    if Instant::now().duration_since(active_todo.time_set) < 50.millis() {
                        return;
                    }
                    // else handle the case where some time goes by without it being set
                    exec_after(50.millis(), move |_| {
                        if Instant::now().duration_since(active.get().time_set) < 50.millis() {
                            return;
                        }
                        let old_active = active.get();
                        active.update(|a| a.set(None));
                        if let Some(active) = old_active.active {
                            selected.update(|s| {
                                s.insert(active);
                            });
                        }
                    });
                }
                AppCommand::Escape => {
                    let active_todo = active.get();
                    if Instant::now().duration_since(active_todo.time_set) < 200.millis() {
                        return;
                    }
                    selected.update(|s| s.clear());
                    if let Some(active) = active_todo.active {
                        selected.update(|s| {
                            s.insert(active);
                        });
                    }
                    active.update(|s| s.set(None));
                    floem::action::clear_app_focus();
                }
                AppCommand::RefreshDB => {
                    s.refresh_db();
                }
                AppCommand::NewTodo => {
                    s.new_todo();
                }
                AppCommand::UpdateDone(todo) => {
                    if let Some(db_id) = todo.db_id.with_untracked(|opt_id| *opt_id) {
                        s.update_done(db_id, todo.done.get_untracked());
                    } else {
                        AppCommand::CommitTodo(todo).execute();
                    }
                }
                AppCommand::UpdateDescription(todo) => {
                    if let Some(db_id) = todo.db_id.with_untracked(|opt_id| *opt_id) {
                        s.update_description(db_id, &todo.description.get_untracked());
                    } else {
                        AppCommand::CommitTodo(todo).execute();
                    }
                }
                AppCommand::CommitTodo(todo) => {
                    let new_db_id =
                        s.create(&todo.description.get_untracked(), todo.done.get_untracked());
                    todo.db_id.set(Some(new_db_id));
                }
                AppCommand::InsertSelected(todo) => {
                    active.update(|a| a.set(None));
                    selected.update(|s| {
                        s.insert(todo);
                    });
                }
                AppCommand::ToggleSelected(todo) => {
                    active.update(|a| a.set(None));
                    selected.update(|s| {
                        if s.contains(&todo) {
                            s.remove(&todo);
                        } else {
                            s.insert(todo);
                        }
                    });
                }
                AppCommand::SetSelected(todo) => {
                    active.update(|a| a.set(None));
                    selected.update(|s| {
                        s.clear();
                        s.insert(todo);
                    });
                }
                AppCommand::SelectUp => {
                    let todos = todos.get();

                    selected.update(|sel| {
                        // If nothing is selected, select the last item
                        if sel.is_empty() {
                            if let Some(last_todo) = todos.last() {
                                sel.insert(*last_todo);
                                return;
                            }
                            return;
                        }

                        // Find the highest selected index
                        if let Some(highest_selected_idx) =
                            todos.iter().position(|todo| sel.contains(todo))
                        {
                            // If we're not at the top, select the item above
                            if highest_selected_idx > 0 {
                                sel.clear();
                                sel.insert(todos[highest_selected_idx - 1]);
                            } else if sel.len() > 1 {
                                sel.clear();
                                sel.insert(todos[highest_selected_idx]);
                            }
                        }
                    });
                }

                AppCommand::SelectDown => {
                    let todos = todos.get();

                    selected.update(|sel| {
                        // If nothing is selected, select the first item
                        if sel.is_empty() {
                            if let Some(first_todo) = todos.iter().next() {
                                sel.insert(*first_todo);
                                return;
                            }
                            return;
                        }

                        // Find the lowest selected index
                        if let Some(lowest_selected_idx) =
                            todos.iter().rposition(|todo| sel.contains(todo))
                        {
                            // If we're not at the bottom, select the item below
                            if lowest_selected_idx < todos.len() - 1 {
                                sel.clear();
                                sel.insert(todos[lowest_selected_idx + 1]);
                            } else if sel.len() > 1 {
                                sel.clear();
                                sel.insert(todos[lowest_selected_idx]);
                            }
                        }
                    });
                }
                AppCommand::AppAction => {
                    let selected = selected.get();
                    if selected.is_empty() {
                        AppCommand::SelectUp.execute();
                        return;
                    }
                    if selected.len() != 1 {
                        return;
                    }
                    let selected = selected.iter().next().unwrap();
                    active.update(|a| a.set(Some(*selected)));
                }
                AppCommand::SelectRange(todo) => {
                    active.update(|a| a.set(None));
                    selected.update(|s| {
                        if s.is_empty() {
                            s.insert(todo);
                            return;
                        }
                        // Get ordered list of todos
                        let todos = todos.get();

                        // Find indices of the last selected todo and the newly clicked todo
                        let mut start_idx = None;
                        let mut end_idx = None;

                        for (idx, child_todo) in todos.iter().enumerate() {
                            // Find the last selected todo
                            if s.contains(child_todo) {
                                start_idx = Some(idx);
                            }
                            // Find the newly clicked todo
                            if child_todo == &todo {
                                end_idx = Some(idx);
                            }
                        }

                        // If we found both todos, select everything between them
                        if let (Some(start), Some(end)) = (start_idx, end_idx) {
                            let (start, end) = if start <= end {
                                (start, end)
                            } else {
                                (end, start)
                            };

                            // Clear existing selection
                            s.clear();

                            // Select all todos in the range
                            for idx in start..=end {
                                if let Some(child_todo) = todos.get(idx) {
                                    s.insert(*child_todo);
                                }
                            }
                        }
                    });
                }
            }
        });
    }
}
