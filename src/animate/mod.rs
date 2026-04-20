#![deny(missing_docs)]

//! Animations
//!
//! This module is a thin reactive wrapper around the engine animation
//! types that live in [`floem_style::animation`]. The engine owns
//! keyframes, the state machine, interpolation math, and easings; this
//! wrapper adds floem-specific pieces the engine can't depend on —
//! reactive triggers for lifecycle callbacks, `RwSignal`-backed command
//! wiring, and `ViewId` routing for imperative `.start()` / `.pause()` /
//! `.state()` controls.

pub use floem_style::easing;
pub use easing::{Bezier, Easing, Linear, Spring, Step, StepPosition};

pub use floem_style::{
    AnimStateCommand, AnimStateKind, AnimationEvents, KeyFrame, KeyFrameStyle, PropCache,
    RepeatMode, ReverseOnce,
};

use crate::{ViewId, view::StackOffset};

use floem_reactive::{RwSignal, SignalGet, Trigger, UpdaterEffect};
use smallvec::SmallVec;

use crate::platform::Duration;

type EffectStateVec = SmallVec<[RwSignal<SmallVec<[(ViewId, StackOffset<Animation>); 1]>>; 1]>;

/// The main animation struct.
///
/// Wraps [`floem_style::Animation`] with floem's reactive lifecycle
/// triggers and signal-based imperative control. Most configuration
/// builders forward directly to the engine; `.start()` / `.pause()` /
/// `.state()` etc. wire reactive command routing that the engine
/// knows nothing about.
#[derive(Debug, Clone)]
pub struct Animation {
    pub(crate) engine: floem_style::Animation,
    pub(crate) effect_states: EffectStateVec,
    /// Fires at the start of each cycle.
    pub(crate) on_start: Trigger,
    /// Fires at the end of the animation's duration (ext-mode may still
    /// be settling spring interpolations after this).
    pub(crate) on_visual_complete: Trigger,
    /// Fires once every repeat is done — prefer [`Self::on_visual_complete`]
    /// for UI gating (e.g. removing a view once the exit animation's
    /// visible phase finishes).
    pub(crate) on_complete: Trigger,
}

impl Default for Animation {
    fn default() -> Self {
        Self {
            engine: floem_style::Animation::default(),
            effect_states: SmallVec::new(),
            on_start: Trigger::new(),
            on_complete: Trigger::new(),
            on_visual_complete: Trigger::new(),
        }
    }
}

/// # Constructors and quick-setup helpers.
impl Animation {
    /// Create a new animation.
    pub fn new() -> Self {
        Self::default()
    }

    /// Quick setup for a view transition (runs on create + remove with
    /// computed-style keyframes at 0 and 100).
    pub fn view_transition(mut self) -> Self {
        self.engine = self.engine.view_transition();
        self
    }

    /// View transition with a custom easing on both endpoints.
    pub fn view_transition_with_ease(mut self, ease: impl Easing + 'static + Clone) -> Self {
        self.engine = self.engine.view_transition_with_ease(ease);
        self
    }

    /// View transition animating scale 0% → 100%.
    pub fn scale_effect(mut self) -> Self {
        self.engine = self.engine.scale_effect();
        self
    }

    /// View transition animating size 0×0 → computed.
    pub fn scale_size_effect(mut self) -> Self {
        self.engine = self.engine.scale_size_effect();
        self
    }
}

/// # Builder methods that forward to the engine.
impl Animation {
    /// Build a keyframe (merge into an existing one at the same id).
    pub fn keyframe(mut self, frame_id: u16, key_frame: impl Fn(KeyFrame) -> KeyFrame) -> Self {
        self.engine = self.engine.keyframe(frame_id, key_frame);
        self
    }

    /// Build a keyframe, replacing any existing one at the same id.
    pub fn keyframe_override(
        mut self,
        frame_id: u16,
        key_frame: impl Fn(KeyFrame) -> KeyFrame,
    ) -> Self {
        self.engine = self.engine.keyframe_override(frame_id, key_frame);
        self
    }

    /// Perceived animation duration.
    pub fn duration(mut self, duration: Duration) -> Self {
        self.engine = self.engine.duration(duration);
        self
    }

    /// Access the current duration while setting further properties.
    pub fn with_duration(self, duration: impl FnOnce(Self, Duration) -> Self) -> Self {
        let d = self.engine.get_duration();
        duration(self, d)
    }

    /// Conditionally apply `f`.
    pub fn apply_if(self, cond: bool, f: impl FnOnce(Self) -> Self) -> Self {
        if cond { f(self) } else { self }
    }

    /// Run on view create (via dyn container, post-hide).
    pub fn run_on_create(mut self, run_on_create: bool) -> Self {
        self.engine = self.engine.run_on_create(run_on_create);
        self
    }

    /// Run on create and NOT on remove.
    pub fn only_on_create(mut self) -> Self {
        self.engine = self.engine.only_on_create();
        self
    }

    /// Run on view remove.
    pub fn run_on_remove(mut self, run_on_remove: bool) -> Self {
        self.engine = self.engine.run_on_remove(run_on_remove);
        self
    }

    /// Run on remove and NOT on create.
    pub fn only_on_remove(mut self) -> Self {
        self.engine = self.engine.only_on_remove();
        self
    }

    /// Apply the final keyframe even once the animation is finished.
    pub fn apply_when_finished(mut self, apply: bool) -> Self {
        self.engine = self.engine.apply_when_finished(apply);
        self
    }

    /// Enable auto-reverse.
    pub fn auto_reverse(mut self, auto_rev: bool) -> Self {
        self.engine = self.engine.auto_reverse(auto_rev);
        self
    }

    /// Allow reversing on view remove / hide.
    pub fn reverse_on_exit(mut self, allow: bool) -> Self {
        self.engine = self.engine.reverse_on_exit(allow);
        self
    }

    /// Delay before starting.
    pub fn delay(mut self, delay: Duration) -> Self {
        self.engine = self.engine.delay(delay);
        self
    }

    /// Also delay when reversing.
    pub fn delay_on_reverse(mut self, on_reverse: bool) -> Self {
        self.engine = self.engine.delay_on_reverse(on_reverse);
        self
    }

    /// Loop forever (`true`) or play once (`false`).
    pub fn repeat(mut self, repeat: bool) -> Self {
        self.engine = self.engine.repeat(repeat);
        self
    }

    /// Play exactly `times` times.
    pub fn repeat_times(mut self, times: usize) -> Self {
        self.engine = self.engine.repeat_times(times);
        self
    }

    /// Keyframe number representing 100% completion (default 100).
    pub fn max_key_frame(mut self, max: u16) -> Self {
        self.engine = self.engine.max_key_frame(max);
        self
    }

    /// Apply an initial state command at build time.
    pub fn initial_state(mut self, command: AnimStateCommand) -> Self {
        self.engine = self.engine.initial_state(command);
        self
    }

    /// Human-readable debug description.
    pub fn debug_name(mut self, description: impl Into<String>) -> Self {
        self.engine = self.engine.debug_name(description);
        self
    }
}

/// # Reactive lifecycle callbacks (floem-only).
impl Animation {
    /// Provides access to the on-create [`Trigger`].
    pub fn on_create(self, on_create: impl FnOnce(Trigger) + 'static) -> Self {
        on_create(self.on_start);
        self
    }

    /// Provides access to the on-visual-complete [`Trigger`].
    pub fn on_visual_complete(self, on_visual_complete: impl FnOnce(Trigger) + 'static) -> Self {
        on_visual_complete(self.on_visual_complete);
        self
    }

    /// Provides access to the on-complete [`Trigger`].
    pub fn on_complete(self, on_complete: impl FnOnce(Trigger) + 'static) -> Self {
        on_complete(self.on_complete);
        self
    }
}

/// # Reactive state control (floem-only).
impl Animation {
    /// Route state commands through a reactive effect. The animation
    /// receives `command()` each time any reactive value it touches
    /// changes. `apply_initial` controls whether the initial call is
    /// executed at build time.
    pub fn state(
        mut self,
        command: impl Fn() -> AnimStateCommand + 'static,
        apply_initial: bool,
    ) -> Self {
        let states = RwSignal::new(SmallVec::new());
        self.effect_states.push(states);
        let initial_command = UpdaterEffect::new(command, move |command| {
            for (view_id, stack_offset) in states.get_untracked() {
                view_id.update_animation_state(stack_offset, command)
            }
        });
        if apply_initial {
            self.engine.transition(initial_command);
        }
        self
    }

    /// Pause when the trigger tracks a reactive update.
    pub fn pause(self, trigger: impl Fn() + 'static) -> Self {
        self.state(
            move || {
                trigger();
                AnimStateCommand::Pause
            },
            false,
        )
    }

    /// Resume when the trigger tracks a reactive update.
    pub fn resume(self, trigger: impl Fn() + 'static) -> Self {
        self.state(
            move || {
                trigger();
                AnimStateCommand::Resume
            },
            false,
        )
    }

    /// Start when the trigger tracks a reactive update.
    pub fn start(self, trigger: impl Fn() + 'static) -> Self {
        self.state(
            move || {
                trigger();
                AnimStateCommand::Start
            },
            false,
        )
    }

    /// Start reversing when the trigger tracks a reactive update.
    pub fn reverse(self, trigger: impl Fn() + 'static) -> Self {
        self.state(
            move || {
                trigger();
                AnimStateCommand::Reverse
            },
            false,
        )
    }

    /// Stop when the trigger tracks a reactive update.
    pub fn stop(self, trigger: impl Fn() + 'static) -> Self {
        self.state(
            move || {
                trigger();
                AnimStateCommand::Stop
            },
            false,
        )
    }
}

/// # Engine tick — advance the animation and route lifecycle events to
/// the reactive [`Trigger`]s.
impl Animation {
    /// Advance the animation one frame, routing engine events through
    /// the reactive triggers.
    pub fn advance(&mut self) {
        let events = self.engine.advance();
        if events.started {
            self.on_start.notify();
        }
        if events.visual_completed {
            self.on_visual_complete.notify();
        }
        if events.completed {
            self.on_complete.notify();
        }
    }

    /// Apply an imperative state-machine transition.
    pub(crate) fn transition(&mut self, command: AnimStateCommand) {
        self.engine.transition(command);
    }

    /// Interpolate animated props into `computed_style`.
    pub fn animate_into(&mut self, computed_style: &mut crate::style::Style) {
        self.engine.animate_into(computed_style);
    }

    /// Apply the last-computed folded style.
    pub fn apply_folded(&self, computed_style: &mut crate::style::Style) {
        self.engine.apply_folded(computed_style);
    }

    /// Current state discriminant.
    pub const fn state_kind(&self) -> AnimStateKind {
        self.engine.state_kind()
    }

    /// Elapsed time since (first) start.
    pub fn elapsed(&self) -> Option<Duration> {
        self.engine.elapsed()
    }

    /// `true` when state is [`AnimStateKind::Idle`].
    pub fn is_idle(&self) -> bool {
        self.engine.is_idle()
    }

    /// `true` when a pass is in progress.
    pub fn is_in_progress(&self) -> bool {
        self.engine.is_in_progress()
    }

    /// `true` when the animation has fully completed.
    pub fn is_completed(&self) -> bool {
        self.engine.is_completed()
    }

    /// `true` when the animation is stopped.
    pub fn is_stopped(&self) -> bool {
        self.engine.is_stopped()
    }

    /// `true` when calling [`Self::advance`] will make progress.
    pub const fn can_advance(&self) -> bool {
        self.engine.can_advance()
    }

    /// `true` when [`Animation::auto_reverse`] is enabled.
    pub const fn is_auto_reverse(&self) -> bool {
        self.engine.is_auto_reverse()
    }

    /// `true` when the folded style should be re-applied even though
    /// [`Self::advance`] won't progress (paused state, or
    /// `apply_when_finished`).
    pub fn should_apply_folded(&self) -> bool {
        self.engine.should_apply_folded()
    }

    /// Whether `run_on_create` is enabled.
    pub const fn runs_on_create(&self) -> bool {
        self.engine.runs_on_create()
    }

    /// Whether `run_on_remove` is enabled.
    pub const fn runs_on_remove(&self) -> bool {
        self.engine.runs_on_remove()
    }

    /// The configured repeat mode.
    pub fn repeat_mode(&self) -> &RepeatMode {
        self.engine.get_repeat_mode()
    }

    /// Imperatively transition the animation to a running state.
    pub(crate) fn start_mut(&mut self) {
        self.engine.transition(AnimStateCommand::Start);
    }

    /// Imperatively transition the animation into reverse playback.
    pub(crate) fn reverse_mut(&mut self) {
        self.engine.transition(AnimStateCommand::Reverse);
    }
}
