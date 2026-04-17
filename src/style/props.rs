//! Floem-side property reader infrastructure.
//!
//! The core style-key types (`StyleKey`, `StyleProp`, `StyleClass`, etc.) now
//! live in the `floem-style` crate. What remains here is the
//! [`StylePropReader`] trait and [`ExtractorField`] helper, which reference
//! the `Style` type directly and so must stay in `floem`.

use std::fmt::{self, Debug};
use std::hash::Hasher;

#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;
#[cfg(target_arch = "wasm32")]
use web_time::Instant;

use super::Style;
use floem_style::{StyleValue, TransitionState};

pub use floem_style::props::{StyleClass, StyleClassInfo, StyleClassRef};
pub use floem_style::props::{StyleDebugGroup, StyleDebugGroupInfo, StyleDebugGroupRef};
pub use floem_style::props::{StyleKey, StyleKeyInfo};
pub use floem_style::props::{StyleProp, StylePropInfo, StylePropRef};
#[allow(unused_imports)]
pub use floem_style::props::{EqAnyFn, HashAnyFn, InterpolateFn, ResolveInheritedAnyFn};


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
        now: &Instant,
        request_transition: &mut bool,
    ) -> bool {
        // get the style property
        let style_value = style.get_prop_style_value::<P>();
        let mut prop_animated = false;
        let new = match style_value {
            StyleValue::Context(_) => {
                unreachable!("context values should resolve during property reads")
            }
            StyleValue::Animated(val) => {
                *request_transition = true;
                prop_animated = true;
                val
            }
            StyleValue::Val(val) => val,
            StyleValue::Unset | StyleValue::Base => P::default_value(),
        };
        // set the transition state to the transition if one is found
        state.1.read(style.get_transition::<P>());

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
    pub fn read(&mut self, style: &Style, now: &Instant, request_transition: &mut bool) -> bool {
        R::read(&mut self.state, style, now, request_transition)
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
