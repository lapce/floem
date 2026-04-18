//! Property-reading abstractions used by `prop_extractor!`-generated structs.
//!
//! [`StylePropReader`] is the trait that lets a prop extractor pull a value
//! (with transition state) out of a [`Style`]. Every [`StyleProp`] gets a
//! blanket impl. [`ExtractorField`] is the per-prop state slot the
//! `prop_extractor!` macro embeds in each generated struct.
//! [`PropExtractorCx`] is the narrow host interface the
//! convenience methods on extractor structs need — hosts
//! implement it on whatever per-view context they hand to
//! `.read(cx)` / `.read_style(cx, ..)`.

use std::fmt::{self, Debug};
use std::hash::Hasher;

#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;
#[cfg(target_arch = "wasm32")]
use web_time::Instant;

use crate::element_id::ElementId;
use crate::props::StyleProp;
use crate::style::Style;
use crate::style_value::StyleValue;
use crate::transition::TransitionState;

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

// ============================================================================
// PropExtractorCx
// ============================================================================

/// Narrow host interface the `prop_extractor!`-generated convenience
/// methods (`read`, `read_for`, `read_style`, `read_style_for`) need.
///
/// Hosts implement this on their per-view style context type (e.g.
/// floem's `StyleCx`). A second host's context needs only to answer:
///
/// - "what time is it?" for transition ticks ([`Self::now`]),
/// - "what style am I extracting from?" for the no-arg `read`
///   convenience ([`Self::direct_style`]),
/// - "what element am I styling?" so callers that don't pass a
///   `target` default to the current one ([`Self::current_element`]),
/// - "a transition is active, please restyle this element next
///   frame" ([`Self::request_transition_for`]).
///
/// Keeps the `prop_extractor!` macro fully engine-defined — the
/// macro's expansion references only this trait, never a host type.
pub trait PropExtractorCx {
    /// Frame timestamp used to advance in-flight transitions.
    fn now(&self) -> Instant;
    /// The merged direct style the extractor reads from when no
    /// explicit style is passed.
    fn direct_style(&self) -> &Style;
    /// Element currently being styled; used as the default `target`
    /// of transition re-style requests.
    fn current_element(&self) -> ElementId;
    /// Request that `target` be re-styled on the next frame because
    /// at least one transition on this pass is still animating.
    fn request_transition_for(&mut self, target: ElementId);
}
