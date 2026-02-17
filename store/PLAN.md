# Floem Store - Implementation Plan

> **IMPORTANT**: This plan is a living document. Anyone implementing or modifying
> the store crate should update this file to reflect current progress, completed
> tasks, and any changes to the design. Keep it in sync with the actual implementation!

## Motivation: Why Store?

### The Problem with Current Signals

Floem's current signal system (SolidJS-style) has a fundamental issue with **nested signals and scope lifetimes**:

```rust
// Parent creates a struct containing signals
struct Parent {
    items: RwSignal<Vec<Item>>,
}

struct Item {
    name: RwSignal<String>,  // Created in CHILD scope
}

// Scenario:
// 1. Parent passes items to child component
// 2. Child creates Item with signals tied to child's scope
// 3. Child component is unmounted → child scope disposed
// 4. Parent still holds the Item struct
// 5. Parent tries to read item.name → RUNTIME PANIC (dangling signal)
```

The signal ID still exists in the struct, but the underlying reactive node was cleaned up when the child scope was disposed.

### Why Signals Can't Reliably Track Usage

The root cause is the combination of **Copy + arena allocation**:

1. **Copy trait** - `RwSignal<T>` is just an `Id` wrapper, so it's `Copy`. Copy types have no `Drop`, meaning Rust can't track when they go out of scope.

2. **Arena allocation** - Signal data lives in `HashMap<Id, SignalState>` in the Runtime, not owned by the handle. The `Id` can be freely copied everywhere.

3. **Scope-based lifetime** - Data lifetime is tied to scope disposal, not to actual usage of the signal handles.

```rust
let signal = create_rw_signal(42);  // Copy type - just an Id

// Can freely copy everywhere - Rust tracks nothing
let copy1 = signal;
let copy2 = signal;
move_to_closure(signal);
store_in_struct(signal);

// When scope disposes, ALL these copies become dangling
// Rust has no way to know they exist or invalidate them
```

With `Rc`-based Store/Binding, reference counting naturally tracks lifetime - data lives as long as any reference exists.

### The Reconciliation Problem

Another pain point with fine-grained signals: **server data synchronization**.

Consider a Todo app where each property needs fine-grained reactivity:

```rust
// For fine-grained updates, each property is a signal
struct TodoItem {
    text: RwSignal<String>,
    done: RwSignal<bool>,
    priority: RwSignal<i32>,
}

// But server returns plain data
struct ServerTodoItem {
    text: String,
    done: bool,
    priority: i32,
}
```

When fetching from server, you need **manual diffing** to avoid unnecessary updates:

```rust
fn update_from_server(local: &TodoItem, server: ServerTodoItem) {
    // Must manually check each field - tedious and error-prone!
    if local.text.get() != server.text {
        local.text.set(server.text);
    }
    if local.done.get() != server.done {
        local.done.set(server.done);
    }
    if local.priority.get() != server.priority {
        local.priority.set(server.priority);
    }
    // Repeat for every field...
}
```

Problems:
- **Boilerplate** - Must write diff code for every field
- **Error-prone** - Easy to forget a field or make mistakes
- **No automatic reconciliation** - Unlike React's VDOM diffing

Store has the same issue currently - `binding.set(new_value)` always notifies even if unchanged. Future improvements could include:
- `set_if_changed()` method that checks `PartialEq`
- `#[derive(Reconcile)]` macro for automatic field-by-field diffing
- Keyed list reconciliation for collections

### The Solution: Elm-Style Store

Instead of signals scattered across scopes, we use a **central Store** with **Binding handles**:

1. **State lives in Store** - Not tied to any component scope
2. **Bindings are handles** - Clone-able references into the Store (like lenses with data)
3. **Updates are messages** - `binding.set(v)` internally queues an update
4. **No dangling references** - Data outlives components that use it

```rust
// New approach
struct AppState {
    items: Vec<Item>,  // Plain data, no signals
}

struct Item {
    name: String,  // Plain data
}

// Store owns the data
let store = Store::new(AppState::default());

// Bindings are derived handles
let items = store.binding(|s| &s.items, |s| &mut s.items);
let first_name = items.index(0).binding(|i| &i.name, |i| &mut i.name);

// Child component receives Binding, not the data
// When child unmounts, Binding is dropped but Store data remains
first_name.set("Alice".into());  // Works even after child scope cleanup
```

## Design Goals

1. **Coexist with RwSignal** - Gradual migration, not a replacement
2. **Same traits** - `Binding` implements `SignalGet`, `SignalUpdate`, etc.
3. **Fine-grained reactivity** - Each Binding path has its own subscribers
4. **Implicit messages** - No user-defined message enums (unlike traditional Elm)
5. **Clone-friendly** - Bindings are cheap to clone (Rc + lens)

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│                      Store<T>                           │
│  ┌─────────────────────────────────────────────────────┐│
│  │  data: RefCell<T>                                   ││
│  │  subscribers: HashMap<PathId, HashSet<EffectId>>    ││
│  └─────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────┘
           │
           │ derives
           ▼
┌─────────────────────────────────────────────────────────┐
│                  Binding<Root, T, L>                    │
│  ┌─────────────────────────────────────────────────────┐│
│  │  inner: Rc<StoreInner<Root>>  (shared with Store)   ││
│  │  path_id: PathId              (for subscriptions)   ││
│  │  lens: L                      (how to access T)     ││
│  └─────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────┘
           │
           │ implements
           ▼
┌─────────────────────────────────────────────────────────┐
│  SignalGet, SignalWith, SignalUpdate, SignalTrack      │
│  (from floem_reactive - same as RwSignal)              │
└─────────────────────────────────────────────────────────┘
```

## Implementation Plan

### Phase 1: Core Types ✅ COMPLETE

- [x] Create `store` subcrate in workspace
- [x] Add to workspace Cargo.toml
- [x] Add `Runtime::current_effect_id()` and `Runtime::update_from_id()` to reactive crate
- [x] **`path.rs`** - PathId for tracking binding locations
- [x] **`lens.rs`** - Lens trait and implementations
  - [x] `Lens<S, T>` trait (Copy + 'static)
  - [x] `LensFn<S, T, G, M>` - closure-based lens
  - [x] `ComposedLens<L1, L2, M>` - for nested bindings (includes middle type parameter)
  - [x] `IndexLens` - for Vec access
  - [x] `IdentityLens<T>` - for root access
- [x] **`store.rs`** - Store type
  - [x] `Store<T>` struct with Rc<StoreInner>
  - [x] `StoreInner<T>` with data + subscribers
  - [x] `store.root()` → Binding
  - [x] `store.binding(getter, getter_mut)` → Binding
- [x] **`binding.rs`** - Binding type
  - [x] `Binding<Root, T, L>` struct
  - [x] `binding.get()`, `binding.set()`, `binding.update()`
  - [x] `binding.binding(getter, getter_mut)` for nesting
  - [x] Effect subscription via `Runtime::current_effect_id()`
  - [x] Effect notification via `Runtime::update_from_id()`
- [x] **`traits.rs`** - Implement reactive traits on Binding
  - [x] `SignalGet<T>` for Binding
  - [x] `SignalWith<T>` for Binding
  - [x] `SignalUpdate<T>` for Binding
  - [x] `SignalTrack<T>` for Binding
- [x] **`tests.rs`** - Unit tests
  - [x] Basic get/set
  - [x] Nested bindings
  - [x] Vec operations
  - [x] Effect subscription
  - [x] Trait compatibility

### Phase 2: Collection Support ✅ COMPLETE

- [x] `Binding<Vec<T>>` methods
  - [x] `index(usize)` → Binding<T>
  - [x] `push(T)`, `pop()`, `clear()`
  - [x] `iter_bindings()` → Iterator<Item = Binding<T>>
  - [x] `len()`, `is_empty()`
- [x] `Binding<HashMap<K, V>>` methods (K must be Copy + Hash + Eq)
  - [x] `key(k)` → Binding<V>
  - [x] `len()`, `is_empty()`, `contains_key(&k)`
  - [x] `insert(k, v)`, `remove(&k)`, `clear()`
  - [x] `get_value(&k)` → Option<V> (for cloned access)
  - [x] `iter_bindings()` → Iterator<Item = (K, Binding<V>)>

### Phase 3: Convenience Features ✅ COMPLETE

- [x] `lens!` macro for ergonomic field access
  ```rust
  // Instead of: store.binding(|s| &s.user, |s| &mut s.user)
  let user = store.binding_lens(lens!(user));

  // Nested paths work too:
  let name = store.binding_lens(lens!(user.name));
  ```
- [x] `binding!` macro combining store.binding_lens() with lens!()
  ```rust
  let count = binding!(store, count);
  let name = binding!(store, user.name);
  ```
- [x] `binding_lens()` method on Store and Binding to accept lens! tuple
- [x] `#[derive(Lenses)]` macro for automatic lens generation (in `store-derive` crate)
  ```rust
  #[derive(Lenses)]
  struct State {
      count: i32,    // Generates state_lenses::CountLens + StateStore/StateBinding wrappers
      #[nested]
      user: User,    // Returns UserBinding wrapper for nested access
  }

  // Method-style access via wrapper types - NO IMPORTS NEEDED:
  let store = StateStore::new(State::default());
  let count = store.count();
  let name = store.user().name();  // Chained access with #[nested]!

  // Or use with binding_with_lens (still works):
  let count = store.inner().binding_with_lens(state_lenses::CountLens);
  ```

### Phase 4: Testing & Examples ✅ COMPLETE

- [x] Unit tests for Store, Binding, Lens (31 tests passing)
- [x] Integration test with Effects
- [x] Example: Todo app using Store (`examples/todo-store`)
  - Demonstrates Store/Binding usage with Floem views
  - Shows hybrid approach: Store for app state, signals for view-specific state
  - Uses `binding!` macro, `Binding::index()`, nested binding access
- [x] Documentation with migration guide (`store/README.md`)

### Phase 5: Integration with Floem Views ✅ PARTIAL

- [x] Test with `dyn_container` - works for view switching based on Store state
- [x] Test with `dyn_view` - works for reactive text display
- [x] Test with `dyn_stack` - works for filtered list rendering with Binding
- [ ] Ensure proper effect cleanup when views unmount (inherits from scope disposal)
- [ ] Consider adding Store-aware view helpers

## File Structure

```
store/
├── Cargo.toml
├── PLAN.md           (this file)
├── README.md         (user documentation)
└── src/
    ├── lib.rs        (public API re-exports)
    ├── store.rs      (Store type)
    ├── binding.rs    (Binding type)
    ├── lens.rs       (Lens trait and impls)
    ├── path.rs       (PathId for subscriptions)
    ├── traits.rs     (SignalGet etc. impls for Binding)
    └── tests.rs      (unit tests)

store-derive/
├── Cargo.toml
└── src/
    └── lib.rs        (#[derive(Lenses)] proc macro)
```

## Usage Example (Target API)

```rust
use floem_store::Lenses;
use floem_reactive::Effect;

#[derive(Lenses, Default, Clone, PartialEq)]
struct AppState {
    #[nested]
    user: User,
    #[nested(key = id)]  // Use `id` field for reconciliation AND by_id() access (type inferred)
    todos: Vec<Todo>,
}

#[derive(Lenses, Default, Clone, PartialEq)]
struct User {
    name: String,
    email: String,
}

#[derive(Lenses, Default, Clone, PartialEq)]
struct Todo {
    id: u64,
    text: String,
    done: bool,
}

fn main() {
    // Create typed store wrapper (generated by derive)
    let store = AppStateStore::new(AppState::default());

    // Access fields via generated methods
    let name = store.user().name();
    let todos = store.todos();

    // Set values
    name.set("Alice".into());

    // Create effect that tracks the binding
    let name_clone = name.clone();
    Effect::new(move |_| {
        println!("Name changed to: {}", name_clone.get());
    });

    // This triggers the effect
    name.set("Bob".into());

    // Vec operations with typed wrappers
    todos.push(Todo { id: 1, text: "Learn Floem".into(), done: false });
    todos.push(Todo { id: 2, text: "Build app".into(), done: false });

    // Position-based access (may change after reorder)
    let first_text = todos.index(0).text();
    println!("First todo: {}", first_text.get());

    // Identity-based access (stable across reorders!)
    let todo1 = todos.by_id(1);
    let todo1_text = todo1.text();
    println!("Todo #1: {}", todo1_text.get());  // Always gets the todo with id=1

    // Helper methods for identity-based operations
    if todos.contains_key(&2) {
        todos.remove_by_key(&2);  // Remove by id, not position
    }

    // Get all bindings for use with dyn_stack
    let all_bindings = todos.all_bindings();
    for binding in all_bindings {
        println!("Todo: {}", binding.text().get());
    }

    // Filtered bindings - perfect for dyn_stack views
    // Returns impl Iterator, collect when needed
    let active_bindings: Vec<_> = todos.filtered_bindings(|t| !t.done).collect();
    // Use in dyn_stack: each_fn returns Vec<TodoBinding>, view_fn receives binding directly
    // dyn_stack(
    //     move || todos.filtered_bindings(|t| !t.done).collect::<Vec<_>>(),
    //     |binding| binding.id().get_untracked(),  // Key function
    //     move |binding| todo_item_view(binding),  // View receives binding!
    // )

    // Reconcile with server data - only updates changed fields
    // For todos, if ids match in same order, each item is reconciled individually
    store.reconcile(&AppState {
        user: User { name: "Charlie".into(), email: "charlie@example.com".into() },
        todos: vec![Todo { id: 1, text: "Updated".into(), done: true }],
    });
}
```

### Phase 6: LocalState ✅ COMPLETE

- [x] `LocalState<T>` - Simple atomic reactive value
  - [x] `LocalState::new(value)` - Create new LocalState
  - [x] `get()`, `get_untracked()` - Read value
  - [x] `set(value)` - Set value
  - [x] `update(f)`, `try_update(f)` - Update with closure
  - [x] `with(f)`, `with_untracked(f)` - Read by reference
  - [x] Implements SignalGet, SignalWith, SignalUpdate, SignalTrack
  - [x] Clone (not Copy, Rc-based)
  - [x] Default implementation
  - [x] 7 unit tests passing

## Current Progress

**Phase 1-6 Substantially Complete!**

- Created subcrate structure
- Added `Runtime::current_effect_id()` and `Runtime::update_from_id()` to reactive crate
- Implemented all core types: Store, Binding, Lens, PathId
- Implemented reactive traits on Binding
- Added Vec collection support with `index()`, `push()`, `pop()`, `clear()`, `iter_bindings()`
- Added HashMap support with `key()`, `get_value()`, `insert()`, `remove()`, `iter_bindings()`
  - Note: `key()` and `iter_bindings()` require K: Copy due to Lens trait constraints
  - Non-Copy keys work with `get_value()`, `insert()`, `remove()`, `update()`
- Created `store-derive` crate with `#[derive(Lenses)]` proc macro
  - Generates lens types for each struct field (named `{FieldName}Lens` to avoid shadowing)
  - Generates wrapper types (`StateStore`, `StateBinding`) with direct method access
  - `#[nested]` attribute for fully import-free nested access (works at multiple levels)
  - `#[nested]` also works on `Vec<T>` fields where T has `#[derive(Lenses)]`
  - `#[nested]` also works on `HashMap<K, V>` fields where V has `#[derive(Lenses)]`
  - No trait imports needed - just use wrapper types!
- Renamed `Field` to `Binding` for clearer semantics:
  - Lens = stateless accessor recipe (how to navigate data)
  - Binding = live handle with data reference + reactivity
- Added dead effect cleanup in `notify_subscribers()`:
  - Added `Runtime::effect_exists(effect_id)` to reactive crate for checking if an effect is still alive
  - Both `Binding` and `LocalState` now clean up dead effect IDs during notification
  - This prevents minor memory leak where disposed effect IDs would accumulate in subscriber HashSets
- Added `reconcile()` method to binding wrappers generated by `#[derive(Lenses)]`:
  - Automatically compares each field and only updates changed fields
  - For `#[nested]` fields, calls `reconcile()` recursively
  - For `Vec`/`HashMap` fields, compares and replaces the whole collection if different
  - Requires `Clone + PartialEq` bounds on the struct
  - Solves the "server data synchronization" pain point documented above
- Changed `PathId` from incrementing counter to hash-based:
  - Bindings with the same normalized lens path share the same `PathId`
  - Effects subscribed to one binding see updates from other bindings on the same path
  - Required for reconcile to work correctly with existing effects
- Implemented lens path normalization:
  - `store.count()` and `store.root().count()` now share the same PathId
  - Works for arbitrarily deep nesting: `store.nested().value()` == `store.root().nested().value()`
  - Uses hash-based path composition that strips identity lenses
- Added `store.reconcile()` shortcut (equivalent to `store.root().reconcile()`)
- **Simplified API by removing closure-based binding methods**:
  - Removed `store.binding()`, `store.binding_lens()`, `binding.binding()`, `binding.binding_lens()`
  - Removed `LensFn`, `lens!` macro, and `binding!` macro
  - The derive-generated accessor methods are now the only way to create bindings
  - This avoids the "closure type uniqueness" problem where identical closures would get different PathIds
  - `binding_with_lens()` kept as `#[doc(hidden)]` for derive macro internal use
- Added keyed list reconciliation with `#[nested(key = field)]` attribute:
  - When reconciling a Vec, if keys match in the same order, reconcile items individually
  - If structure differs (keys added/removed/reordered), replace the entire Vec
  - Example: `#[nested(key = id)] todos: Vec<Todo>` uses `todo.id` as the key
  - Solves the problem of losing fine-grained reactivity when reconciling lists
- Added identity-based Vec access with `#[nested(key = field)]` attribute:
  - When a key field is specified, `by_{field}()`, `contains_key()`, and `remove_by_key()` methods are generated
  - The key type is automatically inferred from the inner type's field type alias
  - Example: `#[nested(key = id)] todos: Vec<Todo>` generates `todos.by_id(id)` (type inferred from `Todo::id`)
  - Explicit type can still be provided: `#[nested(key = id: u64)]` (useful for complex types)
  - `by_id(5)` returns a binding to the item with `id == 5`, regardless of position
  - Unlike `index(0)`, the binding stays attached to the same logical item after reorders
  - PathId is based on key value, not position - enables per-item effect isolation
- Added per-index and per-key PathId isolation:
  - `IndexLens` now includes the index in its path hash (`todos[0].text` ≠ `todos[1].text`)
  - `KeyLens` now includes the key's hash in its path hash
  - Effects subscribed to one index/key are NOT triggered by updates to other indices/keys
  - Enables true per-item fine-grained reactivity in collections
- Added `filtered_bindings()` and `all_bindings()` helpers to Vec wrappers (for `#[nested(key = field)]`):
  - `filtered_bindings(|item| predicate)` returns `impl Iterator<Item = ItemBinding>` matching the filter
  - `all_bindings()` returns `impl Iterator<Item = ItemBinding>` for all items
  - Returns iterator (keys collected internally, bindings created lazily)
  - For `dyn_stack`, collect into Vec: `todos.filtered_bindings(...).collect::<Vec<_>>()`
  - Each binding is already connected to its item via identity-based access
  - Example: `todos.filtered_bindings(|t| !t.done)` returns iterator of bindings to all active todos
- All 53 unit tests passing (including `test_filtered_bindings`)
- Workspace compiles successfully
- Created `todo-store` example demonstrating Store with Floem views:
  - `dyn_container` for view switching (filter tabs)
  - `dyn_view` for reactive text display
  - `dyn_stack` with `filtered_bindings()` for filtered list rendering
  - Identity-based access with `by_id()` for stable bindings across reorders
  - `remove_by_key()` for identity-based deletion
  - Demonstrates passing bindings directly to view functions (no id lookup needed)

## Next Steps

1. Consider view-aware helpers for Store
2. Add `set_if_changed()` method to Binding and LocalState (requires `T: PartialEq`)
3. ~~Consider `#[derive(Reconcile)]` macro for automatic field-by-field diffing~~ ✅ Implemented as `binding.reconcile(&new_value)`
4. ~~Consider keyed list reconciliation for Vec bindings (currently replaces entire Vec)~~ ✅ Implemented with `#[nested(key = field)]`
5. ~~Consider normalizing lens paths (e.g., strip IdentityLens) for more consistent PathId matching~~ ✅ Implemented
6. ~~Remove closure-based API to fix PathId inconsistency~~ ✅ Implemented
