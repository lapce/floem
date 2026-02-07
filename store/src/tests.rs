#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::collections::HashMap;
    use std::rc::Rc;

    use floem_reactive::{Effect, Runtime};

    // ===== Derive Macro Tests =====

    // Alias crate as floem_store so the derive macro's generated code works
    use crate as floem_store;
    use crate::Lenses;

    #[derive(Lenses, Default, Clone, PartialEq)]
    struct DeriveTestState {
        count: i32,
        name: String,
        #[nested]  // Mark as nested so wrapper returns DeriveTestNestedBinding
        nested: DeriveTestNested,
    }

    #[derive(Lenses, Default, Clone, PartialEq)]
    struct DeriveTestNested {
        value: f64,
        flag: bool,
    }

    #[test]
    fn test_derive_lenses_basic() {
        // No imports needed - use the generated wrapper type
        let store = DeriveTestStateStore::new(DeriveTestState {
            count: 42,
            name: "Alice".into(),
            ..Default::default()
        });

        // Method-style access via wrapper type
        let count = store.count();
        let name = store.name();

        assert_eq!(count.get(), 42);
        assert_eq!(name.get(), "Alice");

        count.set(100);
        name.set("Bob".into());

        assert_eq!(count.get(), 100);
        assert_eq!(name.get(), "Bob");
    }

    #[test]
    fn test_derive_lenses_nested() {
        // No imports needed - use the generated wrapper type with #[nested]
        let store = DeriveTestStateStore::new(DeriveTestState {
            nested: DeriveTestNested {
                value: 3.14,
                flag: true,
            },
            ..Default::default()
        });

        // Chain method calls: store.nested().value()
        // Works without imports because nested field is marked with #[nested]
        let value = store.nested().value();
        let flag = store.nested().flag();

        assert!((value.get() - 3.14).abs() < 0.001);
        assert!(flag.get());

        value.set(2.71);
        flag.set(false);

        assert!((value.get() - 2.71).abs() < 0.001);
        assert!(!flag.get());
    }

    #[test]
    fn test_derive_lenses_with_effects() {
        // No imports needed - use wrapper type
        let store = DeriveTestStateStore::new(DeriveTestState {
            count: 0,
            ..Default::default()
        });

        let count = store.count();
        let run_count = Rc::new(Cell::new(0));

        {
            let run_count = run_count.clone();
            let count = count.clone();
            Effect::new(move |_| {
                let _ = count.get();
                run_count.set(run_count.get() + 1);
            });
        }

        // Initial run
        assert_eq!(run_count.get(), 1);

        // Change triggers effect
        count.set(1);
        Runtime::drain_pending_work();
        assert_eq!(run_count.get(), 2);
    }

    #[test]
    fn test_wrapper_store_basic() {
        // Test the generated StateStore wrapper - no imports needed!
        let store = DeriveTestStateStore::new(DeriveTestState {
            count: 42,
            name: "Alice".into(),
            ..Default::default()
        });

        // Direct method access without any trait imports
        let count = store.count();
        let name = store.name();

        assert_eq!(count.get(), 42);
        assert_eq!(name.get(), "Alice");

        count.set(100);
        name.set("Bob".into());

        assert_eq!(count.get(), 100);
        assert_eq!(name.get(), "Bob");
    }

    #[test]
    fn test_wrapper_store_nested() {
        // With #[nested] attribute, NO IMPORTS NEEDED for nested access!
        let store = DeriveTestStateStore::new(DeriveTestState {
            nested: DeriveTestNested {
                value: 3.14,
                flag: true,
            },
            ..Default::default()
        });

        // First level: no import needed (using wrapper)
        // With #[nested], store.nested() returns DeriveTestNestedBinding, not raw Binding
        let nested_binding = store.nested();

        // Second level: ALSO no import needed because nested_binding is a wrapper!
        let value = nested_binding.value();
        let flag = nested_binding.flag();

        assert!((value.get() - 3.14).abs() < 0.001);
        assert!(flag.get());
    }

    #[test]
    fn test_wrapper_binding() {
        // Test the generated StateBinding wrapper
        let store = DeriveTestStateStore::new(DeriveTestState {
            count: 10,
            ..Default::default()
        });

        // Get the root as a wrapper
        let root = store.root();

        // Access fields on the binding wrapper - no imports needed
        let count = root.count();
        assert_eq!(count.get(), 10);

        count.set(20);
        assert_eq!(count.get(), 20);
    }

    #[test]
    fn test_wrapper_store_default() {
        // Test Default impl for wrapper
        let store = DeriveTestStateStore::default();
        let count = store.count();
        assert_eq!(count.get(), 0); // Default for i32
    }

    #[test]
    fn test_wrapper_store_clone() {
        // Test Clone impl for wrapper
        let store1 = DeriveTestStateStore::new(DeriveTestState {
            count: 42,
            ..Default::default()
        });

        let store2 = store1.clone();

        // Both point to the same data
        store1.count().set(100);
        assert_eq!(store2.count().get(), 100);
    }

    // ===== Multi-level Nested Tests =====

    #[derive(Lenses, Default, Clone, PartialEq)]
    struct Level1 {
        #[nested]
        level2: Level2,
        name: String,
    }

    #[derive(Lenses, Default, Clone, PartialEq)]
    struct Level2 {
        #[nested]
        level3: Level3,
        count: i32,
    }

    #[derive(Lenses, Default, Clone, PartialEq)]
    struct Level3 {
        value: f64,
        flag: bool,
    }

    #[test]
    fn test_multi_level_nested() {
        // Test 3 levels of nesting: Level1 -> Level2 -> Level3
        let store = Level1Store::new(Level1 {
            name: "Root".into(),
            level2: Level2 {
                count: 42,
                level3: Level3 {
                    value: 3.14,
                    flag: true,
                },
            },
        });

        // Direct field access
        assert_eq!(store.name().get(), "Root");

        // 2 levels deep
        assert_eq!(store.level2().count().get(), 42);

        // 3 levels deep - this is the key test!
        assert!((store.level2().level3().value().get() - 3.14).abs() < 0.001);
        assert!(store.level2().level3().flag().get());

        // Modify at each level
        store.name().set("Updated".into());
        store.level2().count().set(100);
        store.level2().level3().value().set(2.71);
        store.level2().level3().flag().set(false);

        // Verify changes
        assert_eq!(store.name().get(), "Updated");
        assert_eq!(store.level2().count().get(), 100);
        assert!((store.level2().level3().value().get() - 2.71).abs() < 0.001);
        assert!(!store.level2().level3().flag().get());
    }

    // ===== Vec Nested Tests =====

    #[derive(Lenses, Default, Clone, PartialEq)]
    struct VecItem {
        name: String,
        value: i32,
    }

    #[derive(Lenses, Default, Clone, PartialEq)]
    struct VecContainer {
        #[nested]
        items: Vec<VecItem>,
    }

    #[test]
    fn test_vec_nested_basic() {
        // Test that #[nested] on Vec<T> returns a wrapper that gives wrapped elements
        let store = VecContainerStore::new(VecContainer {
            items: vec![
                VecItem { name: "First".into(), value: 1 },
                VecItem { name: "Second".into(), value: 2 },
            ],
        });

        // store.items() returns a Vec wrapper
        let items = store.items();
        assert_eq!(items.len(), 2);

        // items.index(0) returns VecItemBinding, not raw Binding
        // So we can use .name() and .value() methods
        let first = items.index(0);
        assert_eq!(first.name().get(), "First");
        assert_eq!(first.value().get(), 1);

        let second = items.index(1);
        assert_eq!(second.name().get(), "Second");
        assert_eq!(second.value().get(), 2);
    }

    #[test]
    fn test_vec_nested_mutation() {
        let store = VecContainerStore::new(VecContainer {
            items: vec![
                VecItem { name: "Item".into(), value: 10 },
            ],
        });

        let items = store.items();
        let first = items.index(0);

        // Mutate through wrapper methods
        first.name().set("Updated".into());
        first.value().set(42);

        assert_eq!(first.name().get(), "Updated");
        assert_eq!(first.value().get(), 42);
    }

    #[test]
    fn test_vec_nested_push_pop() {
        let store = VecContainerStore::new(VecContainer::default());

        let items = store.items();
        assert!(items.is_empty());

        // Push items
        items.push(VecItem { name: "A".into(), value: 1 });
        items.push(VecItem { name: "B".into(), value: 2 });
        assert_eq!(items.len(), 2);

        // Access pushed items via wrapper
        assert_eq!(items.index(0).name().get(), "A");
        assert_eq!(items.index(1).name().get(), "B");

        // Pop
        let popped = items.pop();
        assert!(popped.is_some());
        assert_eq!(items.len(), 1);

        // Clear
        items.clear();
        assert!(items.is_empty());
    }

    #[test]
    fn test_vec_nested_with_update() {
        let store = VecContainerStore::new(VecContainer {
            items: vec![
                VecItem { name: "X".into(), value: 100 },
            ],
        });

        let items = store.items();

        // Use with() to read
        let name = items.with(|v| v[0].name.clone());
        assert_eq!(name, "X");

        // Use update() to modify the whole vec
        items.update(|v| {
            v.push(VecItem { name: "Y".into(), value: 200 });
        });
        assert_eq!(items.len(), 2);
        assert_eq!(items.index(1).name().get(), "Y");
    }

    // Test Vec<T> where T has #[nested] fields (nested inside nested)
    #[derive(Lenses, Default, Clone, PartialEq)]
    struct PointItem {
        #[nested]
        coords: Coords,
        label: String,
    }

    #[derive(Lenses, Default, Clone, PartialEq)]
    struct Coords {
        x: f64,
        y: f64,
    }

    #[derive(Lenses, Default, Clone, PartialEq)]
    struct PointsContainer {
        #[nested]
        points: Vec<PointItem>,
    }

    #[test]
    fn test_vec_nested_deep() {
        // Test Vec<T> where T itself has #[nested] fields
        let store = PointsContainerStore::new(PointsContainer {
            points: vec![
                PointItem {
                    coords: Coords { x: 1.0, y: 2.0 },
                    label: "Point 1".into(),
                },
            ],
        });

        let points = store.points();
        let first = points.index(0);

        // Access nested field inside Vec element
        assert_eq!(first.label().get(), "Point 1");

        // Access deeply nested field - coords() returns CoordsBinding wrapper!
        assert!((first.coords().x().get() - 1.0).abs() < 0.001);
        assert!((first.coords().y().get() - 2.0).abs() < 0.001);

        // Modify deeply nested
        first.coords().x().set(10.0);
        first.coords().y().set(20.0);

        assert!((first.coords().x().get() - 10.0).abs() < 0.001);
        assert!((first.coords().y().get() - 20.0).abs() < 0.001);
    }

    // ===== HashMap Nested Tests =====

    // Note: HashMap is already imported at line 303

    #[derive(Lenses, Default, Clone, PartialEq)]
    struct MapEntry {
        name: String,
        score: i32,
    }

    #[derive(Lenses, Default, Clone, PartialEq)]
    struct MapContainer {
        #[nested]
        entries: HashMap<u32, MapEntry>,
    }

    #[test]
    fn test_hashmap_nested_basic() {
        // Test that #[nested] on HashMap<K, V> returns a wrapper that gives wrapped values
        let mut initial_entries = HashMap::new();
        initial_entries.insert(1, MapEntry { name: "Alice".into(), score: 100 });
        initial_entries.insert(2, MapEntry { name: "Bob".into(), score: 85 });

        let store = MapContainerStore::new(MapContainer {
            entries: initial_entries,
        });

        // store.entries() returns a HashMap wrapper
        let entries = store.entries();
        assert_eq!(entries.len(), 2);

        // entries.key(1) returns MapEntryBinding, not raw Binding
        // So we can use .name() and .score() methods
        let alice = entries.key(1);
        assert_eq!(alice.name().get(), "Alice");
        assert_eq!(alice.score().get(), 100);

        let bob = entries.key(2);
        assert_eq!(bob.name().get(), "Bob");
        assert_eq!(bob.score().get(), 85);
    }

    #[test]
    fn test_hashmap_nested_mutation() {
        let mut initial_entries = HashMap::new();
        initial_entries.insert(1, MapEntry { name: "Test".into(), score: 50 });

        let store = MapContainerStore::new(MapContainer {
            entries: initial_entries,
        });

        let entries = store.entries();
        let entry = entries.key(1);

        // Mutate through wrapper methods
        entry.name().set("Updated".into());
        entry.score().set(99);

        assert_eq!(entry.name().get(), "Updated");
        assert_eq!(entry.score().get(), 99);
    }

    #[test]
    fn test_hashmap_nested_insert_remove() {
        let store = MapContainerStore::new(MapContainer::default());

        let entries = store.entries();
        assert!(entries.is_empty());

        // Insert entries
        entries.insert(10, MapEntry { name: "Entry A".into(), score: 10 });
        entries.insert(20, MapEntry { name: "Entry B".into(), score: 20 });
        assert_eq!(entries.len(), 2);
        assert!(entries.contains_key(&10));
        assert!(entries.contains_key(&20));

        // Access inserted entries via wrapper
        assert_eq!(entries.key(10).name().get(), "Entry A");
        assert_eq!(entries.key(20).name().get(), "Entry B");

        // Remove
        let removed = entries.remove(&10);
        assert!(removed.is_some());
        assert_eq!(entries.len(), 1);
        assert!(!entries.contains_key(&10));

        // Clear
        entries.clear();
        assert!(entries.is_empty());
    }

    #[test]
    fn test_hashmap_nested_with_update() {
        let mut initial_entries = HashMap::new();
        initial_entries.insert(1, MapEntry { name: "X".into(), score: 100 });

        let store = MapContainerStore::new(MapContainer {
            entries: initial_entries,
        });

        let entries = store.entries();

        // Use with() to read
        let name = entries.with(|m| m.get(&1).unwrap().name.clone());
        assert_eq!(name, "X");

        // Use update() to modify the whole map
        entries.update(|m| {
            m.insert(2, MapEntry { name: "Y".into(), score: 200 });
        });
        assert_eq!(entries.len(), 2);
        assert_eq!(entries.key(2).name().get(), "Y");
    }

    #[test]
    fn test_hashmap_nested_get_value() {
        let mut initial_entries = HashMap::new();
        initial_entries.insert(1, MapEntry { name: "Alice".into(), score: 100 });

        let store = MapContainerStore::new(MapContainer {
            entries: initial_entries,
        });

        let entries = store.entries();

        // get_value returns cloned value
        let entry = entries.get_value(&1);
        assert!(entry.is_some());
        let entry = entry.unwrap();
        assert_eq!(entry.name, "Alice");
        assert_eq!(entry.score, 100);

        // Non-existent key
        let missing = entries.get_value(&999);
        assert!(missing.is_none());
    }

    // Test HashMap<K, V> where V has #[nested] fields (nested inside nested)
    #[derive(Lenses, Default, Clone, PartialEq)]
    struct PlayerData {
        #[nested]
        stats: PlayerStats,
        level: i32,
    }

    #[derive(Lenses, Default, Clone, PartialEq)]
    struct PlayerStats {
        health: i32,
        mana: i32,
    }

    #[derive(Lenses, Default, Clone, PartialEq)]
    struct PlayersContainer {
        #[nested]
        players: HashMap<u32, PlayerData>,
    }

    #[test]
    fn test_hashmap_nested_deep() {
        // Test HashMap<K, V> where V itself has #[nested] fields
        let mut initial_players = HashMap::new();
        initial_players.insert(1, PlayerData {
            stats: PlayerStats { health: 100, mana: 50 },
            level: 10,
        });

        let store = PlayersContainerStore::new(PlayersContainer {
            players: initial_players,
        });

        let players = store.players();
        let player1 = players.key(1);

        // Access nested field inside HashMap value
        assert_eq!(player1.level().get(), 10);

        // Access deeply nested field - stats() returns PlayerStatsBinding wrapper!
        assert_eq!(player1.stats().health().get(), 100);
        assert_eq!(player1.stats().mana().get(), 50);

        // Modify deeply nested
        player1.stats().health().set(80);
        player1.stats().mana().set(75);

        assert_eq!(player1.stats().health().get(), 80);
        assert_eq!(player1.stats().mana().get(), 75);
    }

    // ===== Reconcile Tests =====

    // ===== Path Normalization Tests =====

    #[test]
    fn test_path_normalization_store_vs_root() {
        // Test that store.count() and store.root().count() share the same PathId
        // This is achieved by stripping IdentityLens from ComposedLens paths
        let store = DeriveTestStateStore::new(DeriveTestState {
            count: 0,
            ..Default::default()
        });

        // Get bindings via different paths
        let count_direct = store.count();
        let count_via_root = store.root().count();

        // They should have the same PathId
        assert_eq!(count_direct.path_id(), count_via_root.path_id());
    }

    #[test]
    fn test_path_normalization_effect_subscription() {
        // Test that an effect subscribed to store.count() sees updates from store.root().count().set()
        let store = DeriveTestStateStore::new(DeriveTestState {
            count: 0,
            ..Default::default()
        });

        let run_count = Rc::new(Cell::new(0));

        // Subscribe via direct path
        {
            let count = store.count();
            let run_count = run_count.clone();
            Effect::new(move |_| {
                let _ = count.get();
                run_count.set(run_count.get() + 1);
            });
        }

        // Initial run
        assert_eq!(run_count.get(), 1);

        // Update via root path - should trigger the effect subscribed to direct path
        store.root().count().set(42);
        Runtime::drain_pending_work();

        // Effect should have run again because paths are normalized
        assert_eq!(run_count.get(), 2);
        assert_eq!(store.count().get(), 42);
    }

    #[test]
    fn test_path_normalization_reverse_subscription() {
        // Test the reverse: subscribe to store.root().count(), update via store.count()
        let store = DeriveTestStateStore::new(DeriveTestState {
            count: 0,
            ..Default::default()
        });

        let run_count = Rc::new(Cell::new(0));

        // Subscribe via root path
        {
            let count = store.root().count();
            let run_count = run_count.clone();
            Effect::new(move |_| {
                let _ = count.get();
                run_count.set(run_count.get() + 1);
            });
        }

        // Initial run
        assert_eq!(run_count.get(), 1);

        // Update via direct path - should trigger the effect subscribed to root path
        store.count().set(100);
        Runtime::drain_pending_work();

        // Effect should have run again
        assert_eq!(run_count.get(), 2);
        assert_eq!(store.root().count().get(), 100);
    }

    #[test]
    fn test_path_normalization_nested() {
        // Test path normalization with nested bindings
        let store = DeriveTestStateStore::new(DeriveTestState {
            nested: DeriveTestNested {
                value: 0.0,
                ..Default::default()
            },
            ..Default::default()
        });

        let run_count = Rc::new(Cell::new(0));

        // Subscribe via direct nested path: store.nested().value()
        {
            let value = store.nested().value();
            let run_count = run_count.clone();
            Effect::new(move |_| {
                let _ = value.get();
                run_count.set(run_count.get() + 1);
            });
        }

        assert_eq!(run_count.get(), 1);

        // Update via root nested path: store.root().nested().value()
        store.root().nested().value().set(3.14);
        Runtime::drain_pending_work();

        // Effect should have run
        assert_eq!(run_count.get(), 2);
        assert!((store.nested().value().get() - 3.14).abs() < 0.001);
    }

    // ===== Reconcile Tests =====

    #[derive(Lenses, Default, Clone, PartialEq)]
    struct ReconcileTest {
        count: i32,
        name: String,
        flag: bool,
    }

    #[test]
    fn test_reconcile_basic() {
        let store = ReconcileTestStore::new(ReconcileTest {
            count: 10,
            name: "Original".into(),
            flag: false,
        });

        // Track which fields are updated
        let count_updates = Rc::new(Cell::new(0));
        let name_updates = Rc::new(Cell::new(0));
        let flag_updates = Rc::new(Cell::new(0));

        // Create effects to track updates
        // IMPORTANT: Use root().field_name() to get the same lens path that reconcile uses
        {
            let count = store.root().count();
            let count_updates = count_updates.clone();
            Effect::new(move |_| {
                count.get();
                count_updates.set(count_updates.get() + 1);
            });
        }
        {
            let name = store.root().name();
            let name_updates = name_updates.clone();
            Effect::new(move |_| {
                name.get();
                name_updates.set(name_updates.get() + 1);
            });
        }
        {
            let flag = store.root().flag();
            let flag_updates = flag_updates.clone();
            Effect::new(move |_| {
                flag.get();
                flag_updates.set(flag_updates.get() + 1);
            });
        }

        // Initial run counts
        assert_eq!(count_updates.get(), 1);
        assert_eq!(name_updates.get(), 1);
        assert_eq!(flag_updates.get(), 1);

        // Reconcile with only count changed
        store.root().reconcile(&ReconcileTest {
            count: 20,          // Changed
            name: "Original".into(), // Same
            flag: false,        // Same
        });
        Runtime::drain_pending_work();

        // Only count effect should have re-run
        assert_eq!(count_updates.get(), 2);
        assert_eq!(name_updates.get(), 1); // No change
        assert_eq!(flag_updates.get(), 1); // No change

        // Verify value
        assert_eq!(store.root().count().get(), 20);
        assert_eq!(store.root().name().get(), "Original");

        // Reconcile with multiple changes
        store.root().reconcile(&ReconcileTest {
            count: 20,          // Same
            name: "Updated".into(), // Changed
            flag: true,         // Changed
        });
        Runtime::drain_pending_work();

        assert_eq!(count_updates.get(), 2); // No change
        assert_eq!(name_updates.get(), 2);  // Changed
        assert_eq!(flag_updates.get(), 2);  // Changed

        // Verify values
        assert_eq!(store.root().name().get(), "Updated");
        assert!(store.root().flag().get());
    }

    #[test]
    fn test_reconcile_no_change() {
        let store = ReconcileTestStore::new(ReconcileTest {
            count: 42,
            name: "Test".into(),
            flag: true,
        });

        let update_count = Rc::new(Cell::new(0));

        {
            let root = store.root();
            let update_count = update_count.clone();
            Effect::new(move |_| {
                root.with(|_| {}); // Track root
                update_count.set(update_count.get() + 1);
            });
        }

        assert_eq!(update_count.get(), 1);

        // Reconcile with same values - no effects should trigger
        store.root().reconcile(&ReconcileTest {
            count: 42,
            name: "Test".into(),
            flag: true,
        });
        Runtime::drain_pending_work();

        // Note: root effect doesn't re-run because we only update individual fields
        // But field effects would not re-run either (tested above)
        assert_eq!(store.root().count().get(), 42);
        assert_eq!(store.root().name().get(), "Test");
        assert!(store.root().flag().get());
    }

    #[derive(Lenses, Default, Clone, PartialEq)]
    struct ReconcileOuter {
        #[nested]
        coords: ReconcileCoords,
        value: i32,
    }

    #[derive(Lenses, Default, Clone, PartialEq)]
    struct ReconcileCoords {
        x: f64,
        y: f64,
    }

    #[test]
    fn test_reconcile_nested() {
        let store = ReconcileOuterStore::new(ReconcileOuter {
            coords: ReconcileCoords { x: 1.0, y: 2.0 },
            value: 100,
        });

        let x_updates = Rc::new(Cell::new(0));
        let y_updates = Rc::new(Cell::new(0));
        let value_updates = Rc::new(Cell::new(0));

        // Use root().coords().x() to get same lens path that reconcile uses
        {
            let x = store.root().coords().x();
            let x_updates = x_updates.clone();
            Effect::new(move |_| {
                x.get();
                x_updates.set(x_updates.get() + 1);
            });
        }
        {
            let y = store.root().coords().y();
            let y_updates = y_updates.clone();
            Effect::new(move |_| {
                y.get();
                y_updates.set(y_updates.get() + 1);
            });
        }
        {
            let value = store.root().value();
            let value_updates = value_updates.clone();
            Effect::new(move |_| {
                value.get();
                value_updates.set(value_updates.get() + 1);
            });
        }

        assert_eq!(x_updates.get(), 1);
        assert_eq!(y_updates.get(), 1);
        assert_eq!(value_updates.get(), 1);

        // Reconcile with only coords.x changed
        store.root().reconcile(&ReconcileOuter {
            coords: ReconcileCoords { x: 10.0, y: 2.0 }, // Only x changed
            value: 100,
        });
        Runtime::drain_pending_work();

        assert_eq!(x_updates.get(), 2); // Changed
        assert_eq!(y_updates.get(), 1); // No change
        assert_eq!(value_updates.get(), 1); // No change

        assert!((store.root().coords().x().get() - 10.0).abs() < 0.001);
        assert!((store.root().coords().y().get() - 2.0).abs() < 0.001);
    }

    #[derive(Lenses, Default, Clone, PartialEq)]
    struct ReconcileWithVec {
        #[nested]
        entries: Vec<VecItem>,
        count: i32,
    }

    #[test]
    fn test_reconcile_with_vec() {
        let store = ReconcileWithVecStore::new(ReconcileWithVec {
            entries: vec![
                VecItem { name: "A".into(), value: 1 },
                VecItem { name: "B".into(), value: 2 },
            ],
            count: 10,
        });

        let entries_updates = Rc::new(Cell::new(0));
        let count_updates = Rc::new(Cell::new(0));

        // Use root().field_name() to get same lens path that reconcile uses
        {
            let entries = store.root().entries();
            let entries_updates = entries_updates.clone();
            Effect::new(move |_| {
                entries.with(|_| {});
                entries_updates.set(entries_updates.get() + 1);
            });
        }
        {
            let count = store.root().count();
            let count_updates = count_updates.clone();
            Effect::new(move |_| {
                count.get();
                count_updates.set(count_updates.get() + 1);
            });
        }

        assert_eq!(entries_updates.get(), 1);
        assert_eq!(count_updates.get(), 1);

        // Reconcile with same vec - no update
        store.root().reconcile(&ReconcileWithVec {
            entries: vec![
                VecItem { name: "A".into(), value: 1 },
                VecItem { name: "B".into(), value: 2 },
            ],
            count: 10,
        });
        Runtime::drain_pending_work();

        assert_eq!(entries_updates.get(), 1); // No change
        assert_eq!(count_updates.get(), 1); // No change

        // Reconcile with different vec
        store.root().reconcile(&ReconcileWithVec {
            entries: vec![
                VecItem { name: "A".into(), value: 1 },
                VecItem { name: "C".into(), value: 3 }, // Changed
            ],
            count: 10,
        });
        Runtime::drain_pending_work();

        assert_eq!(entries_updates.get(), 2); // Changed
        assert_eq!(count_updates.get(), 1); // No change

        assert_eq!(store.root().entries().index(1).name().get(), "C");
    }

    #[test]
    fn test_store_reconcile_shortcut() {
        // Test that store.reconcile() works the same as store.root().reconcile()
        let store = ReconcileTestStore::new(ReconcileTest {
            count: 10,
            name: "Original".into(),
            flag: false,
        });

        let count_updates = Rc::new(Cell::new(0));

        {
            let count = store.count();
            let count_updates = count_updates.clone();
            Effect::new(move |_| {
                count.get();
                count_updates.set(count_updates.get() + 1);
            });
        }

        assert_eq!(count_updates.get(), 1);

        // Use store.reconcile() directly (shortcut for store.root().reconcile())
        store.reconcile(&ReconcileTest {
            count: 42,  // Changed
            name: "Original".into(),
            flag: false,
        });
        Runtime::drain_pending_work();

        // Effect should have run because count changed
        assert_eq!(count_updates.get(), 2);
        assert_eq!(store.count().get(), 42);
    }

    // ===== DynBinding Tests =====

    #[test]
    fn test_dyn_binding_basic() {
        use crate::DynBinding;

        let store = DeriveTestStateStore::new(DeriveTestState {
            count: 10,
            name: "Test".into(),
            ..Default::default()
        });

        // Convert to DynBinding
        let count_dyn: DynBinding<i32> = store.count().into_dyn();
        let name_dyn: DynBinding<String> = store.name().into_dyn();

        // Test get
        assert_eq!(count_dyn.get(), 10);
        assert_eq!(name_dyn.get(), "Test");

        // Test set
        count_dyn.set(20);
        assert_eq!(count_dyn.get(), 20);

        // Test update
        count_dyn.update(|c| *c += 5);
        assert_eq!(count_dyn.get(), 25);

        // Test try_update
        let old_value = count_dyn.try_update(|c| {
            let old = *c;
            *c = 100;
            old
        });
        assert_eq!(old_value, 25);
        assert_eq!(count_dyn.get(), 100);
    }

    #[test]
    fn test_dyn_binding_clone() {
        use crate::DynBinding;

        let store = DeriveTestStateStore::new(DeriveTestState::default());

        let count_dyn: DynBinding<i32> = store.count().into_dyn();
        let count_dyn2 = count_dyn.clone();

        count_dyn.set(42);
        assert_eq!(count_dyn2.get(), 42);
    }

    #[test]
    fn test_dyn_binding_reactive_traits() {
        use crate::DynBinding;
        use floem_reactive::{SignalGet, SignalUpdate, SignalWith};

        let store = DeriveTestStateStore::new(DeriveTestState {
            count: 5,
            ..Default::default()
        });

        let count_dyn: DynBinding<i32> = store.count().into_dyn();

        // Test SignalGet
        assert_eq!(SignalGet::get(&count_dyn), 5);

        // Test SignalUpdate
        SignalUpdate::set(&count_dyn, 15);
        assert_eq!(SignalGet::get(&count_dyn), 15);

        // Test SignalWith
        let doubled = SignalWith::with(&count_dyn, |v| *v * 2);
        assert_eq!(doubled, 30);
    }

    #[test]
    fn test_dyn_binding_as_function_param() {
        use crate::DynBinding;

        // This is the main use case: passing bindings to functions without complex generics
        fn increment_counter(counter: &DynBinding<i32>) {
            counter.update(|c| *c += 1);
        }

        fn get_doubled(counter: &DynBinding<i32>) -> i32 {
            counter.get() * 2
        }

        let store = DeriveTestStateStore::new(DeriveTestState {
            count: 10,
            ..Default::default()
        });

        let count_dyn = store.count().into_dyn();

        increment_counter(&count_dyn);
        assert_eq!(count_dyn.get(), 11);

        assert_eq!(get_doubled(&count_dyn), 22);
    }

    #[test]
    fn test_dyn_binding_with_effect() {
        use crate::DynBinding;

        let store = DeriveTestStateStore::new(DeriveTestState::default());
        let run_count = Rc::new(Cell::new(0));

        let count_dyn: DynBinding<i32> = store.count().into_dyn();

        {
            let count_dyn = count_dyn.clone();
            let run_count = run_count.clone();
            Effect::new(move |_| {
                count_dyn.get();
                run_count.set(run_count.get() + 1);
            });
        }

        assert_eq!(run_count.get(), 1);

        // Update through DynBinding should trigger the effect
        count_dyn.set(42);
        Runtime::drain_pending_work();
        assert_eq!(run_count.get(), 2);
    }

    // ===== Keyed Vec Reconciliation Tests =====

    #[derive(Lenses, Default, Clone, PartialEq)]
    struct KeyedItem {
        id: u64,
        text: String,
        done: bool,
    }

    #[derive(Lenses, Default, Clone, PartialEq)]
    struct KeyedVecContainer {
        #[nested(key = id)]  // Type is inferred from KeyedItem::id
        todos: Vec<KeyedItem>,
        count: i32,
    }

    #[test]
    fn test_keyed_reconcile_same_structure_content_change() {
        // Test that when structure (keys in same order) matches:
        // - The Vec itself is NOT replaced (structure preserved)
        // - Individual items are reconciled (only changed fields update)
        let store = KeyedVecContainerStore::new(KeyedVecContainer {
            todos: vec![
                KeyedItem { id: 1, text: "First".into(), done: false },
                KeyedItem { id: 2, text: "Second".into(), done: false },
            ],
            count: 0,
        });

        let todos_updates = Rc::new(Cell::new(0));

        // Subscribe to the whole items array
        {
            let items = store.root().todos();
            let todos_updates = todos_updates.clone();
            Effect::new(move |_| {
                items.with(|_| {});
                todos_updates.set(todos_updates.get() + 1);
            });
        }

        // Initial effect run
        assert_eq!(todos_updates.get(), 1);

        // Reconcile with same structure but changed content for item 1 only
        store.root().reconcile(&KeyedVecContainer {
            todos: vec![
                KeyedItem { id: 1, text: "First".into(), done: false }, // Unchanged
                KeyedItem { id: 2, text: "Modified".into(), done: false }, // Changed text
            ],
            count: 0,
        });
        Runtime::drain_pending_work();

        // Key guarantee: Vec itself should NOT be replaced (same structure)
        // So the Vec-level effect should NOT run again
        assert_eq!(todos_updates.get(), 1);

        // Verify the actual values are correct
        assert_eq!(store.todos().index(0).text().get(), "First"); // Unchanged
        assert_eq!(store.todos().index(1).text().get(), "Modified"); // Updated
    }

    #[test]
    fn test_keyed_reconcile_structural_change_reorder() {
        // Test that reordering items (structure change) replaces the whole Vec
        let store = KeyedVecContainerStore::new(KeyedVecContainer {
            todos: vec![
                KeyedItem { id: 1, text: "First".into(), done: false },
                KeyedItem { id: 2, text: "Second".into(), done: false },
            ],
            count: 0,
        });

        let todos_updates = Rc::new(Cell::new(0));

        {
            let items = store.root().todos();
            let todos_updates = todos_updates.clone();
            Effect::new(move |_| {
                items.with(|_| {});
                todos_updates.set(todos_updates.get() + 1);
            });
        }

        assert_eq!(todos_updates.get(), 1);

        // Reconcile with reordered items (structure change)
        store.root().reconcile(&KeyedVecContainer {
            todos: vec![
                KeyedItem { id: 2, text: "Second".into(), done: false }, // Was at index 1
                KeyedItem { id: 1, text: "First".into(), done: false }, // Was at index 0
            ],
            count: 0,
        });
        Runtime::drain_pending_work();

        // Items array should be updated (structural change)
        assert_eq!(todos_updates.get(), 2);

        // Verify the new order
        assert_eq!(store.todos().index(0).id().get(), 2);
        assert_eq!(store.todos().index(1).id().get(), 1);
    }

    #[test]
    fn test_keyed_reconcile_structural_change_add_remove() {
        // Test that adding/removing items (structure change) replaces the whole Vec
        let store = KeyedVecContainerStore::new(KeyedVecContainer {
            todos: vec![
                KeyedItem { id: 1, text: "First".into(), done: false },
                KeyedItem { id: 2, text: "Second".into(), done: false },
            ],
            count: 0,
        });

        let todos_updates = Rc::new(Cell::new(0));

        {
            let items = store.root().todos();
            let todos_updates = todos_updates.clone();
            Effect::new(move |_| {
                items.with(|_| {});
                todos_updates.set(todos_updates.get() + 1);
            });
        }

        assert_eq!(todos_updates.get(), 1);

        // Reconcile with an added item (structure change)
        store.root().reconcile(&KeyedVecContainer {
            todos: vec![
                KeyedItem { id: 1, text: "First".into(), done: false },
                KeyedItem { id: 2, text: "Second".into(), done: false },
                KeyedItem { id: 3, text: "Third".into(), done: false }, // New item
            ],
            count: 0,
        });
        Runtime::drain_pending_work();

        // Items array should be updated (structural change)
        assert_eq!(todos_updates.get(), 2);
        assert_eq!(store.todos().len(), 3);

        // Reconcile with a removed item (structure change)
        store.root().reconcile(&KeyedVecContainer {
            todos: vec![
                KeyedItem { id: 1, text: "First".into(), done: false },
            ],
            count: 0,
        });
        Runtime::drain_pending_work();

        // Items array should be updated again
        assert_eq!(todos_updates.get(), 3);
        assert_eq!(store.todos().len(), 1);
    }

    #[test]
    fn test_keyed_reconcile_no_change() {
        // Test that no updates happen when nothing changed
        let store = KeyedVecContainerStore::new(KeyedVecContainer {
            todos: vec![
                KeyedItem { id: 1, text: "First".into(), done: false },
                KeyedItem { id: 2, text: "Second".into(), done: true },
            ],
            count: 5,
        });

        let todos_updates = Rc::new(Cell::new(0));
        let item0_text_updates = Rc::new(Cell::new(0));
        let count_updates = Rc::new(Cell::new(0));

        {
            let items = store.root().todos();
            let todos_updates = todos_updates.clone();
            Effect::new(move |_| {
                items.with(|_| {});
                todos_updates.set(todos_updates.get() + 1);
            });
        }
        {
            let text = store.root().todos().index(0).text();
            let item0_text_updates = item0_text_updates.clone();
            Effect::new(move |_| {
                text.get();
                item0_text_updates.set(item0_text_updates.get() + 1);
            });
        }
        {
            let count = store.root().count();
            let count_updates = count_updates.clone();
            Effect::new(move |_| {
                count.get();
                count_updates.set(count_updates.get() + 1);
            });
        }

        assert_eq!(todos_updates.get(), 1);
        assert_eq!(item0_text_updates.get(), 1);
        assert_eq!(count_updates.get(), 1);

        // Reconcile with identical data
        store.root().reconcile(&KeyedVecContainer {
            todos: vec![
                KeyedItem { id: 1, text: "First".into(), done: false },
                KeyedItem { id: 2, text: "Second".into(), done: true },
            ],
            count: 5,
        });
        Runtime::drain_pending_work();

        // Nothing should be updated
        assert_eq!(todos_updates.get(), 1);
        assert_eq!(item0_text_updates.get(), 1);
        assert_eq!(count_updates.get(), 1);
    }

    #[test]
    fn test_per_index_effect_isolation() {
        // Test that effects subscribed to different indices are isolated.
        // Updating todos[1].text should NOT trigger effects on todos[0].text.
        let store = KeyedVecContainerStore::new(KeyedVecContainer {
            todos: vec![
                KeyedItem { id: 1, text: "First".into(), done: false },
                KeyedItem { id: 2, text: "Second".into(), done: false },
            ],
            count: 0,
        });

        let item0_text_updates = Rc::new(Cell::new(0));
        let item1_text_updates = Rc::new(Cell::new(0));

        // Subscribe to item 0's text
        {
            let text = store.todos().index(0).text();
            let item0_text_updates = item0_text_updates.clone();
            Effect::new(move |_| {
                text.get();
                item0_text_updates.set(item0_text_updates.get() + 1);
            });
        }

        // Subscribe to item 1's text
        {
            let text = store.todos().index(1).text();
            let item1_text_updates = item1_text_updates.clone();
            Effect::new(move |_| {
                text.get();
                item1_text_updates.set(item1_text_updates.get() + 1);
            });
        }

        // Initial effect runs
        assert_eq!(item0_text_updates.get(), 1);
        assert_eq!(item1_text_updates.get(), 1);

        // Update only item 1's text
        store.todos().index(1).text().set("Modified".into());
        Runtime::drain_pending_work();

        // Only item 1's effect should run (per-index isolation!)
        assert_eq!(item0_text_updates.get(), 1); // Unchanged
        assert_eq!(item1_text_updates.get(), 2); // Updated

        // Update only item 0's text
        store.todos().index(0).text().set("Also Modified".into());
        Runtime::drain_pending_work();

        // Only item 0's effect should run
        assert_eq!(item0_text_updates.get(), 2); // Updated
        assert_eq!(item1_text_updates.get(), 2); // Unchanged
    }

    #[test]
    fn test_per_key_effect_isolation() {
        // Test that effects subscribed to different HashMap keys are isolated.
        // Uses the existing MapContainer and MapEntry types defined in this module.
        let mut initial_entries = HashMap::new();
        initial_entries.insert(1, MapEntry { name: "One".into(), score: 10 });
        initial_entries.insert(2, MapEntry { name: "Two".into(), score: 20 });

        let store = MapContainerStore::new(MapContainer { entries: initial_entries });

        let key1_score_updates = Rc::new(Cell::new(0));
        let key2_score_updates = Rc::new(Cell::new(0));

        // Subscribe to key 1's score
        {
            let score = store.entries().key(1).score();
            let key1_score_updates = key1_score_updates.clone();
            Effect::new(move |_| {
                score.get();
                key1_score_updates.set(key1_score_updates.get() + 1);
            });
        }

        // Subscribe to key 2's score
        {
            let score = store.entries().key(2).score();
            let key2_score_updates = key2_score_updates.clone();
            Effect::new(move |_| {
                score.get();
                key2_score_updates.set(key2_score_updates.get() + 1);
            });
        }

        // Initial effect runs
        assert_eq!(key1_score_updates.get(), 1);
        assert_eq!(key2_score_updates.get(), 1);

        // Update only key 2's score
        store.entries().key(2).score().set(200);
        Runtime::drain_pending_work();

        // Only key 2's effect should run (per-key isolation!)
        assert_eq!(key1_score_updates.get(), 1); // Unchanged
        assert_eq!(key2_score_updates.get(), 2); // Updated

        // Update only key 1's score
        store.entries().key(1).score().set(100);
        Runtime::drain_pending_work();

        // Only key 1's effect should run
        assert_eq!(key1_score_updates.get(), 2); // Updated
        assert_eq!(key2_score_updates.get(), 2); // Unchanged
    }

    // ===== Identity-Based Vec Access Tests (by_key) =====

    #[test]
    fn test_by_id_basic_access() {
        // Test that by_id can access items by their key
        let store = KeyedVecContainerStore::new(KeyedVecContainer {
            todos: vec![
                KeyedItem { id: 1, text: "First".into(), done: false },
                KeyedItem { id: 2, text: "Second".into(), done: true },
                KeyedItem { id: 3, text: "Third".into(), done: false },
            ],
            count: 0,
        });

        // Access by id
        assert_eq!(store.todos().by_id(1).text().get(), "First");
        assert_eq!(store.todos().by_id(2).text().get(), "Second");
        assert_eq!(store.todos().by_id(3).text().get(), "Third");

        // Check done status
        assert!(!store.todos().by_id(1).done().get());
        assert!(store.todos().by_id(2).done().get());
    }

    #[test]
    fn test_by_id_update() {
        // Test that by_id can update items
        let store = KeyedVecContainerStore::new(KeyedVecContainer {
            todos: vec![
                KeyedItem { id: 1, text: "First".into(), done: false },
                KeyedItem { id: 2, text: "Second".into(), done: false },
            ],
            count: 0,
        });

        // Update via by_id
        store.todos().by_id(2).text().set("Updated".into());
        store.todos().by_id(1).done().set(true);

        // Verify updates
        assert_eq!(store.todos().by_id(2).text().get(), "Updated");
        assert!(store.todos().by_id(1).done().get());

        // Position should still be the same
        assert_eq!(store.todos().index(0).id().get(), 1);
        assert_eq!(store.todos().index(1).id().get(), 2);
    }

    #[test]
    fn test_by_id_stable_after_reorder() {
        // Test that by_id bindings stay on the same logical item after reorder
        let store = KeyedVecContainerStore::new(KeyedVecContainer {
            todos: vec![
                KeyedItem { id: 1, text: "First".into(), done: false },
                KeyedItem { id: 2, text: "Second".into(), done: false },
                KeyedItem { id: 3, text: "Third".into(), done: false },
            ],
            count: 0,
        });

        // Get a binding by id
        let item2_text = store.todos().by_id(2).text();
        assert_eq!(item2_text.get(), "Second");

        // Reorder the vec - move item 2 to the front
        store.todos().update(|v| {
            let item = v.remove(1); // Remove item with id=2
            v.insert(0, item); // Insert at front
        });

        // The by_id binding should still point to the same logical item
        // (even though its position changed from index 1 to index 0)
        assert_eq!(item2_text.get(), "Second");

        // Verify the new order
        assert_eq!(store.todos().index(0).id().get(), 2); // id=2 is now at index 0
        assert_eq!(store.todos().index(1).id().get(), 1); // id=1 is now at index 1
        assert_eq!(store.todos().index(2).id().get(), 3); // id=3 is still at index 2
    }

    #[test]
    fn test_by_id_effect_isolation() {
        // Test that effects on by_id bindings are isolated by key, not position
        let store = KeyedVecContainerStore::new(KeyedVecContainer {
            todos: vec![
                KeyedItem { id: 1, text: "First".into(), done: false },
                KeyedItem { id: 2, text: "Second".into(), done: false },
            ],
            count: 0,
        });

        let id1_text_updates = Rc::new(Cell::new(0));
        let id2_text_updates = Rc::new(Cell::new(0));

        // Subscribe to item 1's text via by_id
        {
            let text = store.todos().by_id(1).text();
            let id1_text_updates = id1_text_updates.clone();
            Effect::new(move |_| {
                text.get();
                id1_text_updates.set(id1_text_updates.get() + 1);
            });
        }

        // Subscribe to item 2's text via by_id
        {
            let text = store.todos().by_id(2).text();
            let id2_text_updates = id2_text_updates.clone();
            Effect::new(move |_| {
                text.get();
                id2_text_updates.set(id2_text_updates.get() + 1);
            });
        }

        // Initial effect runs
        assert_eq!(id1_text_updates.get(), 1);
        assert_eq!(id2_text_updates.get(), 1);

        // Update only item 2's text via by_id
        store.todos().by_id(2).text().set("Modified".into());
        Runtime::drain_pending_work();

        // Only item 2's effect should run (identity-based isolation!)
        assert_eq!(id1_text_updates.get(), 1); // Unchanged
        assert_eq!(id2_text_updates.get(), 2); // Updated

        // Update only item 1's text via by_id
        store.todos().by_id(1).text().set("Also Modified".into());
        Runtime::drain_pending_work();

        // Only item 1's effect should run
        assert_eq!(id1_text_updates.get(), 2); // Updated
        assert_eq!(id2_text_updates.get(), 2); // Unchanged
    }

    #[test]
    fn test_by_id_helper_methods() {
        // Test contains_key and remove_by_key helper methods
        let store = KeyedVecContainerStore::new(KeyedVecContainer {
            todos: vec![
                KeyedItem { id: 1, text: "First".into(), done: false },
                KeyedItem { id: 2, text: "Second".into(), done: false },
                KeyedItem { id: 3, text: "Third".into(), done: false },
            ],
            count: 0,
        });

        // Test contains_key
        assert!(store.todos().contains_key(&1));
        assert!(store.todos().contains_key(&2));
        assert!(store.todos().contains_key(&3));
        assert!(!store.todos().contains_key(&99));

        // Test remove_by_key
        let removed = store.todos().remove_by_key(&2);
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().text, "Second");

        // Verify removal
        assert!(!store.todos().contains_key(&2));
        assert_eq!(store.todos().len(), 2);

        // Remaining items
        assert!(store.todos().contains_key(&1));
        assert!(store.todos().contains_key(&3));
    }

    #[test]
    fn test_filtered_bindings() {
        // Test filtered_bindings helper method for dyn_stack integration
        let store = KeyedVecContainerStore::new(KeyedVecContainer {
            todos: vec![
                KeyedItem { id: 1, text: "First".into(), done: false },
                KeyedItem { id: 2, text: "Second".into(), done: true },
                KeyedItem { id: 3, text: "Third".into(), done: false },
            ],
            count: 0,
        });

        // Get all bindings (collect the iterator)
        let all: Vec<_> = store.todos().all_bindings().collect();
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].id().get(), 1);
        assert_eq!(all[1].id().get(), 2);
        assert_eq!(all[2].id().get(), 3);

        // Filter for only not-done items
        let active: Vec<_> = store.todos().filtered_bindings(|item| !item.done).collect();
        assert_eq!(active.len(), 2);
        assert_eq!(active[0].id().get(), 1);
        assert_eq!(active[1].id().get(), 3);

        // Filter for only done items
        let completed: Vec<_> = store.todos().filtered_bindings(|item| item.done).collect();
        assert_eq!(completed.len(), 1);
        assert_eq!(completed[0].id().get(), 2);

        // Bindings are reactive - update through them
        active[0].text().set("Updated First".into());
        assert_eq!(store.todos().by_id(1).text().get(), "Updated First");

        // Empty filter
        let none: Vec<_> = store.todos().filtered_bindings(|item| item.id > 100).collect();
        assert_eq!(none.len(), 0);
    }

    // ===== IndexMap Tests =====

    #[derive(Lenses, Default, Clone, PartialEq)]
    struct IndexMapTestItem {
        id: u64,
        name: String,
        done: bool,
    }

    #[derive(Lenses, Default, Clone, PartialEq)]
    struct IndexMapTestState {
        #[nested(key = id)]
        items: crate::IndexMap<u64, IndexMapTestItem>,
    }

    #[test]
    fn test_indexmap_nested_basic() {
        let mut items = crate::IndexMap::new();
        items.insert(1, IndexMapTestItem { id: 1, name: "First".into(), done: false });
        items.insert(2, IndexMapTestItem { id: 2, name: "Second".into(), done: true });

        let store = IndexMapTestStateStore::new(IndexMapTestState { items });

        // Check length
        assert_eq!(store.items().len(), 2);

        // Get by key (O(1) access)
        let item1 = store.items().get(1);
        assert_eq!(item1.name().get(), "First");
        assert_eq!(item1.done().get(), false);

        let item2 = store.items().get(2);
        assert_eq!(item2.name().get(), "Second");
        assert_eq!(item2.done().get(), true);

        // Update through binding
        item1.done().set(true);
        assert_eq!(store.items().get(1).done().get(), true);
    }

    #[test]
    fn test_indexmap_push() {
        let store = IndexMapTestStateStore::new(IndexMapTestState::default());

        // Push extracts key from value's id field
        store.items().push(IndexMapTestItem { id: 10, name: "Item 10".into(), done: false });
        store.items().push(IndexMapTestItem { id: 20, name: "Item 20".into(), done: true });

        assert_eq!(store.items().len(), 2);
        assert_eq!(store.items().get(10).name().get(), "Item 10");
        assert_eq!(store.items().get(20).name().get(), "Item 20");

        // Insertion order preserved
        let all: Vec<_> = store.items().all_bindings().collect();
        assert_eq!(all[0].id().get(), 10);
        assert_eq!(all[1].id().get(), 20);
    }

    #[test]
    fn test_indexmap_filtered_bindings() {
        let mut items = crate::IndexMap::new();
        items.insert(1, IndexMapTestItem { id: 1, name: "First".into(), done: false });
        items.insert(2, IndexMapTestItem { id: 2, name: "Second".into(), done: true });
        items.insert(3, IndexMapTestItem { id: 3, name: "Third".into(), done: false });

        let store = IndexMapTestStateStore::new(IndexMapTestState { items });

        // Filter for not-done items
        let active: Vec<_> = store.items().filtered_bindings(|item| !item.done).collect();
        assert_eq!(active.len(), 2);
        assert_eq!(active[0].id().get(), 1);
        assert_eq!(active[1].id().get(), 3);

        // Filter for done items
        let completed: Vec<_> = store.items().filtered_bindings(|item| item.done).collect();
        assert_eq!(completed.len(), 1);
        assert_eq!(completed[0].id().get(), 2);
    }

    #[test]
    fn test_indexmap_remove_by_key() {
        let mut items = crate::IndexMap::new();
        items.insert(1, IndexMapTestItem { id: 1, name: "First".into(), done: false });
        items.insert(2, IndexMapTestItem { id: 2, name: "Second".into(), done: true });

        let store = IndexMapTestStateStore::new(IndexMapTestState { items });

        // Remove by key (O(1))
        let removed = store.items().remove_by_key(&2);
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().name, "Second");

        assert_eq!(store.items().len(), 1);
        assert!(!store.items().contains_key(&2));
        assert!(store.items().contains_key(&1));
    }

    #[test]
    fn test_indexmap_o1_vs_vec() {
        // This test demonstrates O(1) access with IndexMap vs Vec's by_id which is O(N)
        // Both have the same API thanks to the derive macro, but IndexMap is faster

        // IndexMap version
        let mut items = crate::IndexMap::new();
        for i in 1..=100 {
            items.insert(i, IndexMapTestItem { id: i, name: format!("Item {}", i), done: false });
        }
        let store = IndexMapTestStateStore::new(IndexMapTestState { items });

        // Access last item - O(1) with IndexMap
        let item = store.items().get(100);
        assert_eq!(item.name().get(), "Item 100");

        // Update it
        item.done().set(true);
        assert!(store.items().get(100).done().get());
    }

    // ===== Lazy Cache Tests =====

    #[test]
    fn test_lazy_cache_basic_access() {
        // Test that lazy cache works for basic access - position is cached at creation time
        let store = KeyedVecContainerStore::new(KeyedVecContainer {
            todos: vec![
                KeyedItem { id: 1, text: "First".into(), done: false },
                KeyedItem { id: 2, text: "Second".into(), done: false },
                KeyedItem { id: 3, text: "Third".into(), done: false },
            ],
            count: 0,
        });

        // Get binding for item with id=2 (at position 1)
        // The cached_pos should be 1
        let item2 = store.todos().by_id(2);
        assert_eq!(item2.text().get(), "Second");

        // Access should work via cached position (O(1))
        assert_eq!(item2.id().get(), 2);
        assert!(!item2.done().get());

        // Update should also work
        item2.text().set("Modified".into());
        assert_eq!(item2.text().get(), "Modified");
    }

    #[test]
    fn test_lazy_cache_fallback_after_reorder() {
        // Test that lazy cache falls back to O(N) search when item moves
        let store = KeyedVecContainerStore::new(KeyedVecContainer {
            todos: vec![
                KeyedItem { id: 1, text: "First".into(), done: false },
                KeyedItem { id: 2, text: "Second".into(), done: false },
                KeyedItem { id: 3, text: "Third".into(), done: false },
            ],
            count: 0,
        });

        // Get binding for item with id=2 (at position 1)
        // This caches position 1
        let item2 = store.todos().by_id(2);
        assert_eq!(item2.text().get(), "Second");

        // Now reorder the Vec - move item with id=3 to the front
        store.todos().update(|v| {
            let item = v.remove(2); // Remove id=3 from position 2
            v.insert(0, item); // Insert at position 0
        });

        // New order: [id=3, id=1, id=2]
        // item2's cached_pos (1) now points to id=1, not id=2
        // But the fallback should still find the correct item

        // Verify the new order
        assert_eq!(store.todos().index(0).id().get(), 3);
        assert_eq!(store.todos().index(1).id().get(), 1);
        assert_eq!(store.todos().index(2).id().get(), 2);

        // item2 binding should STILL work - it falls back to O(N) search
        assert_eq!(item2.text().get(), "Second");
        assert_eq!(item2.id().get(), 2);

        // Update through the binding should still work
        item2.done().set(true);
        assert!(store.todos().by_id(2).done().get());
    }

    #[test]
    fn test_lazy_cache_filtered_bindings_positions() {
        // Test that filtered_bindings captures correct positions for each item
        let store = KeyedVecContainerStore::new(KeyedVecContainer {
            todos: vec![
                KeyedItem { id: 1, text: "First".into(), done: false },
                KeyedItem { id: 2, text: "Second".into(), done: true },
                KeyedItem { id: 3, text: "Third".into(), done: false },
                KeyedItem { id: 4, text: "Fourth".into(), done: true },
            ],
            count: 0,
        });

        // Get bindings for done items
        // These should have cached positions: id=2 at pos 1, id=4 at pos 3
        let done_items: Vec<_> = store.todos().filtered_bindings(|item| item.done).collect();
        assert_eq!(done_items.len(), 2);

        // Verify they point to correct items
        assert_eq!(done_items[0].id().get(), 2);
        assert_eq!(done_items[0].text().get(), "Second");
        assert_eq!(done_items[1].id().get(), 4);
        assert_eq!(done_items[1].text().get(), "Fourth");

        // Update through these bindings
        done_items[0].text().set("Second Modified".into());
        done_items[1].text().set("Fourth Modified".into());

        // Verify updates
        assert_eq!(store.todos().by_id(2).text().get(), "Second Modified");
        assert_eq!(store.todos().by_id(4).text().get(), "Fourth Modified");
    }

    #[test]
    fn test_lazy_cache_all_bindings() {
        // Test that all_bindings captures positions for each item
        let store = KeyedVecContainerStore::new(KeyedVecContainer {
            todos: vec![
                KeyedItem { id: 10, text: "Item 10".into(), done: false },
                KeyedItem { id: 20, text: "Item 20".into(), done: false },
                KeyedItem { id: 30, text: "Item 30".into(), done: false },
            ],
            count: 0,
        });

        let all: Vec<_> = store.todos().all_bindings().collect();
        assert_eq!(all.len(), 3);

        // Each binding should have correct cached position
        assert_eq!(all[0].id().get(), 10);
        assert_eq!(all[1].id().get(), 20);
        assert_eq!(all[2].id().get(), 30);

        // Updates should work through each binding
        for binding in &all {
            binding.done().set(true);
        }

        // Verify all items are now done
        assert!(store.todos().by_id(10).done().get());
        assert!(store.todos().by_id(20).done().get());
        assert!(store.todos().by_id(30).done().get());
    }

    #[test]
    fn test_lazy_cache_multiple_reorders() {
        // Test that bindings work correctly through multiple reorders
        let store = KeyedVecContainerStore::new(KeyedVecContainer {
            todos: vec![
                KeyedItem { id: 1, text: "A".into(), done: false },
                KeyedItem { id: 2, text: "B".into(), done: false },
                KeyedItem { id: 3, text: "C".into(), done: false },
            ],
            count: 0,
        });

        // Get binding for middle item
        let item2 = store.todos().by_id(2);
        assert_eq!(item2.text().get(), "B");

        // First reorder: reverse
        store.todos().update(|v| v.reverse());
        // New order: [3, 2, 1]
        assert_eq!(item2.text().get(), "B"); // Still works

        // Second reorder: sort by id
        store.todos().update(|v| v.sort_by_key(|item| item.id));
        // New order: [1, 2, 3]
        assert_eq!(item2.text().get(), "B"); // Still works

        // Third reorder: move to end
        store.todos().update(|v| {
            let item = v.remove(1); // Remove id=2
            v.push(item); // Push to end
        });
        // New order: [1, 3, 2]
        assert_eq!(item2.text().get(), "B"); // Still works

        // Update still works
        item2.text().set("B Modified".into());
        assert_eq!(store.todos().index(2).text().get(), "B Modified");
    }
}
