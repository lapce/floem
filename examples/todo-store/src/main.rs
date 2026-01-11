//! Todo app demonstrating the floem_store crate with IndexMap.
//!
//! This example shows how to use Store with IndexMap for O(1) key lookup.
//! The key benefits are:
//! - O(1) access by key (vs O(N) for Vec with by_id)
//! - State is centralized and not tied to any component scope
//! - Binding handles can be passed around freely without lifetime issues
//! - Updates automatically trigger reactive effects
//! - Insertion order is preserved for iteration
//!
//! Note: This example uses a hybrid approach where some views still use signals
//! (like text_input), demonstrating how Store can coexist with the signal system.
//!
//! This example demonstrates:
//! - `#[derive(Lenses)]` for zero-import wrapper types
//! - `IndexMap<K, V>` for O(1) keyed access with preserved insertion order
//! - `dyn_view` for reactive text display
//! - `dyn_stack` for reactive list rendering
//! - `dyn_container` for view switching based on Store state
//! - `filtered_bindings()` for filtered iteration with bindings
//! - Proper integration of Store with Floem's reactive view system

use std::sync::atomic::{AtomicU64, Ordering};

use floem::{prelude::*, unit::UnitExt, views::dyn_container};
use floem_store::{IndexMap, Lenses};

/// Counter for generating unique todo IDs.
static NEXT_TODO_ID: AtomicU64 = AtomicU64::new(1);

fn next_todo_id() -> u64 {
    NEXT_TODO_ID.fetch_add(1, Ordering::Relaxed)
}

/// A single todo item with a stable identity.
#[derive(Clone, Default, PartialEq, Lenses)]
struct Todo {
    id: u64,
    text: String,
    done: bool,
}

/// View mode for the todo list
#[derive(Clone, Copy, PartialEq, Default)]
enum ViewMode {
    #[default]
    All,
    Active,
    Completed,
}

/// The application state.
///
/// Using `IndexMap<u64, Todo>` instead of `Vec<Todo>` gives us:
/// - O(1) lookup by key (vs O(N) for Vec with by_id)
/// - Preserved insertion order for iteration
/// - `#[nested(key = id)]` provides `push()` and `filtered_bindings()` convenience methods
#[derive(Clone, Default, PartialEq, Lenses)]
struct AppState {
    #[nested(key = id)]
    todos: IndexMap<u64, Todo>,
    view_mode: ViewMode,
}

fn app_view() -> impl IntoView {
    // Create the store using the generated wrapper type - no imports needed!
    let store = AppStateStore::new(AppState::default());

    // Get binding handles using the generated wrapper methods
    let todos = store.todos();
    let view_mode = store.view_mode();

    // For text input, we still use a signal (demonstrating hybrid approach)
    // In a future version, text_input could accept Binding directly
    let new_todo_text = RwSignal::new(String::new());

    // Header with title
    let header = "Todo Store Example (IndexMap)"
        .style(|s| s.font_size(24.0).margin_bottom(20.0));

    // Input for new todos (using signal for text_input compatibility)
    let input = {
        let todos = todos.clone();

        text_input(new_todo_text)
            .placeholder("What needs to be done?")
            .style(|s| {
                s.width(300.0)
                    .padding(10.0)
                    .border(1.0)
                    .border_radius(5.0)
                    .border_color(palette::css::GRAY)
            })
            .on_key_down(
                Key::Named(NamedKey::Enter),
                |m| m.is_empty(),
                move |_| {
                    let text = new_todo_text.get();
                    if !text.trim().is_empty() {
                        // Store update: push to the todos IndexMap
                        // `push()` extracts the id from the Todo automatically
                        todos.push(Todo {
                            id: next_todo_id(),
                            text: text.trim().to_string(),
                            done: false,
                        });
                        new_todo_text.set(String::new());
                    }
                },
            )
    };

    // Add button
    let add_button = {
        let todos = todos.clone();

        "Add"
            .style(|s| {
                s.padding(10.0)
                    .margin_left(10.0)
                    .background(palette::css::LIGHT_BLUE)
                    .border_radius(5.0)
                    .hover(|s| s.background(palette::css::DEEP_SKY_BLUE))
                    .active(|s| s.background(palette::css::DODGER_BLUE))
            })
            .on_click_stop(move |_| {
                let text = new_todo_text.get();
                if !text.trim().is_empty() {
                    todos.push(Todo {
                        id: next_todo_id(),
                        text: text.trim().to_string(),
                        done: false,
                    });
                    new_todo_text.set(String::new());
                }
            })
    };

    let input_row = (input, add_button).style(|s| s.flex_row().items_center().margin_bottom(20.0));

    // Filter tabs - demonstrates dyn_container with Store
    let filter_tabs = {
        let view_mode = view_mode.clone();

        let make_tab = |mode: ViewMode, label: &'static str| {
            let view_mode = view_mode.clone();
            let view_mode_for_style = view_mode.clone();

            label
                .style(move |s| {
                    let is_active = view_mode_for_style.get() == mode;
                    s.padding(8.0)
                        .margin_right(5.0)
                        .border_radius(5.0)
                        .apply_if(is_active, |s| s.background(palette::css::LIGHT_BLUE))
                        .apply_if(!is_active, |s| {
                            s.background(palette::css::LIGHT_GRAY)
                                .hover(|s| s.background(palette::css::SILVER))
                        })
                })
                .on_click_stop(move |_| {
                    view_mode.set(mode);
                })
        };

        (
            make_tab(ViewMode::All, "All"),
            make_tab(ViewMode::Active, "Active"),
            make_tab(ViewMode::Completed, "Completed"),
        )
            .style(|s| s.flex_row().margin_bottom(10.0))
    };

    // Todo list using dyn_container - switches view based on filter mode
    // This demonstrates Store integration with dyn_container for view switching
    let todo_list = {
        let todos = todos.clone();
        let view_mode = view_mode.clone();

        dyn_container(
            move || view_mode.get(),
            move |mode| {
                let todos = todos.clone();
                filtered_todo_list(todos, mode).into_any()
            },
        )
        .style(|s| s.min_width(350.0))
    };

    // Stats footer - reactive to todo changes
    let stats = {
        let todos = todos.clone();

        dyn_view(move || {
            let total = todos.len();
            let done = todos.with(|t| t.values().filter(|todo| todo.done).count());
            let active = total - done;
            format!("{} items left, {} completed", active, done)
        })
        .style(|s| s.margin_top(20.0).color(palette::css::GRAY))
    };

    // Clear completed button
    let clear_button = {
        let todos = todos.clone();

        "Clear Completed"
            .style(|s| {
                s.padding(8.0)
                    .margin_top(10.0)
                    .background(palette::css::LIGHT_CORAL)
                    .border_radius(5.0)
                    .hover(|s| s.background(palette::css::INDIAN_RED))
                    .active(|s| s.background(palette::css::DARK_RED))
            })
            .on_click_stop(move |_| {
                // Store update: filter out completed todos
                todos.update(|t| t.retain(|_k, todo| !todo.done));
            })
    };

    (
        header,
        input_row,
        filter_tabs,
        todo_list,
        stats,
        clear_button,
    )
        .style(|s| {
            s.flex_col()
                .items_center()
                .padding(40.0)
                .size(100.pct(), 100.pct())
        })
        .on_key_up(
            Key::Named(NamedKey::F11),
            |m| m.is_empty(),
            move |_| floem::action::inspect(),
        )
}

/// Render a filtered todo list based on the view mode.
///
/// This demonstrates `filtered_bindings()` with IndexMap - O(1) access!
/// The each_fn returns an iterator of bindings, and
/// the view_fn receives `TodoBinding` so it can access fields directly.
///
/// Using `filtered_bindings()` provides:
/// - Clean API: filter with plain `&Todo` reference, get bindings back
/// - O(1) access: each binding uses KeyLens for O(1) IndexMap lookup
/// - Full reactivity: bindings are connected to the store
fn filtered_todo_list<L: floem_store::Lens<AppState, IndexMap<u64, Todo>>>(
    todos: TodosIndexMapBinding<AppState, L>,
    mode: ViewMode,
) -> impl IntoView {
    let todos_for_delete = todos.clone();

    dyn_stack(
        move || {
            // filtered_bindings returns an iterator of TodoBinding
            // We collect for dyn_stack (it needs to iterate multiple times for diffing)
            todos
                .filtered_bindings(|todo| match mode {
                    ViewMode::All => true,
                    ViewMode::Active => !todo.done,
                    ViewMode::Completed => todo.done,
                })
                .collect::<Vec<_>>()
        },
        // Key function: extract id from the binding without subscribing
        |binding| binding.id().get_untracked(),
        // View function receives the binding directly!
        move |binding| todo_item(todos_for_delete.clone(), binding),
    )
    .style(|s| s.flex_col().gap(5.0))
}

/// Render a single todo item.
///
/// This function receives a `TodoBinding` directly from `filtered_bindings()`.
/// No need to call `get()` - the binding is already connected to the right item!
/// TodoBinding has .done() and .text() methods - no manual bindings needed!
///
/// We also receive the parent IndexMap binding for delete operations.
fn todo_item<L1, L2>(
    todos: TodosIndexMapBinding<AppState, L1>,
    todo: TodoBinding<AppState, L2>,
) -> impl IntoView
where
    L1: floem_store::Lens<AppState, IndexMap<u64, Todo>>,
    L2: floem_store::Lens<AppState, Todo>,
{
    // Access nested fields using wrapper methods - binding already points to our item!
    let done = todo.done();
    let text = todo.text();
    let id = todo.id().get_untracked(); // Get id for removal

    // Checkbox that toggles the done state
    let checkbox_view = {
        let done_for_display = done.clone();
        let done_for_click = done.clone();
        checkbox(move || done_for_display.get())
            .on_click_stop(move |_| {
                done_for_click.update(|d| *d = !*d);
            })
            .style(|s| s.margin_right(10.0))
    };

    // Label that shows the todo text with strikethrough if done
    let label = {
        let done = done.clone();
        dyn_view(move || text.get()).style(move |s| {
            if done.get() {
                s.color(palette::css::GRAY)
            } else {
                s
            }
        })
    };

    // Delete button - uses remove_by_key on the parent binding (O(1))
    let delete_button = "X"
        .style(|s| {
            s.margin_left(10.0)
                .padding_horiz(8.0)
                .padding_vert(4.0)
                .background(palette::css::LIGHT_GRAY)
                .border_radius(3.0)
                .hover(|s| s.background(palette::css::RED).color(palette::css::WHITE))
        })
        .on_click_stop(move |_| {
            // Remove by id using the parent IndexMap binding (O(1))
            todos.remove_by_key(&id);
        });

    (checkbox_view, label, delete_button).style(|s| {
        s.flex_row()
            .items_center()
            .padding(10.0)
            .background(palette::css::WHITE_SMOKE)
            .border_radius(5.0)
            .width(350.0)
    })
}

fn main() {
    floem::launch(app_view);
}
