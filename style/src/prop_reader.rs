//! Property-reading abstractions used by `prop_extractor!`-generated structs.
//!
//! [`ExtractorField`] is the per-prop state slot the `prop_extractor!`
//! macro embeds in each generated struct: it holds the last-read value and
//! its transition state, and knows how to pull fresh values off a
//! [`Style`].
//!
//! [`PropExtractorCx`] is the narrow host interface the convenience
//! methods on extractor structs need — hosts implement it on whatever
//! per-view context they hand to `.read(cx)` / `.read_style(cx, ..)`.

use std::fmt::{self, Debug};
use std::hash::Hasher;

#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;
#[cfg(target_arch = "wasm32")]
use web_time::Instant;

use crate::props::StyleProp;
use crate::style::Style;
use crate::style_value::StyleValue;
use crate::transition::TransitionState;

// ============================================================================
// ExtractorField
// ============================================================================

#[derive(Clone)]
pub struct ExtractorField<P: StyleProp> {
    value: P::Type,
    transition: TransitionState<P::Type>,
}

impl<P: StyleProp> Debug for ExtractorField<P>
where
    P::Type: Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        (&self.value, &self.transition).fmt(f)
    }
}

impl<P: StyleProp> ExtractorField<P> {
    /// Pull the current value of `P` off `style`, update the transition
    /// state, and return whether the resolved value changed. Sets
    /// `*request_transition` if a transition is currently animating.
    pub fn read(&mut self, style: &Style, now: &Instant, request_transition: &mut bool) -> bool {
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
        self.transition.read(style.get_transition::<P>());

        let changed = new != self.value;
        if changed && !prop_animated {
            self.transition.transition(&self.get(), &new);
            self.value = new;
        } else if prop_animated {
            self.value = new;
        }
        changed | self.transition.step(now, request_transition)
    }

    /// Current resolved value, driven by the transition if one is active.
    pub fn get(&self) -> P::Type {
        self.transition.get(&self.value)
    }

    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self {
            value: P::default_value(),
            transition: TransitionState::default(),
        }
    }
}

impl<P: StyleProp> PartialEq for ExtractorField<P>
where
    P::Type: PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        self.get() == other.get()
    }
}

impl<P: StyleProp> Eq for ExtractorField<P> where P::Type: Eq {}

impl<P: StyleProp> std::hash::Hash for ExtractorField<P>
where
    P::Type: std::hash::Hash,
{
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.get().hash(state)
    }
}

// ============================================================================
// PropExtractorCx
// ============================================================================

/// Narrow host interface the `prop_extractor!`-generated convenience
/// methods (`read`, `read_style`) need.
///
/// Hosts implement this on their per-view style context type (e.g.
/// floem's `StyleCx`). A second host's context needs only to answer:
///
/// - "what time is it?" for transition ticks ([`Self::now`]),
/// - "what style am I extracting from?" for the no-arg `read`
///   convenience ([`Self::direct_style`]).
///
/// Transitions are reported as data: each generated method takes a
/// `&mut bool` the extractor sets when a read is still animating, and
/// the caller decides what to do (typically schedule a re-cascade on
/// the node). No host callback fires during property extraction.
///
/// Keeps the `prop_extractor!` macro fully engine-defined — the
/// macro's expansion references only this trait, never a host type.
pub trait PropExtractorCx {
    /// Frame timestamp used to advance in-flight transitions.
    fn now(&self) -> Instant;
    /// The merged direct style the extractor reads from when no
    /// explicit style is passed.
    fn direct_style(&self) -> &Style;
}
