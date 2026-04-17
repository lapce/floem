//! Per-node visibility state used to drive enter/exit transitions.
//!
//! [`Visibility`] tracks a node's current [`VisibilityPhase`]. The engine
//! advances the phase via [`VisibilityPhase::transition`], invoking
//! host-provided callbacks to start/stop/reset animations at each edge.

/// The current phase of visibility for enter/exit animations.
///
/// This enum tracks the display state during CSS-driven visibility transitions
/// (e.g., animating from visible to display:none).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VisibilityPhase {
    /// Initial state — display not yet computed.
    #[default]
    Initial,
    /// Visible with the given display mode.
    Visible(taffy::style::Display),
    /// Exit animation in progress.
    Animating(taffy::style::Display),
    /// Hidden (display: none).
    Hidden,
}

impl VisibilityPhase {
    /// Returns the `taffy::Display` override this phase imposes, if any.
    pub fn get_display_override(&self) -> Option<taffy::style::Display> {
        match self {
            VisibilityPhase::Animating(dis) => Some(*dis),
            _ => None,
        }
    }

    /// Advance the phase toward `computed_display`, invoking host callbacks
    /// to start/stop/reset enter and exit animations at the edges.
    pub fn transition(
        &mut self,
        computed_display: taffy::Display,
        remove_animations: impl FnOnce() -> bool,
        add_animations: impl FnOnce(),
        stop_reset_animations: impl FnOnce(),
        num_waiting_anim: impl FnOnce() -> u16,
    ) {
        let computed_has_hide = computed_display == taffy::Display::None;
        *self = match self {
            // Initial states — skip animations on initial app/view load.
            Self::Initial if computed_has_hide => Self::Hidden,
            Self::Initial if !computed_has_hide => Self::Visible(computed_display),
            // No transition needed.
            Self::Visible(dis) if !computed_has_hide => Self::Visible(*dis),
            // Transition to hidden.
            Self::Visible(dis) if computed_has_hide => {
                let active_animations = remove_animations();
                if active_animations {
                    Self::Animating(*dis)
                } else {
                    Self::Hidden
                }
            }
            Self::Animating(_) if !computed_has_hide => {
                stop_reset_animations();
                Self::Visible(computed_display)
            }
            Self::Animating(dis) if computed_has_hide => {
                if num_waiting_anim() == 0 {
                    Self::Hidden
                } else {
                    Self::Animating(*dis)
                }
            }
            Self::Hidden if computed_has_hide => Self::Hidden,
            Self::Hidden if !computed_has_hide => {
                add_animations();
                Self::Visible(computed_display)
            }
            _ => unreachable!(),
        };
    }
}

/// Controls view visibility state.
#[derive(Debug, Clone, Copy, Default)]
pub struct Visibility {
    /// The current visibility phase (for enter/exit animations).
    pub phase: VisibilityPhase,
}

impl Visibility {
    /// Returns true if the view should be treated as hidden.
    pub fn is_hidden(&self) -> bool {
        self.phase == VisibilityPhase::Hidden
    }
}
