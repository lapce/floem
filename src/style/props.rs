//! Style property traits, classes, and keys.
//!
//! This module provides the core traits and types for defining and working with
//! style properties:
//! - [`StyleClass`] - trait for defining style classes
//! - [`StyleProp`] - trait for defining style properties
//! - [`StylePropReader`] - trait for reading properties from styles
//! - [`StyleKey`] - unique identifier for style entries
//! - Macros: `style_class!`, `prop!`, `prop_extractor!`

use std::any::{Any, type_name};
use std::fmt::{self, Debug};
use std::hash::{Hash, Hasher};
use std::ptr;
use std::rc::Rc;

use rustc_hash::FxHashMap;
use smallvec::SmallVec;

#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;
#[cfg(target_arch = "wasm32")]
use web_time::Instant;

use crate::view::{IntoView, View};
use crate::views::Label;

use super::Style;
use super::selectors::StyleSelectors;
use super::transition::TransitionState;
use super::values::{StyleMapValue, StylePropValue, StyleValue};

// ============================================================================
// StyleClass
// ============================================================================

pub trait StyleClass: Default + Copy + 'static {
    fn key() -> StyleKey;
    fn class_ref() -> StyleClassRef {
        StyleClassRef { key: Self::key() }
    }
}

pub trait StyleDebugGroup: Default + Copy + 'static {
    fn key() -> StyleKey;
    fn group_ref() -> StyleDebugGroupRef {
        StyleDebugGroupRef { key: Self::key() }
    }
    fn member_props() -> Vec<StyleKey>;
    fn debug_view(style: &Style) -> Option<Box<dyn View>>;
}

#[derive(Debug, Clone)]
pub struct StyleClassInfo {
    pub(crate) name: fn() -> &'static str,
}

impl StyleClassInfo {
    pub const fn new<Name>() -> Self {
        StyleClassInfo {
            name: || std::any::type_name::<Name>(),
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct StyleClassRef {
    pub key: StyleKey,
}

#[derive(Debug, Clone)]
pub struct StyleDebugGroupInfo {
    pub(crate) name: fn() -> &'static str,
    pub(crate) inherited: bool,
    pub(crate) member_props: fn() -> Vec<StyleKey>,
    pub(crate) debug_view: fn(style: &Style) -> Option<Box<dyn View>>,
}

impl StyleDebugGroupInfo {
    pub const fn new<Name>(
        inherited: bool,
        member_props: fn() -> Vec<StyleKey>,
        debug_view: fn(style: &Style) -> Option<Box<dyn View>>,
    ) -> Self {
        StyleDebugGroupInfo {
            name: || std::any::type_name::<Name>(),
            inherited,
            member_props,
            debug_view,
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct StyleDebugGroupRef {
    pub key: StyleKey,
}

macro_rules! style_key_selector {
    ($v:vis $name:ident, $sel:expr) => {
        fn $name() -> $crate::style::StyleKey {
            static INFO: $crate::style::StyleKeyInfo = $crate::style::StyleKeyInfo::Selector($sel);
            $crate::style::StyleKey { info: &INFO }
        }
    };
}

pub(crate) use style_key_selector;

#[macro_export]
macro_rules! style_class {
    ($(#[$meta:meta])* $v:vis $name:ident) => {
        $(#[$meta])*
        #[derive(Default, Copy, Clone)]
        $v struct $name;

        impl $crate::style::StyleClass for $name {
            fn key() -> $crate::style::StyleKey {
                static INFO: $crate::style::StyleKeyInfo = $crate::style::StyleKeyInfo::Class(
                    $crate::style::StyleClassInfo::new::<$name>()
                );
                $crate::style::StyleKey { info: &INFO }
            }
        }
    };
}

#[macro_export]
macro_rules! style_debug_group {
    ($(#[$meta:meta])* $v:vis $name:ident $(, inherited = $inherited:ident)?, members = [$($prop:ty),* $(,)?], view = $view:path) => {
        $(#[$meta])*
        #[derive(Default, Copy, Clone)]
        $v struct $name;

        impl $crate::style::StyleDebugGroup for $name {
            fn key() -> $crate::style::StyleKey {
                static INFO: $crate::style::StyleKeyInfo =
                    $crate::style::StyleKeyInfo::DebugGroup(
                        $crate::style::StyleDebugGroupInfo::new::<$name>(
                            style_debug_group!(@inherited $($inherited)?),
                            || vec![$(<$prop as $crate::style::StyleProp>::key()),*],
                            $view,
                        )
                    );
                $crate::style::StyleKey { info: &INFO }
            }

            fn member_props() -> Vec<$crate::style::StyleKey> {
                vec![$(<$prop as $crate::style::StyleProp>::key()),*]
            }

            fn debug_view(style: &$crate::style::Style) -> Option<Box<dyn $crate::view::View>> {
                $view(style)
            }
        }
    };
    (@inherited inherited) => {
        true
    };
    (@inherited) => {
        false
    };
}

// ============================================================================
// StyleProp
// ============================================================================

pub trait StyleProp: Default + Copy + 'static {
    type Type: StylePropValue;
    fn key() -> StyleKey;
    fn prop_ref() -> StylePropRef {
        StylePropRef { key: Self::key() }
    }
    fn default_value() -> Self::Type;
}

pub(crate) type InterpolateFn =
    fn(val1: &dyn Any, val2: &dyn Any, time: f64) -> Option<Rc<dyn Any>>;

/// Function pointer type for computing content hash of a style value.
pub(crate) type HashAnyFn = fn(val: &dyn Any) -> u64;

/// Function pointer type for comparing two style values for equality.
pub(crate) type EqAnyFn = fn(val1: &dyn Any, val2: &dyn Any) -> bool;

#[derive(Debug)]
pub struct StylePropInfo {
    pub(crate) name: fn() -> &'static str,
    pub(crate) inherited: bool,
    #[allow(unused)]
    pub(crate) default_as_any: fn() -> Rc<dyn Any>,
    pub(crate) interpolate: InterpolateFn,
    pub(crate) debug_any: fn(val: &dyn Any) -> String,
    pub(crate) debug_view: fn(val: &dyn Any) -> Option<Box<dyn View>>,
    pub(crate) transition_key: StyleKey,
    /// Computes a content-based hash for a style value.
    pub(crate) hash_any: HashAnyFn,
    /// Compares two style values for equality.
    pub(crate) eq_any: EqAnyFn,
}

impl StylePropInfo {
    pub const fn new<Name, T: StylePropValue + 'static>(
        inherited: bool,
        default_as_any: fn() -> Rc<dyn Any>,
        transition_key: StyleKey,
    ) -> Self {
        StylePropInfo {
            name: || std::any::type_name::<Name>(),
            inherited,
            default_as_any,
            debug_any: |val| {
                if let Some(v) = val.downcast_ref::<StyleMapValue<T>>() {
                    match v {
                        StyleMapValue::Val(v) | StyleMapValue::Animated(v) => format!("{v:?}"),

                        StyleMapValue::Unset => "Unset".to_owned(),
                    }
                } else {
                    panic!(
                        "expected type {} for property {}",
                        type_name::<T>(),
                        std::any::type_name::<Name>(),
                    )
                }
            },
            interpolate: |val1, val2, time| {
                if let (Some(v1), Some(v2)) = (
                    val1.downcast_ref::<StyleMapValue<T>>(),
                    val2.downcast_ref::<StyleMapValue<T>>(),
                ) {
                    if let (
                        StyleMapValue::Val(v1) | StyleMapValue::Animated(v1),
                        StyleMapValue::Val(v2) | StyleMapValue::Animated(v2),
                    ) = (v1, v2)
                    {
                        v1.interpolate(v2, time)
                            .map(|val| Rc::new(StyleMapValue::Animated(val)) as Rc<dyn Any>)
                    } else {
                        None
                    }
                } else {
                    panic!(
                        "expected type {} for property {}. Got typeids {:?} and {:?}",
                        type_name::<T>(),
                        std::any::type_name::<Name>(),
                        val1.type_id(),
                        val2.type_id()
                    )
                }
            },
            debug_view: |val| {
                if let Some(v) = val.downcast_ref::<StyleMapValue<T>>() {
                    match v {
                        StyleMapValue::Val(v) | StyleMapValue::Animated(v) => v.debug_view(),

                        StyleMapValue::Unset => Some(Label::new("Unset").into_any()),
                    }
                } else {
                    panic!(
                        "expected type {} for property {}",
                        type_name::<T>(),
                        std::any::type_name::<Name>(),
                    )
                }
            },
            transition_key,
            hash_any: |val| {
                if let Some(v) = val.downcast_ref::<StyleMapValue<T>>() {
                    match v {
                        StyleMapValue::Val(v) | StyleMapValue::Animated(v) => v.content_hash(),
                        StyleMapValue::Unset => 0, // Stable hash for unset
                    }
                } else {
                    panic!(
                        "expected type {} for property {}",
                        type_name::<T>(),
                        std::any::type_name::<Name>(),
                    )
                }
            },
            eq_any: |val1, val2| {
                if let (Some(v1), Some(v2)) = (
                    val1.downcast_ref::<StyleMapValue<T>>(),
                    val2.downcast_ref::<StyleMapValue<T>>(),
                ) {
                    match (v1, v2) {
                        (
                            StyleMapValue::Val(a) | StyleMapValue::Animated(a),
                            StyleMapValue::Val(b) | StyleMapValue::Animated(b),
                        ) => {
                            // Compare by content hash since we don't have PartialEq
                            a.content_hash() == b.content_hash()
                        }
                        (StyleMapValue::Unset, StyleMapValue::Unset) => true,
                        _ => false,
                    }
                } else {
                    panic!(
                        "expected type {} for property {}. Got typeids {:?} and {:?}",
                        type_name::<T>(),
                        std::any::type_name::<Name>(),
                        val1.type_id(),
                        val2.type_id()
                    )
                }
            },
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct StylePropRef {
    pub key: StyleKey,
}

impl StylePropRef {
    pub(crate) fn info(&self) -> &StylePropInfo {
        if let StyleKeyInfo::Prop(prop) = self.key.info {
            prop
        } else {
            panic!()
        }
    }
}

// ============================================================================
// StylePropReader
// ============================================================================

pub trait StylePropReader {
    type State: Debug;
    type Type: Clone;

    /// Reads the property from the style.
    /// Returns true if the property changed.
    fn read(
        state: &mut Self::State,
        style: &Style,
        fallback: &Style,
        now: &Instant,
        request_transition: &mut bool,
    ) -> bool;

    fn get(state: &Self::State) -> Self::Type;
    fn new() -> Self::State;
}

impl<P: StyleProp> StylePropReader for P {
    type State = (P::Type, TransitionState<P::Type>);
    type Type = P::Type;

    // returns true if the value has changed
    fn read(
        state: &mut Self::State,
        style: &Style,
        fallback: &Style,
        now: &Instant,
        request_transition: &mut bool,
    ) -> bool {
        // get the style property
        let style_value = style.get_prop_style_value::<P>();
        let mut prop_animated = false;
        let new = match style_value {
            StyleValue::Animated(val) => {
                *request_transition = true;
                prop_animated = true;
                val
            }
            StyleValue::Val(val) => val,
            StyleValue::Unset | StyleValue::Base => fallback
                .get_prop::<P>()
                .unwrap_or_else(|| P::default_value()),
        };
        // set the transition state to the transition if one is found
        state.1.read(
            style
                .get_transition::<P>()
                .or_else(|| fallback.get_transition::<P>()),
        );

        // there is a previously stored value in state.0. if the values are different, a transition should be started if there is one
        let changed = new != state.0;
        if changed && !prop_animated {
            state.1.transition(&Self::get(state), &new);
            state.0 = new;
        } else if prop_animated {
            state.0 = new;
        }
        changed | state.1.step(now, request_transition)
    }

    // get the current value from the transition state if one is active, else just return the value that was read from the style map
    fn get(state: &Self::State) -> Self::Type {
        state.1.get(&state.0)
    }

    fn new() -> Self::State {
        (P::default_value(), TransitionState::default())
    }
}

impl<P: StyleProp> StylePropReader for Option<P> {
    type State = Option<P::Type>;
    type Type = Option<P::Type>;
    fn read(
        state: &mut Self::State,
        style: &Style,
        fallback: &Style,
        _now: &Instant,
        _transition: &mut bool,
    ) -> bool {
        let new = style.get_prop::<P>().or_else(|| fallback.get_prop::<P>());
        let changed = new != *state;
        *state = new;
        changed
    }
    fn get(state: &Self::State) -> Self::Type {
        state.clone()
    }
    fn new() -> Self::State {
        None
    }
}

// ============================================================================
// ExtractorField
// ============================================================================

#[derive(Clone)]
pub struct ExtractorField<R: StylePropReader> {
    state: R::State,
}

impl<R: StylePropReader> Debug for ExtractorField<R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.state.fmt(f)
    }
}

impl<R: StylePropReader> ExtractorField<R> {
    pub fn read(
        &mut self,
        style: &Style,
        fallback: &Style,
        now: &Instant,
        request_transition: &mut bool,
    ) -> bool {
        R::read(&mut self.state, style, fallback, now, request_transition)
    }
    pub fn get(&self) -> R::Type {
        R::get(&self.state)
    }
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self { state: R::new() }
    }
}

impl<R: StylePropReader> PartialEq for ExtractorField<R>
where
    R::Type: PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        self.get() == other.get()
    }
}

impl<R: StylePropReader> Eq for ExtractorField<R> where R::Type: Eq {}

impl<R: StylePropReader> std::hash::Hash for ExtractorField<R>
where
    R::Type: std::hash::Hash,
{
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.get().hash(state)
    }
}

// ============================================================================
// Macros
// ============================================================================

#[macro_export]
macro_rules! prop {
    ($(#[$meta:meta])* $v:vis $name:ident: $ty:ty { $($options:tt)* } = $default:expr
    ) => {
        $(#[$meta])*
        #[derive(Default, Copy, Clone)]
        #[allow(missing_docs)]
        $v struct $name;
        impl $crate::style::StyleProp for $name {
            type Type = $ty;
            fn key() -> $crate::style::StyleKey {
                static TRANSITION_INFO: $crate::style::StyleKeyInfo = $crate::style::StyleKeyInfo::Transition;
                static INFO: $crate::style::StyleKeyInfo = $crate::style::StyleKeyInfo::Prop($crate::style::StylePropInfo::new::<$name, $ty>(
                    prop!([impl inherited][$($options)*]),
                    || std::rc::Rc::new($crate::style::StyleMapValue::Val($name::default_value())),
                    $crate::style::StyleKey { info: &TRANSITION_INFO },
                ));
                $crate::style::StyleKey { info: &INFO }
            }
            fn default_value() -> Self::Type {
                $default
            }
        }
    };
    ([impl inherited][inherited]) => {
        true
    };
    ([impl inherited][]) => {
        false
    };
}

#[macro_export]
macro_rules! prop_extractor {
    (
        $(#[$attrs:meta])* $vis:vis $name:ident {
            $($prop_vis:vis $prop:ident: $reader:ty),*
            $(,)?
        }
    ) => {
        #[derive(Debug, Clone)]
        $(#[$attrs])?
        $vis struct $name {
            $(
                $prop_vis $prop: $crate::style::ExtractorField<$reader>,
            )*
        }

        impl $name {
            #[allow(dead_code)]
            $vis fn read_style(&mut self, cx: &mut $crate::context::StyleCx, style: &$crate::style::Style) -> bool {
                self.read_style_for(cx, style, cx.current_view().get_element_id())
            }

            #[allow(dead_code)]
            $vis fn read_style_for(
                &mut self,
                cx: &mut $crate::context::StyleCx,
                style: &$crate::style::Style,
                target: impl Into<$crate::ElementId>,
            ) -> bool {
                let mut transition = false;
                let changed = false $(| self.$prop.read(style, style, &cx.now(), &mut transition))*;
                if transition {
                    cx.request_transition_for(target);
                }
                changed
            }

           #[allow(dead_code)]
            $vis fn read(&mut self, cx: &mut $crate::context::StyleCx) -> bool {
                self.read_for(cx, cx.current_view().get_element_id())
            }

           #[allow(dead_code)]
            $vis fn read_for(
                &mut self,
                cx: &mut $crate::context::StyleCx,
                target: impl Into<$crate::ElementId>,
            ) -> bool {
                let mut transition = false;
                let changed = self.read_explicit(&cx.direct_style(), &cx.indirect_style(), &cx.now(), &mut transition);
                if transition {
                    cx.request_transition_for(target);
                }
                changed
            }

            #[allow(dead_code)]
            $vis fn read_explicit(
                &mut self,
                style: &$crate::style::Style,
                fallback: &$crate::style::Style,
                #[cfg(not(target_arch = "wasm32"))]
                now: &std::time::Instant,
                #[cfg(target_arch = "wasm32")]
                now: &web_time::Instant,
                request_transition: &mut bool
            ) -> bool {
                false $(| self.$prop.read(style, fallback, now, request_transition))*
            }

            $($prop_vis fn $prop(&self) -> <$reader as $crate::style::StylePropReader>::Type
            {
                self.$prop.get()
            })*
        }

        impl Default for $name {
            fn default() -> Self {
                Self {
                    $(
                        $prop: $crate::style::ExtractorField::new(),
                    )*
                }
            }
        }
    };
}

// ============================================================================
// StyleKey
// ============================================================================

#[derive(Debug)]
pub enum StyleKeyInfo {
    Transition,
    Prop(StylePropInfo),
    Selector(StyleSelectors),
    Class(StyleClassInfo),
    DebugGroup(StyleDebugGroupInfo),
    /// Storage for context mapping closures.
    ContextMappings,
    /// Storage for parameterized structural selectors (`:first-child`, `:nth-child(...)`, etc.).
    StructuralSelectors,
    /// Storage for parameterized responsive selectors (`min/max/range` window width).
    ResponsiveSelectors,
}

pub(crate) static CONTEXT_MAPPINGS_INFO: StyleKeyInfo = StyleKeyInfo::ContextMappings;
pub(crate) static STRUCTURAL_SELECTORS_INFO: StyleKeyInfo = StyleKeyInfo::StructuralSelectors;
pub(crate) static RESPONSIVE_SELECTORS_INFO: StyleKeyInfo = StyleKeyInfo::ResponsiveSelectors;

#[derive(Copy, Clone)]
pub struct StyleKey {
    pub info: &'static StyleKeyInfo,
}

impl StyleKey {
    pub(crate) fn debug_any(&self, value: &dyn Any) -> String {
        match self.info {
            StyleKeyInfo::Selector(selectors) => selectors.debug_string(),
            StyleKeyInfo::Transition
            | StyleKeyInfo::DebugGroup(_)
            | StyleKeyInfo::ContextMappings
            | StyleKeyInfo::StructuralSelectors
            | StyleKeyInfo::ResponsiveSelectors => String::new(),
            StyleKeyInfo::Class(info) => (info.name)().to_string(),
            StyleKeyInfo::Prop(v) => (v.debug_any)(value),
        }
    }
    pub(crate) fn inherited(&self) -> bool {
        match self.info {
            StyleKeyInfo::Selector(..)
            | StyleKeyInfo::Transition
            | StyleKeyInfo::ContextMappings
            | StyleKeyInfo::StructuralSelectors
            | StyleKeyInfo::ResponsiveSelectors => false,
            StyleKeyInfo::Class(..) => true,
            StyleKeyInfo::DebugGroup(v) => v.inherited,
            StyleKeyInfo::Prop(v) => v.inherited,
        }
    }
}

impl PartialEq for StyleKey {
    fn eq(&self, other: &Self) -> bool {
        ptr::eq(self.info, other.info)
    }
}

impl Hash for StyleKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_usize(self.info as *const _ as usize)
    }
}

impl Eq for StyleKey {}

impl Debug for StyleKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.info {
            StyleKeyInfo::Selector(selectors) => {
                write!(f, "selectors: {}", selectors.debug_string())
            }
            StyleKeyInfo::Transition => write!(f, "transition"),
            StyleKeyInfo::ContextMappings => write!(f, "ContextMappings"),
            StyleKeyInfo::StructuralSelectors => write!(f, "StructuralSelectors"),
            StyleKeyInfo::ResponsiveSelectors => write!(f, "ResponsiveSelectors"),
            StyleKeyInfo::Class(v) => write!(f, "{}", (v.name)()),
            StyleKeyInfo::DebugGroup(v) => write!(f, "{}", (v.name)()),
            StyleKeyInfo::Prop(v) => write!(f, "{}", (v.name)()),
        }
    }
}

// ============================================================================
// MapStorage
// ============================================================================

/// Number of entries that stay fully inline inside [`MapStorage::Inline`].
///
/// The intent is that tiny style maps avoid heap allocation entirely. This is
/// the dominant case for local view styles and many nested selector/class maps.
const INLINE_STYLE_MAP_CAPACITY: usize = 4;

/// Maximum number of entries we allow in the spilled `SmallVec` representation
/// before switching to [`FxHashMap`].
///
/// This gives us three effective stages with a single enum:
/// 1. `SmallVec` inline storage while `len <= INLINE_STYLE_MAP_CAPACITY`
/// 2. spilled `SmallVec` heap storage while `INLINE_STYLE_MAP_CAPACITY < len <= HEAP_STYLE_MAP_CAPACITY`
/// 3. `FxHashMap` once the linear representation is large enough that lookup cost dominates
///
/// The exact threshold is a tuning knob and is intentionally documented here so
/// benchmark-driven adjustments stay easy.
const HEAP_STYLE_MAP_CAPACITY: usize = 16;

/// Backing storage for style maps.
///
/// # Motivation
///
/// The style system creates a very large number of tiny maps. A full hash map is
/// wasteful in both allocation cost and memory traffic for those cases, while a
/// flat linear representation is often faster when there are only a handful of
/// entries. At the same time, some merged styles do grow large enough that a
/// hash table becomes the right tradeoff.
///
/// This enum encodes that directly:
/// - `Inline`: a `SmallVec` that starts on the stack and transparently spills to
///   the heap once it outgrows its inline capacity
/// - `Hash`: an `FxHashMap` for the larger cases where repeated linear scans are
///   no longer acceptable
///
/// # Correctness Invariants
///
/// - Every key appears at most once, regardless of storage variant.
/// - Promotion from `Inline` to `Hash` preserves all key/value pairs.
/// - Removal is order-insensitive. We use `swap_remove` for the linear form,
///   which is correct because no caller relies on stable insertion order.
/// - Iteration order is therefore not semantically meaningful. Code that needs
///   deterministic hashing must sort keys explicitly, which the style cache does.
///
/// # API Design
///
/// The methods below intentionally mirror the small subset of map operations the
/// style system actually needs: lookup, insertion, removal, clearing, and cheap
/// iteration over keys/values/entries. Keeping that surface small makes it much
/// easier to reason about promotion behavior and cloning costs.
#[derive(Clone)]
pub(crate) enum MapStorage<K, V> {
    Inline(SmallVec<[(K, V); INLINE_STYLE_MAP_CAPACITY]>),
    Hash(FxHashMap<K, V>),
}

impl<K, V> Default for MapStorage<K, V> {
    fn default() -> Self {
        Self::Inline(SmallVec::new())
    }
}

impl<K, V> MapStorage<K, V>
where
    K: Copy + Eq + Hash,
{
    /// Returns the number of live entries in the map.
    ///
    /// This must agree across both representations and is used in hot paths such
    /// as content hashing and cacheability checks.
    pub fn len(&self) -> usize {
        match self {
            MapStorage::Inline(entries) => entries.len(),
            MapStorage::Hash(entries) => entries.len(),
        }
    }

    /// Returns `true` when the map contains no entries.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Clears all entries and resets the storage back to the inline form.
    ///
    /// Resetting to `Inline` is deliberate: a large style that is cleared should
    /// not permanently retain a hash map allocation if the next usage is tiny.
    pub fn clear(&mut self) {
        *self = MapStorage::Inline(SmallVec::new());
    }

    /// Returns a shared reference to the value for `key`, if present.
    ///
    /// The inline path performs a linear scan. That is intentional and faster
    /// than hashing for these tiny maps.
    pub fn get(&self, key: &K) -> Option<&V> {
        match self {
            MapStorage::Inline(entries) => entries.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            MapStorage::Hash(entries) => entries.get(key),
        }
    }

    /// Returns `true` if `key` is present.
    pub fn contains_key(&self, key: &K) -> bool {
        self.get(key).is_some()
    }

    /// Inserts or replaces a key/value pair.
    ///
    /// # Promotion Behavior
    ///
    /// While in `Inline`, we append until the spilled `SmallVec` reaches
    /// [`HEAP_STYLE_MAP_CAPACITY`]. Beyond that point we promote once into
    /// `FxHashMap` and continue there.
    ///
    /// # Correctness
    ///
    /// Replacements preserve the single-entry-per-key invariant. Promotion drains
    /// the linear storage exactly once into the hash map before inserting any
    /// future updates.
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        match self {
            MapStorage::Inline(entries) => {
                if let Some((_, existing)) = entries.iter_mut().find(|(k, _)| *k == key) {
                    return Some(std::mem::replace(existing, value));
                }
                if entries.len() < INLINE_STYLE_MAP_CAPACITY {
                    entries.push((key, value));
                    return None;
                }

                entries.push((key, value));
                if entries.len() <= HEAP_STYLE_MAP_CAPACITY {
                    return None;
                }

                let mut map = FxHashMap::default();
                map.reserve(entries.len());
                for (k, v) in entries.drain(..) {
                    map.insert(k, v);
                }
                *self = MapStorage::Hash(map);
                None
            }
            MapStorage::Hash(entries) => entries.insert(key, value),
        }
    }

    /// Removes `key` and returns its value if present.
    ///
    /// We use `swap_remove` in the inline representation because stable order is
    /// not part of the contract and this avoids shifting all tail elements.
    pub fn remove(&mut self, key: &K) -> Option<V> {
        match self {
            MapStorage::Inline(entries) => entries
                .iter()
                .position(|(k, _)| k == key)
                .map(|idx| entries.swap_remove(idx).1),
            MapStorage::Hash(entries) => entries.remove(key),
        }
    }

    /// Iterates over `(key, value)` pairs.
    pub fn iter(&self) -> MapStorageIter<'_, K, V> {
        match self {
            MapStorage::Inline(entries) => MapStorageIter::Inline(entries.iter()),
            MapStorage::Hash(entries) => MapStorageIter::Hash(entries.iter()),
        }
    }

    /// Iterates over keys.
    pub fn keys(&self) -> MapStorageKeys<'_, K, V> {
        match self {
            MapStorage::Inline(entries) => MapStorageKeys::Inline(entries.iter()),
            MapStorage::Hash(entries) => MapStorageKeys::Hash(entries.keys()),
        }
    }

    /// Iterates over values.
    pub fn values(&self) -> MapStorageValues<'_, K, V> {
        match self {
            MapStorage::Inline(entries) => MapStorageValues::Inline(entries.iter()),
            MapStorage::Hash(entries) => MapStorageValues::Hash(entries.values()),
        }
    }
}

/// Iterator over entries in [`MapStorage`].
pub(crate) enum MapStorageIter<'a, K, V> {
    Inline(std::slice::Iter<'a, (K, V)>),
    Hash(std::collections::hash_map::Iter<'a, K, V>),
}

impl<'a, K, V> Iterator for MapStorageIter<'a, K, V> {
    type Item = (&'a K, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            MapStorageIter::Inline(iter) => iter.next().map(|(k, v)| (k, v)),
            MapStorageIter::Hash(iter) => iter.next(),
        }
    }
}

/// Iterator over keys in [`MapStorage`].
pub(crate) enum MapStorageKeys<'a, K, V> {
    Inline(std::slice::Iter<'a, (K, V)>),
    Hash(std::collections::hash_map::Keys<'a, K, V>),
}

impl<'a, K, V> Iterator for MapStorageKeys<'a, K, V> {
    type Item = &'a K;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            MapStorageKeys::Inline(iter) => iter.next().map(|(k, _)| k),
            MapStorageKeys::Hash(iter) => iter.next(),
        }
    }
}

/// Iterator over values in [`MapStorage`].
pub(crate) enum MapStorageValues<'a, K, V> {
    Inline(std::slice::Iter<'a, (K, V)>),
    Hash(std::collections::hash_map::Values<'a, K, V>),
}

impl<'a, K, V> Iterator for MapStorageValues<'a, K, V> {
    type Item = &'a V;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            MapStorageValues::Inline(iter) => iter.next().map(|(_, v)| v),
            MapStorageValues::Hash(iter) => iter.next(),
        }
    }
}

impl<'a, K, V> IntoIterator for &'a MapStorage<K, V>
where
    K: Copy + Eq + Hash,
{
    type Item = (&'a K, &'a V);
    type IntoIter = MapStorageIter<'a, K, V>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
