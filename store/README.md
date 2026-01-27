# Floem Store

Elm-style state management for Floem with structural data access.

## Overview

`floem_store` provides an alternative to signals for managing complex, structured state. Instead of nesting signals inside structs (which can cause lifetime issues when child scopes are disposed), state lives in a central `Store` and is accessed via `Binding` handles.

## Key Concepts

- **Store**: Central state container for complex nested state
- **Binding**: Live handle pointing to a location in the Store (combines data reference + reactivity)
- **Lens**: Stateless accessor recipe for navigating data structures
- **LocalState**: Simple reactive value for single values (no nesting)

## The Problem Store Solves

With traditional signals, nested signal structs can cause runtime panics:

```rust
// Problem: signals tied to child scope
struct Item {
    name: RwSignal<String>,  // Created in child scope
}

// When child component unmounts, the scope is disposed
// Parent still holds Item struct, but signal is gone
// Accessing item.name causes a runtime panic!
```

## The Solution

Store keeps all state in one place, accessed via Binding handles:

```rust
use floem_store::{Store, binding};

#[derive(Default)]
struct AppState {
    items: Vec<Item>,
}

#[derive(Clone, Default)]
struct Item {
    name: String,  // Plain data, no signals
}

// Store owns the data
let store = Store::new(AppState::default());

// Bindings are handles that point into the Store
let items = binding!(store, items);
let first_name = items.index(0).binding(|i| &i.name, |i| &mut i.name);

// Child components receive Binding handles
// When child unmounts, Binding is dropped but Store data remains safe
first_name.set("Alice".into());  // Always works!
```

## Quick Start

Add to your `Cargo.toml`:

```toml
[dependencies]
floem_store = { path = "path/to/store" }
```

### Basic Usage

```rust
use floem_store::{Store, binding};

#[derive(Default)]
struct State {
    count: i32,
    name: String,
}

// Create a store
let store = Store::new(State::default());

// Get binding handles with the binding! macro
let count = binding!(store, count);
let name = binding!(store, name);

// Read and write
count.set(42);
assert_eq!(count.get(), 42);

name.set("Hello".into());
assert_eq!(name.get(), "Hello");

// Update with closure
count.update(|c| *c += 1);
```

### Nested Bindings

```rust
#[derive(Default)]
struct State {
    user: User,
}

#[derive(Default)]
struct User {
    name: String,
    age: i32,
}

let store = Store::new(State::default());

// Access nested fields with path syntax
let name = binding!(store, user.name);
let age = binding!(store, user.age);

// Or chain binding access
let user = binding!(store, user);
let name = user.binding(|u| &u.name, |u| &mut u.name);
```

### Vec Operations

```rust
#[derive(Default)]
struct State {
    items: Vec<Item>,
}

let store = Store::new(State::default());
let items = binding!(store, items);

// Vec methods
items.push(Item { text: "First".into() });
items.push(Item { text: "Second".into() });

assert_eq!(items.len(), 2);

// Access by index
let first = items.index(0);
let text = first.binding(|i| &i.text, |i| &mut i.text);
assert_eq!(text.get(), "First");

// Iterate over bindings
for item_binding in items.iter_bindings() {
    let text = item_binding.binding(|i| &i.text, |i| &mut i.text);
    println!("{}", text.get());
}

// Other Vec operations
items.pop();
items.clear();
```

### HashMap Operations

```rust
use std::collections::HashMap;

#[derive(Default)]
struct State {
    users: HashMap<u32, String>,  // Keys must be Copy + Hash + Eq
}

let store = Store::new(State::default());
let users = binding!(store, users);

// Insert and remove
users.insert(1, "Alice".into());
users.insert(2, "Bob".into());
assert_eq!(users.len(), 2);

// Access by key (key must be Copy)
let alice = users.key(1);
assert_eq!(alice.get(), "Alice");
alice.set("Alice Smith".into());

// Check and get values
assert!(users.contains_key(&1));
assert_eq!(users.get_value(&2), Some("Bob".to_string()));

// Iterate over bindings
for (id, user_binding) in users.iter_bindings() {
    println!("{}: {}", id, user_binding.get());
}

// Remove and clear
users.remove(&1);
users.clear();
```

### With Floem Views

Binding implements the same reactive traits as signals, so it works seamlessly with Floem views:

```rust
use floem::prelude::*;
use floem_store::{Store, binding};

fn app_view() -> impl IntoView {
    let store = Store::new(State::default());
    let count = binding!(store, count);

    // dyn_view reacts to Binding changes
    let display = dyn_view(move || format!("Count: {}", count.get()));

    // Buttons update the Binding
    let increment = "+"
        .on_click_stop(move |_| count.update(|c| *c += 1));

    (display, increment)
}
```

### With dyn_container

```rust
use floem::views::dyn_container;

#[derive(Clone, Copy, PartialEq)]
enum ViewMode { List, Grid }

let view_mode = binding!(store, view_mode);

dyn_container(
    move || view_mode.get(),
    move |mode| match mode {
        ViewMode::List => list_view().into_any(),
        ViewMode::Grid => grid_view().into_any(),
    },
)
```

## API Reference

### Store

- `Store::new(value)` - Create a new store
- `store.binding(getter, getter_mut)` - Get a binding handle
- `store.binding_lens(lens!(path))` - Get a binding using lens macro
- `store.binding_with_lens(lens)` - Get a binding using a derived lens type
- `store.root()` - Get a binding for the entire state
- `store.with(|state| ...)` - Read entire state
- `store.update(|state| ...)` - Update entire state

### Binding

- `binding.get()` - Get value (cloned), subscribes to changes
- `binding.set(value)` - Set value, notifies subscribers
- `binding.update(|v| ...)` - Update with closure
- `binding.with(|v| ...)` - Read by reference, subscribes
- `binding.with_untracked(|v| ...)` - Read without subscribing
- `binding.binding(getter, getter_mut)` - Derive nested binding
- `binding.binding_lens(lens!(path))` - Derive using lens macro
- `binding.binding_with_lens(lens)` - Derive using a derived lens type

### Vec Bindings

- `binding.index(i)` - Get binding for element at index
- `binding.len()` - Get length
- `binding.is_empty()` - Check if empty
- `binding.push(value)` - Push element
- `binding.pop()` - Pop element
- `binding.clear()` - Clear all elements
- `binding.iter_bindings()` - Iterate as bindings

### HashMap Bindings

For `HashMap<K, V>` where K is Copy + Hash + Eq:

- `binding.key(k)` - Get binding for value at key (requires K: Copy)
- `binding.len()` - Get number of entries
- `binding.is_empty()` - Check if empty
- `binding.contains_key(&k)` - Check if key exists
- `binding.get_value(&k)` - Get cloned value if present
- `binding.insert(k, v)` - Insert key-value pair
- `binding.remove(&k)` - Remove by key
- `binding.clear()` - Clear all entries
- `binding.iter_bindings()` - Iterate as (key, binding) pairs (requires K: Copy)

### LocalState

- `LocalState::new(value)` - Create new LocalState
- `local_state.get()` - Get value (cloned), subscribes to changes
- `local_state.get_untracked()` - Get without subscribing
- `local_state.set(value)` - Set value, notifies subscribers
- `local_state.update(|v| ...)` - Update with closure
- `local_state.with(|v| ...)` - Read by reference, subscribes
- `local_state.with_untracked(|v| ...)` - Read without subscribing

### Macros

- `lens!(field)` - Create getter tuple for a field
- `lens!(a.b.c)` - Create getter tuple for nested path
- `binding!(store, path)` - Shorthand for `store.binding_lens(lens!(path))`

### Derive Macro

Use `#[derive(Lenses)]` to automatically generate lens types and wrapper types:

```rust
use floem_store::{Store, Lenses};

#[derive(Lenses, Default)]
struct State {
    count: i32,
    #[nested]  // Mark fields that also have #[derive(Lenses)]
    user: User,
}

#[derive(Lenses, Default)]
struct User {
    name: String,
    age: i32,
}

// Use the generated wrapper type - NO IMPORTS NEEDED!
let store = StateStore::new(State::default());
let count = store.count();  // Direct method access
let name = store.user().name();  // Nested access also works!
count.set(42);
name.set("Alice".into());
```

The derive macro generates:
- A module `<struct>_lenses` with lens types (e.g., `CountLens`, `UserLens`) for use with `binding_with_lens()`
- A wrapper type `<Struct>Store` with direct method access (no imports needed!)
- A wrapper type `<Struct>Binding` for binding wrappers

The `#[nested]` attribute tells the macro that a field's type also has `#[derive(Lenses)]`,
so it returns the wrapper type instead of raw `Binding`, enabling fully import-free nested access.
This works at multiple levels of nesting (e.g., `store.level1().level2().level3().value()`).

It also works with `Vec<T>` fields where T has `#[derive(Lenses)]`:

```rust
#[derive(Lenses, Default)]
struct AppState {
    #[nested]
    items: Vec<Item>,  // Vec<T> where T has #[derive(Lenses)]
}

#[derive(Lenses, Default, Clone)]
struct Item {
    text: String,
    done: bool,
}

let store = AppStateStore::new(AppState::default());

// items() returns a Vec wrapper with all Vec methods
let items = store.items();
items.push(Item { text: "Task".into(), done: false });

// index() returns ItemBinding, not raw Binding!
let first = items.index(0);
first.text().set("Updated".into());  // Direct method access!
first.done().set(true);
```

It also works with `HashMap<K, V>` fields where V has `#[derive(Lenses)]`:

```rust
use std::collections::HashMap;

#[derive(Lenses, Default)]
struct GameState {
    #[nested]
    players: HashMap<u32, Player>,  // HashMap<K, V> where V has #[derive(Lenses)]
}

#[derive(Lenses, Default, Clone)]
struct Player {
    name: String,
    score: i32,
}

let store = GameStateStore::new(GameState::default());

// players() returns a HashMap wrapper with all HashMap methods
let players = store.players();
players.insert(1, Player { name: "Alice".into(), score: 100 });

// key() returns PlayerBinding, not raw Binding! (requires K: Copy)
let player1 = players.key(1);
player1.name().set("Alice Smith".into());  // Direct method access!
player1.score().set(150);
```

## LocalState: Simple Reactive Values

For simple values that don't need nested access, `LocalState<T>` provides a simpler API:

```rust
use floem_store::LocalState;

// Create a simple reactive value
let count = LocalState::new(0);
let name = LocalState::new("Alice".to_string());

// Read and write
assert_eq!(count.get(), 0);
count.set(42);
assert_eq!(count.get(), 42);

// Update with closure
count.update(|c| *c += 1);
assert_eq!(count.get(), 43);

// Access by reference
let len = name.with(|s| s.len());
```

### When to Use LocalState vs Store

| Use Case | Recommendation |
|----------|----------------|
| Single value (counter, flag, text) | `LocalState<T>` |
| Nested structs with field access | `Store` + `Binding` |
| Collections with item bindings | `Store` + `Binding` |
| Multiple related values | `Store` with struct |

### LocalState with Floem Views

```rust
use floem::prelude::*;
use floem_store::LocalState;

fn counter_view() -> impl IntoView {
    let count = LocalState::new(0);

    // dyn_view reacts to LocalState changes
    let display = dyn_view(move || format!("Count: {}", count.get()));

    // Buttons update the LocalState
    let increment = "+"
        .on_click_stop(move |_| count.update(|c| *c += 1));

    (display, increment)
}
```

## Migration from Signals

### Before (Signals)

```rust
struct State {
    count: RwSignal<i32>,
    items: RwSignal<Vec<Item>>,
}

struct Item {
    name: RwSignal<String>,
}

// Create with signals
let state = State {
    count: RwSignal::new(0),
    items: RwSignal::new(vec![]),
};

// Use signal methods
state.count.set(42);
let val = state.count.get();
```

### After (Store)

```rust
#[derive(Default)]
struct State {
    count: i32,          // Plain data
    items: Vec<Item>,
}

#[derive(Clone, Default)]
struct Item {
    name: String,        // Plain data
}

// Create store
let store = Store::new(State::default());

// Get binding handles
let count = binding!(store, count);
let items = binding!(store, items);

// Same API as signals!
count.set(42);
let val = count.get();
```

### Hybrid Approach

Store coexists with signals. Use Store for app state, signals for view-local state:

```rust
fn my_view() -> impl IntoView {
    // App state from Store
    let items = binding!(store, items);

    // View-local state as signal (for text_input compatibility)
    let input_text = RwSignal::new(String::new());

    (
        text_input(input_text),
        dyn_stack(
            move || (0..items.len()).collect::<Vec<_>>(),
            |i| *i,
            move |i| item_view(items.index(i)),
        ),
    )
}
```

## Design Principles

1. **Coexist with signals** - Gradual migration, not replacement
2. **Same traits** - Binding implements SignalGet, SignalUpdate, etc.
3. **Fine-grained reactivity** - Each binding path has its own subscribers
4. **Implicit messages** - No user-defined message enums (unlike traditional Elm)
5. **Clone-friendly** - Bindings are cheap to clone (Rc-based)

## Example

See `examples/todo-store` for a complete todo app demonstrating:
- Store and Binding usage
- dyn_container for view switching
- dyn_stack for filtered lists
- Hybrid approach with signals
