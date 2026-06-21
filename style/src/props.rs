//! Style property traits, classes, and keys.
//!
//! This module provides the view-agnostic core of Floem's style property
//! system:
//! - [`StyleClass`] / [`StyleDebugGroup`] / [`StyleProp`] traits for defining
//!   styles.
//! - [`StyleKey`] / [`StyleKeyInfo`] — unique identifier for style entries.
//! - [`StylePropInfo`] / [`StyleClassInfo`] / [`StyleDebugGroupInfo`] —
//!   reflective metadata for each kind of key.
//!
//! The `style_class!`, `prop!`, and `style_debug_group!` macros live in this
//! crate (see `props.rs` and `style_macros.rs`). `prop_extractor!` remains
//! in the `floem` crate because it references `floem::context::StyleCx`.
//! `StylePropInfo`'s fields are `pub` rather than hidden behind a `new`
//! constructor because those macros construct it directly.

use std::any::Any;
use std::fmt::{self, Debug};
use std::hash::{Hash, Hasher};
use std::ptr;
use std::rc::Rc;

use crate::selectors::StyleSelectors;

// ============================================================================
// StyleClass
// ============================================================================

pub trait StyleClass: Default + Copy + 'static {
    fn key() -> StyleKey;
    fn class_ref() -> StyleClassRef {
        StyleClassRef { key: Self::key() }
    }
}

/// Declare a style class marker type.
///
/// Expands to a zero-sized struct that implements [`StyleClass`] by installing
/// a unique `'static` [`StyleKeyInfo::Class`] and returning a [`StyleKey`]
/// pointing at it.
#[macro_export]
macro_rules! style_class {
    ($(#[$meta:meta])* $v:vis $name:ident) => {
        $(#[$meta])*
        #[derive(Default, Copy, Clone)]
        $v struct $name;

        impl $crate::StyleClass for $name {
            fn key() -> $crate::StyleKey {
                static INFO: $crate::StyleKeyInfo = $crate::StyleKeyInfo::Class(
                    $crate::StyleClassInfo::new::<$name>()
                );
                $crate::StyleKey { info: &INFO }
            }
        }
    };
}

/// A trait for defining a logical grouping of [`StyleProp`] entries for the
/// style inspector. The group can provide a compact inspector preview that
/// summarises the group's members.
pub trait StyleDebugGroup: Default + Copy + 'static {
    fn key() -> StyleKey;
    fn group_ref() -> StyleDebugGroupRef {
        StyleDebugGroupRef { key: Self::key() }
    }
    fn member_props() -> Vec<StyleKey>;
}

#[derive(Debug, Clone)]
pub struct StyleClassInfo {
    pub name: fn() -> &'static str,
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
    pub name: fn() -> &'static str,
    pub inherited: bool,
    pub member_props: fn() -> Vec<StyleKey>,
}

impl StyleDebugGroupInfo {
    pub const fn new<Name>(inherited: bool, member_props: fn() -> Vec<StyleKey>) -> Self {
        StyleDebugGroupInfo {
            name: || std::any::type_name::<Name>(),
            inherited,
            member_props,
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct StyleDebugGroupRef {
    pub key: StyleKey,
}

// ============================================================================
// StyleProp
// ============================================================================

use crate::prop_value::StylePropValue;

pub trait StyleProp: Default + Copy + 'static {
    type Type: StylePropValue;
    fn key() -> StyleKey;
    fn prop_ref() -> StylePropRef {
        StylePropRef { key: Self::key() }
    }
    fn default_value() -> Self::Type;
}

pub type InterpolateFn =
    fn(val1: &dyn Any, val2: &dyn Any, time: f64) -> Option<Rc<dyn Any>>;

/// Function pointer type for computing content hash of a style value.
pub type HashAnyFn = fn(val: &dyn Any) -> u64;

/// Function pointer type for comparing two style values for equality.
pub type EqAnyFn = fn(val1: &dyn Any, val2: &dyn Any) -> bool;

/// Function pointer type for resolving a stored inherited property into a
/// concrete value. `style` is passed type-erased as `&dyn Any` so this
/// function pointer type does not need to name a concrete `Style`; the
/// `prop!` macro's expansion downcasts it back to [`crate::Style`].
pub type ResolveInheritedAnyFn = fn(val: &dyn Any, style: &dyn Any) -> Rc<dyn Any>;

/// A type-erased view of a stored style value, unwrapped from its
/// [`StyleMapValue`](crate::StyleMapValue) wrapper.
///
/// Returned by [`StylePropInfo::value_as_any`] so a host can dispatch on the
/// concrete value type — e.g. to render an inspector preview — without this
/// crate naming any view type. The borrow flows from the input value.
pub enum PropValueRef<'a> {
    /// A concrete value (set directly, or interpolating mid-animation),
    /// borrowed as `&dyn Any` for the host to downcast.
    Val(&'a dyn Any),
    /// The property resolves from context.
    Context,
    /// The property is explicitly unset.
    Unset,
}

#[derive(Debug)]
pub struct StylePropInfo {
    pub name: fn() -> &'static str,
    pub inherited: bool,
    #[allow(unused)]
    pub default_as_any: fn() -> Rc<dyn Any>,
    pub interpolate: InterpolateFn,
    pub debug_any: fn(val: &dyn Any) -> String,
    /// Type-erased accessor for the stored value, unwrapped from its
    /// [`StyleMapValue`](crate::StyleMapValue) wrapper. Lets a host dispatch
    /// on the concrete value type (e.g. for an inspector preview) without
    /// this crate depending on a view layer. The returned borrow flows from
    /// the input.
    pub value_as_any: for<'a> fn(&'a dyn Any) -> PropValueRef<'a>,
    pub transition_key: StyleKey,
    /// Computes a content-based hash for a style value.
    pub hash_any: HashAnyFn,
    /// Compares two style values for equality.
    pub eq_any: EqAnyFn,
    /// Resolves a stored property value for inheritance propagation.
    pub resolve_inherited_any: ResolveInheritedAnyFn,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct StylePropRef {
    pub key: StyleKey,
}

impl StylePropRef {
    pub fn info(&self) -> &StylePropInfo {
        if let StyleKeyInfo::Prop(prop) = self.key.info {
            prop
        } else {
            panic!()
        }
    }
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
    DeferredEffects,
    /// Storage for parameterized structural selectors (`:first-child`,
    /// `:nth-child(...)`, etc.).
    StructuralSelectors,
    /// Storage for parameterized responsive selectors (`min/max/range` window
    /// width).
    ResponsiveSelectors,
}

pub static STRUCTURAL_SELECTORS_INFO: StyleKeyInfo = StyleKeyInfo::StructuralSelectors;
pub static RESPONSIVE_SELECTORS_INFO: StyleKeyInfo = StyleKeyInfo::ResponsiveSelectors;

#[derive(Copy, Clone)]
pub struct StyleKey {
    pub info: &'static StyleKeyInfo,
}

impl StyleKey {
    pub fn debug_any(&self, value: &dyn Any) -> String {
        match self.info {
            StyleKeyInfo::Selector(selectors) => selectors.debug_string(),
            StyleKeyInfo::Transition
            | StyleKeyInfo::DebugGroup(_)
            | StyleKeyInfo::DeferredEffects
            | StyleKeyInfo::StructuralSelectors
            | StyleKeyInfo::ResponsiveSelectors => String::new(),
            StyleKeyInfo::Class(info) => (info.name)().to_string(),
            StyleKeyInfo::Prop(v) => (v.debug_any)(value),
        }
    }
    pub fn inherited(&self) -> bool {
        match self.info {
            StyleKeyInfo::Selector(..)
            | StyleKeyInfo::Transition
            | StyleKeyInfo::DeferredEffects
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
            StyleKeyInfo::DeferredEffects => write!(f, "DeferredEffects"),
            StyleKeyInfo::StructuralSelectors => write!(f, "StructuralSelectors"),
            StyleKeyInfo::ResponsiveSelectors => write!(f, "ResponsiveSelectors"),
            StyleKeyInfo::Class(v) => write!(f, "{}", (v.name)()),
            StyleKeyInfo::DebugGroup(v) => write!(f, "{}", (v.name)()),
            StyleKeyInfo::Prop(v) => write!(f, "{}", (v.name)()),
        }
    }
}
