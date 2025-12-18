use std::any::{Any, TypeId};

use crate::runtime::RUNTIME;

/// A marker type for context operations.
///
/// Provides static methods for storing and retrieving context values
/// in the reactive scope hierarchy.
///
/// # Example
/// ```rust
/// # use floem_reactive::{Context, Scope};
/// let scope = Scope::new();
/// scope.enter(|| {
///     Context::provide(42i32);
///     Context::provide(String::from("Hello"));
///
///     assert_eq!(Context::get::<i32>(), Some(42));
///     assert_eq!(Context::get::<String>(), Some(String::from("Hello")));
/// });
/// ```
pub struct Context;

impl Context {
    /// Store a context value in the current scope.
    ///
    /// The stored context value can be retrieved by the current scope and any of its
    /// descendants using [`Context::get`]. Child scopes can provide their own values
    /// of the same type, which will shadow the parent's value for that subtree.
    ///
    /// Context values are automatically cleaned up when the scope is disposed.
    ///
    /// # Example
    /// ```rust
    /// # use floem_reactive::{Context, Scope};
    /// let scope = Scope::new();
    /// scope.enter(|| {
    ///     Context::provide(42i32);
    ///     assert_eq!(Context::get::<i32>(), Some(42));
    /// });
    /// ```
    pub fn provide<T>(value: T)
    where
        T: Clone + 'static,
    {
        let ty = TypeId::of::<T>();

        RUNTIME.with(|runtime| {
            let scope = *runtime.current_scope.borrow();
            runtime
                .scope_contexts
                .borrow_mut()
                .entry(scope)
                .or_default()
                .insert(ty, Box::new(value) as Box<dyn Any>);
        });
    }

    /// Try to retrieve a stored context value from the current scope or its ancestors.
    ///
    /// Context lookup walks up the scope tree from the current scope to find the
    /// nearest ancestor that provides a value of the requested type. This enables
    /// nested components to override context values for their subtrees.
    ///
    /// # Example
    /// ```rust
    /// # use floem_reactive::{Context, Scope};
    /// let parent = Scope::new();
    /// parent.enter(|| {
    ///     Context::provide(42i32);
    ///
    ///     let child = parent.create_child();
    ///     child.enter(|| {
    ///         // Child sees parent's context
    ///         assert_eq!(Context::get::<i32>(), Some(42));
    ///     });
    /// });
    /// ```
    pub fn get<T>() -> Option<T>
    where
        T: Clone + 'static,
    {
        let ty = TypeId::of::<T>();
        RUNTIME.with(|runtime| {
            let mut scope = *runtime.current_scope.borrow();
            let scope_contexts = runtime.scope_contexts.borrow();
            let parents = runtime.parents.borrow();

            loop {
                if let Some(contexts) = scope_contexts.get(&scope) {
                    if let Some(value) = contexts.get(&ty) {
                        return value.downcast_ref::<T>().cloned();
                    }
                }
                // Walk up to parent scope
                match parents.get(&scope) {
                    Some(&parent) => scope = parent,
                    None => return None,
                }
            }
        })
    }
}

/// Try to retrieve a stored Context value in the reactive system.
///
/// Context lookup walks up the scope tree from the current scope to find the
/// nearest ancestor that provides a value of the requested type. This enables
/// nested components to override context values for their subtrees.
///
/// # Example
/// In a parent component:
/// ```rust
/// # use floem_reactive::provide_context;
/// provide_context(42);
/// provide_context(String::from("Hello world"));
/// ```
///
/// And so in a child component you can retrieve each context data by specifying the type:
/// ```rust
/// # use floem_reactive::use_context;
/// let foo: Option<i32> = use_context();
/// let bar: Option<String> = use_context();
/// ```
#[deprecated(
    since = "0.2.0",
    note = "Use Context::get instead; this will be removed in a future release"
)]
pub fn use_context<T>() -> Option<T>
where
    T: Clone + 'static,
{
    let ty = TypeId::of::<T>();
    RUNTIME.with(|runtime| {
        let mut scope = *runtime.current_scope.borrow();
        let scope_contexts = runtime.scope_contexts.borrow();
        let parents = runtime.parents.borrow();

        loop {
            if let Some(contexts) = scope_contexts.get(&scope) {
                if let Some(value) = contexts.get(&ty) {
                    return value.downcast_ref::<T>().cloned();
                }
            }
            // Walk up to parent scope
            match parents.get(&scope) {
                Some(&parent) => scope = parent,
                None => return None,
            }
        }
    })
}

/// Sets a context value to be stored in the current scope.
///
/// The stored context value can be retrieved by the current scope and any of its
/// descendants using [use_context](use_context). Child scopes can provide their
/// own values of the same type, which will shadow the parent's value for that
/// subtree.
///
/// Context values are automatically cleaned up when the scope is disposed.
///
/// # Example
/// In a parent component:
/// ```rust
/// # use floem_reactive::provide_context;
/// provide_context(42);
/// provide_context(String::from("Hello world"));
/// ```
///
/// And so in a child component you can retrieve each context data by specifying the type:
/// ```rust
/// # use floem_reactive::use_context;
/// let foo: Option<i32> = use_context();
/// let bar: Option<String> = use_context();
/// ```
#[deprecated(
    since = "0.2.0",
    note = "Use Context::provide instead; this will be removed in a future release"
)]
pub fn provide_context<T>(value: T)
where
    T: Clone + 'static,
{
    let ty = TypeId::of::<T>();

    RUNTIME.with(|runtime| {
        let scope = *runtime.current_scope.borrow();
        runtime
            .scope_contexts
            .borrow_mut()
            .entry(scope)
            .or_default()
            .insert(ty, Box::new(value) as Box<dyn Any>);
    });
}

#[cfg(test)]
mod tests {
    use crate::scope::Scope;

    use super::*;

    #[test]
    fn context_in_same_scope() {
        let scope = Scope::new();
        scope.enter(|| {
            provide_context(42i32);
            assert_eq!(use_context::<i32>(), Some(42));
        });
    }

    #[test]
    fn context_inherited_from_parent() {
        let parent = Scope::new();
        parent.enter(|| {
            provide_context(42i32);

            let child = parent.create_child();
            child.enter(|| {
                // Child should see parent's context
                assert_eq!(use_context::<i32>(), Some(42));
            });
        });
    }

    #[test]
    fn context_shadowing_in_child() {
        let parent = Scope::new();
        parent.enter(|| {
            provide_context(42i32);

            let child = parent.create_child();
            child.enter(|| {
                // Override in child scope
                provide_context(100i32);
                assert_eq!(use_context::<i32>(), Some(100));
            });

            // Parent still has original value
            assert_eq!(use_context::<i32>(), Some(42));
        });
    }

    #[test]
    fn sibling_scopes_isolated() {
        let parent = Scope::new();
        parent.enter(|| {
            provide_context(0i32);

            let child1 = parent.create_child();
            let child2 = parent.create_child();

            child1.enter(|| {
                provide_context(1i32);
                assert_eq!(use_context::<i32>(), Some(1));
            });

            child2.enter(|| {
                provide_context(2i32);
                assert_eq!(use_context::<i32>(), Some(2));
            });

            // Verify they're still isolated
            child1.enter(|| {
                assert_eq!(use_context::<i32>(), Some(1));
            });

            child2.enter(|| {
                assert_eq!(use_context::<i32>(), Some(2));
            });
        });
    }

    #[test]
    fn context_cleaned_up_on_dispose() {
        let parent = Scope::new();
        let child_value = parent.enter(|| {
            provide_context(42i32);

            let child = parent.create_child();
            let value = child.enter(|| {
                provide_context(100i32);
                use_context::<i32>()
            });

            // Dispose child
            child.dispose();

            value
        });

        assert_eq!(child_value, Some(100));

        // After dispose, parent context should still work
        parent.enter(|| {
            assert_eq!(use_context::<i32>(), Some(42));
        });
    }

    #[test]
    fn deeply_nested_context_lookup() {
        let root = Scope::new();
        root.enter(|| {
            provide_context(String::from("root"));

            let level1 = root.create_child();
            level1.enter(|| {
                let level2 = level1.create_child();
                level2.enter(|| {
                    let level3 = level2.create_child();
                    level3.enter(|| {
                        // Should find root's context 3 levels up
                        assert_eq!(use_context::<String>(), Some(String::from("root")));
                    });
                });
            });
        });
    }

    #[test]
    fn multiple_context_types() {
        let scope = Scope::new();
        scope.enter(|| {
            provide_context(42i32);
            provide_context(String::from("hello"));
            provide_context(3.15f64);

            assert_eq!(use_context::<i32>(), Some(42));
            assert_eq!(use_context::<String>(), Some(String::from("hello")));
            assert_eq!(use_context::<f64>(), Some(3.15));
            assert_eq!(use_context::<bool>(), None);
        });
    }

    #[test]
    fn context_not_found_returns_none() {
        let scope = Scope::new();
        scope.enter(|| {
            // No context provided, should return None
            assert_eq!(use_context::<i32>(), None);
            assert_eq!(use_context::<String>(), None);
        });
    }

    #[test]
    fn overwrite_context_in_same_scope() {
        let scope = Scope::new();
        scope.enter(|| {
            provide_context(42i32);
            assert_eq!(use_context::<i32>(), Some(42));

            // Overwrite in same scope
            provide_context(100i32);
            assert_eq!(use_context::<i32>(), Some(100));
        });
    }

    #[test]
    fn parent_disposal_cleans_up_children() {
        let parent = Scope::new();
        let child = parent.create_child();

        parent.enter(|| {
            provide_context(42i32);
        });

        child.enter(|| {
            provide_context(100i32);
        });

        // Verify contexts exist
        parent.enter(|| {
            assert_eq!(use_context::<i32>(), Some(42));
        });
        child.enter(|| {
            assert_eq!(use_context::<i32>(), Some(100));
        });

        // Dispose parent - should clean up child too
        parent.dispose();

        // Verify parent's context is gone (scope no longer has context)
        RUNTIME.with(|runtime| {
            assert!(runtime.scope_contexts.borrow().get(&parent.0).is_none());
            assert!(runtime.scope_contexts.borrow().get(&child.0).is_none());
            assert!(runtime.parents.borrow().get(&child.0).is_none());
        });
    }

    #[test]
    fn context_shadowing_at_multiple_levels() {
        let root = Scope::new();
        root.enter(|| {
            provide_context(String::from("root"));

            let level1 = root.create_child();
            level1.enter(|| {
                // Don't provide at level1, should inherit from root
                assert_eq!(use_context::<String>(), Some(String::from("root")));

                let level2 = level1.create_child();
                level2.enter(|| {
                    // Shadow at level2
                    provide_context(String::from("level2"));
                    assert_eq!(use_context::<String>(), Some(String::from("level2")));

                    let level3 = level2.create_child();
                    level3.enter(|| {
                        // Should see level2's value, not root's
                        assert_eq!(use_context::<String>(), Some(String::from("level2")));
                    });
                });

                // Back at level1, should still see root's value
                assert_eq!(use_context::<String>(), Some(String::from("root")));
            });
        });
    }

    #[test]
    fn child_created_via_set_scope_inherits_context() {
        use crate::id::Id;

        let parent = Scope::new();
        parent.enter(|| {
            provide_context(42i32);

            // Simulate what happens when a signal is created - it calls set_scope
            let child_id = Id::next();
            child_id.set_scope();

            // The child_id should now have parent as its parent
            RUNTIME.with(|runtime| {
                let parents = runtime.parents.borrow();
                assert_eq!(parents.get(&child_id), Some(&parent.0));
            });
        });
    }

    #[test]
    fn orphan_scope_has_no_parent_context() {
        // Scope::new() creates a scope with no parent
        let orphan = Scope::new();
        orphan.enter(|| {
            // No parent, so no inherited context
            assert_eq!(use_context::<i32>(), None);

            // But can still provide its own context
            provide_context(42i32);
            assert_eq!(use_context::<i32>(), Some(42));
        });
    }

    #[test]
    fn newtype_wrappers_are_distinct_types() {
        #[derive(Clone, PartialEq, Debug)]
        struct UserId(i32);

        #[derive(Clone, PartialEq, Debug)]
        struct PostId(i32);

        let scope = Scope::new();
        scope.enter(|| {
            provide_context(UserId(1));
            provide_context(PostId(2));

            // Same underlying type (i32) but different wrapper types
            assert_eq!(use_context::<UserId>(), Some(UserId(1)));
            assert_eq!(use_context::<PostId>(), Some(PostId(2)));

            // Raw i32 is not provided
            assert_eq!(use_context::<i32>(), None);
        });
    }

    #[test]
    fn context_with_rw_signal() {
        use crate::{create_rw_signal, SignalGet, SignalUpdate};

        let scope = Scope::new();
        scope.enter(|| {
            let signal = create_rw_signal(42);
            provide_context(signal);

            // Retrieve the signal from context
            let retrieved = use_context::<crate::signal::RwSignal<i32>>().unwrap();
            assert_eq!(retrieved.get_untracked(), 42);

            // Modifying the retrieved signal should affect the original
            // (they're the same signal, just cloned handle)
            retrieved.set(100);
            assert_eq!(signal.get_untracked(), 100);
        });
    }

    #[test]
    fn dispose_middle_of_hierarchy() {
        let root = Scope::new();
        let middle = root.create_child();
        let leaf = middle.create_child();

        root.enter(|| provide_context(String::from("root")));
        middle.enter(|| provide_context(String::from("middle")));
        leaf.enter(|| provide_context(String::from("leaf")));

        // Verify all contexts exist
        RUNTIME.with(|runtime| {
            assert!(runtime.scope_contexts.borrow().contains_key(&root.0));
            assert!(runtime.scope_contexts.borrow().contains_key(&middle.0));
            assert!(runtime.scope_contexts.borrow().contains_key(&leaf.0));
        });

        // Dispose middle - should clean up leaf too
        middle.dispose();

        RUNTIME.with(|runtime| {
            // Root should still exist
            assert!(runtime.scope_contexts.borrow().contains_key(&root.0));
            // Middle and leaf should be gone
            assert!(!runtime.scope_contexts.borrow().contains_key(&middle.0));
            assert!(!runtime.scope_contexts.borrow().contains_key(&leaf.0));
            // Parent tracking for leaf should be gone
            assert!(!runtime.parents.borrow().contains_key(&leaf.0));
        });

        // Root context still works
        root.enter(|| {
            assert_eq!(use_context::<String>(), Some(String::from("root")));
        });
    }

    #[test]
    fn create_child_outside_enter() {
        let parent = Scope::new();

        // Provide context while inside parent
        parent.enter(|| {
            provide_context(42i32);
        });

        // Create child outside of enter - should still establish parent relationship
        let child = parent.create_child();

        // Child should be able to see parent's context
        child.enter(|| {
            assert_eq!(use_context::<i32>(), Some(42));
        });
    }

    #[test]
    fn context_not_visible_to_parent() {
        let parent = Scope::new();
        let child = parent.create_child();

        // Child provides context
        child.enter(|| {
            provide_context(100i32);
        });

        // Parent should NOT see child's context
        parent.enter(|| {
            assert_eq!(use_context::<i32>(), None);
        });
    }

    #[test]
    fn scope_reentry_preserves_context() {
        let scope = Scope::new();

        scope.enter(|| {
            provide_context(42i32);
        });

        // Re-enter the same scope
        scope.enter(|| {
            // Context should still be there
            assert_eq!(use_context::<i32>(), Some(42));

            // Can update it
            provide_context(100i32);
        });

        // And it persists
        scope.enter(|| {
            assert_eq!(use_context::<i32>(), Some(100));
        });
    }

    #[test]
    fn nested_enter_same_scope() {
        let scope = Scope::new();

        scope.enter(|| {
            provide_context(42i32);

            // Nested enter of the same scope
            scope.enter(|| {
                assert_eq!(use_context::<i32>(), Some(42));
                provide_context(100i32);
            });

            // After nested enter returns, we're back in the first enter
            // The context was updated
            assert_eq!(use_context::<i32>(), Some(100));
        });
    }

    #[test]
    fn context_with_enter_child() {
        let parent = Scope::new();

        parent.enter(|| {
            provide_context(42i32);

            // enter_child creates a child scope and wraps a closure
            let make_child = parent.enter_child(|multiplier: i32| {
                // Should see parent's context
                let val = use_context::<i32>().unwrap();
                // Can provide own context
                provide_context(String::from("child"));
                val * multiplier
            });

            // Call the wrapped closure
            let (result, child_scope) = make_child(2);
            assert_eq!(result, 84);

            // Child scope has its own context
            child_scope.enter(|| {
                assert_eq!(use_context::<String>(), Some(String::from("child")));
                // But still inherits parent's i32
                assert_eq!(use_context::<i32>(), Some(42));
            });

            // Parent doesn't see child's String context
            assert_eq!(use_context::<String>(), None);
        });
    }

    #[test]
    fn context_visible_in_effect() {
        use crate::{create_effect, create_rw_signal, SignalGet};
        use std::cell::Cell;
        use std::rc::Rc;

        let scope = Scope::new();
        let seen_value = Rc::new(Cell::new(0i32));

        scope.enter(|| {
            provide_context(42i32);
            let trigger = create_rw_signal(0);

            let seen = seen_value.clone();
            create_effect(move |_| {
                trigger.get(); // Subscribe to trigger
                               // Effect should see the context from the scope it was created in
                if let Some(val) = use_context::<i32>() {
                    seen.set(val);
                }
            });
        });

        // Effect runs immediately on creation
        assert_eq!(seen_value.get(), 42);
    }

    #[test]
    fn zero_sized_type_as_context_marker() {
        // ZST marker types are a common pattern
        #[derive(Clone, Debug, PartialEq)]
        struct DarkMode;

        #[derive(Clone, Debug, PartialEq)]
        struct DebugEnabled;

        let scope = Scope::new();
        scope.enter(|| {
            // Provide marker
            provide_context(DarkMode);

            // Check presence
            assert!(use_context::<DarkMode>().is_some());
            assert!(use_context::<DebugEnabled>().is_none());

            let child = scope.create_child();
            child.enter(|| {
                // Child inherits marker
                assert!(use_context::<DarkMode>().is_some());

                // Child can add its own marker
                provide_context(DebugEnabled);
                assert!(use_context::<DebugEnabled>().is_some());
            });

            // Parent doesn't see child's marker
            assert!(use_context::<DebugEnabled>().is_none());
        });
    }

    // Tests for the new Context struct API
    #[test]
    fn context_struct_provide_and_get() {
        let scope = Scope::new();
        scope.enter(|| {
            Context::provide(42i32);
            Context::provide(String::from("hello"));

            assert_eq!(Context::get::<i32>(), Some(42));
            assert_eq!(Context::get::<String>(), Some(String::from("hello")));
            assert_eq!(Context::get::<f64>(), None);
        });
    }

    #[test]
    fn context_struct_inheritance() {
        let parent = Scope::new();
        parent.enter(|| {
            Context::provide(42i32);

            let child = parent.create_child();
            child.enter(|| {
                // Child sees parent's context
                assert_eq!(Context::get::<i32>(), Some(42));

                // Child can shadow
                Context::provide(100i32);
                assert_eq!(Context::get::<i32>(), Some(100));
            });

            // Parent still has original
            assert_eq!(Context::get::<i32>(), Some(42));
        });
    }

    // Tests for Scope::provide_context and Scope::get_context
    #[test]
    fn scope_provide_and_get_context() {
        let scope = Scope::new();
        scope.provide_context(42i32);
        scope.provide_context(String::from("hello"));

        assert_eq!(scope.get_context::<i32>(), Some(42));
        assert_eq!(scope.get_context::<String>(), Some(String::from("hello")));
        assert_eq!(scope.get_context::<f64>(), None);
    }

    #[test]
    fn scope_context_inheritance() {
        let parent = Scope::new();
        parent.provide_context(42i32);

        let child = parent.create_child();
        // Child should see parent's context
        assert_eq!(child.get_context::<i32>(), Some(42));

        // Child can provide its own
        child.provide_context(100i32);
        assert_eq!(child.get_context::<i32>(), Some(100));

        // Parent still has original
        assert_eq!(parent.get_context::<i32>(), Some(42));
    }

    #[test]
    fn scope_context_not_visible_to_parent() {
        let parent = Scope::new();
        let child = parent.create_child();

        child.provide_context(100i32);

        // Parent should NOT see child's context
        assert_eq!(parent.get_context::<i32>(), None);
        assert_eq!(child.get_context::<i32>(), Some(100));
    }

    #[test]
    fn dispose_cleans_up_multiple_children() {
        let parent = Scope::new();
        let child1 = parent.create_child();
        let child2 = parent.create_child();
        let child3 = parent.create_child();

        parent.provide_context(String::from("parent"));
        child1.provide_context(String::from("child1"));
        child2.provide_context(String::from("child2"));
        child3.provide_context(String::from("child3"));

        // Verify all contexts exist
        RUNTIME.with(|runtime| {
            assert!(runtime.scope_contexts.borrow().contains_key(&parent.0));
            assert!(runtime.scope_contexts.borrow().contains_key(&child1.0));
            assert!(runtime.scope_contexts.borrow().contains_key(&child2.0));
            assert!(runtime.scope_contexts.borrow().contains_key(&child3.0));
        });

        // Dispose parent - should clean up all children
        parent.dispose();

        RUNTIME.with(|runtime| {
            // All should be gone
            assert!(!runtime.scope_contexts.borrow().contains_key(&parent.0));
            assert!(!runtime.scope_contexts.borrow().contains_key(&child1.0));
            assert!(!runtime.scope_contexts.borrow().contains_key(&child2.0));
            assert!(!runtime.scope_contexts.borrow().contains_key(&child3.0));
            // Parent tracking should be gone for all children
            assert!(!runtime.parents.borrow().contains_key(&child1.0));
            assert!(!runtime.parents.borrow().contains_key(&child2.0));
            assert!(!runtime.parents.borrow().contains_key(&child3.0));
        });
    }

    #[test]
    fn dispose_parent_without_context_cleans_children_context() {
        let parent = Scope::new();
        let child1 = parent.create_child();
        let child2 = parent.create_child();

        // Parent has NO context, but children do
        child1.provide_context(String::from("child1"));
        child2.provide_context(String::from("child2"));

        // Verify children's contexts exist
        RUNTIME.with(|runtime| {
            assert!(!runtime.scope_contexts.borrow().contains_key(&parent.0));
            assert!(runtime.scope_contexts.borrow().contains_key(&child1.0));
            assert!(runtime.scope_contexts.borrow().contains_key(&child2.0));
        });

        // Dispose parent - should still clean up children's contexts
        parent.dispose();

        RUNTIME.with(|runtime| {
            assert!(!runtime.scope_contexts.borrow().contains_key(&child1.0));
            assert!(!runtime.scope_contexts.borrow().contains_key(&child2.0));
            assert!(!runtime.parents.borrow().contains_key(&child1.0));
            assert!(!runtime.parents.borrow().contains_key(&child2.0));
        });
    }

    #[test]
    fn double_dispose_is_idempotent() {
        let scope = Scope::new();
        let child = scope.create_child();

        scope.provide_context(42i32);
        child.provide_context(100i32);

        // First dispose
        scope.dispose();

        RUNTIME.with(|runtime| {
            assert!(!runtime.scope_contexts.borrow().contains_key(&scope.0));
            assert!(!runtime.scope_contexts.borrow().contains_key(&child.0));
        });

        // Second dispose - should not panic
        scope.dispose();
        child.dispose();

        // Still clean
        RUNTIME.with(|runtime| {
            assert!(!runtime.scope_contexts.borrow().contains_key(&scope.0));
            assert!(!runtime.scope_contexts.borrow().contains_key(&child.0));
        });
    }
}
