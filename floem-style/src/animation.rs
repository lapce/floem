//! The engine-side animation system.
//!
//! This module owns the declarative animation data model — keyframes,
//! timing, repeat / reverse modes, the state machine — plus the tick
//! functions the cascade uses to interpolate values per frame.
//!
//! What's **not** here by design:
//! - Imperative controllers (`.start()`, `.pause()`, gesture-linked
//!   values, reactive triggers). Those couple to whatever runtime the
//!   host uses and live in the host crate (for floem, `src/animate`).
//! - Trigger firing during `advance()`. Instead, `advance()` returns an
//!   [`AnimationEvents`] struct so hosts can decide how to propagate
//!   `started`, `visual_completed`, and `completed` transitions to
//!   their own observers.
//!
//! A native host such as floem-native can treat [`Animation`] as pure
//! configuration data (easing, duration, keyframes) and decide at
//! registration time whether to delegate to the platform's compositor
//! animation engine or fall back to this crate's CPU ticker.
//!
//! See the floem-style crate-level docs for the broader engine/host split.

use std::any::Any;
use std::rc::Rc;
use std::time::Duration;

#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;
#[cfg(target_arch = "wasm32")]
use web_time::Instant;

use smallvec::{SmallVec, smallvec};

use crate::easing::{Bezier, Easing, Linear, Spring};
use crate::props::StylePropRef;
use crate::style::Style;
use crate::unit::UnitExt;

// ============================================================================
// KeyFrame building blocks
// ============================================================================

/// Holds a resolved prop, along with the associated frame id and easing function
#[derive(Clone, Debug)]
pub struct KeyFrameProp {
    // The style prop value. Either comes from an animation frame or is
    // pulled from the computed style.
    val: Rc<dyn Any>,
    // The frame id.
    id: u16,
    // Easing used while animating *towards* this keyframe. When this
    // prop is the lower frame in a pair, the easing is taken from the
    // upper frame instead.
    easing: Rc<dyn Easing>,
}

/// Whether a keyframe's style is stored in the frame itself or pulled from the
/// computed style at tick time.
#[derive(Clone, Debug)]
pub enum KeyFrameStyle {
    /// Props pulled from the computed style each tick (retargeting).
    Computed,
    /// Props stored inline with the keyframe.
    Style(Style),
}

impl From<Style> for KeyFrameStyle {
    fn from(value: Style) -> Self {
        Self::Style(value)
    }
}

/// Style properties plus easing for a single keyframe.
#[derive(Clone, Debug)]
pub struct KeyFrame {
    #[allow(unused)]
    id: u16,
    style: KeyFrameStyle,
    easing: Rc<dyn Easing>,
}

impl KeyFrame {
    /// Create a new keyframe at the given frame id.
    pub fn new(id: u16) -> Self {
        Self {
            id,
            style: Style::default().into(),
            easing: Rc::new(Spring::default()),
        }
    }

    /// Apply a style to this keyframe.
    pub fn style(mut self, style: impl Fn(Style) -> Style) -> Self {
        let style = style(Style::new());
        match &mut self.style {
            cs @ KeyFrameStyle::Computed => *cs = style.into(),
            KeyFrameStyle::Style(s) => s.apply_mut(&style),
        }
        self
    }

    /// Use the live computed style instead of a stored style for this
    /// keyframe. Completely overwrites any previously set style.
    pub fn computed_style(mut self) -> Self {
        self.style = KeyFrameStyle::Computed;
        self
    }

    /// Easing used while animating towards this keyframe.
    pub fn ease(mut self, easing: impl Easing + 'static) -> Self {
        self.easing = Rc::new(easing);
        self
    }

    /// Bezier ease-in-out.
    pub fn ease_in_out(self) -> Self {
        self.ease(Bezier::ease_in_out())
    }

    /// Default spring.
    pub fn ease_spring(self) -> Self {
        self.ease(Spring::default())
    }

    /// Linear easing.
    pub fn ease_linear(self) -> Self {
        self.ease(Linear)
    }

    /// Bezier ease-in.
    pub fn ease_in(self) -> Self {
        self.ease(Bezier::ease_in())
    }

    /// Bezier ease-out.
    pub fn ease_out(self) -> Self {
        self.ease(Bezier::ease_out())
    }
}

// ============================================================================
// PropCache
// ============================================================================

/// Holds frame ids and marks whether the frame pulls its props from a stored
/// style or from the computed style.
#[derive(Debug, Clone, Copy, Eq)]
enum PropFrameKind {
    Normal(u16),
    Computed(u16),
}

impl PropFrameKind {
    const fn inner(self) -> u16 {
        match self {
            Self::Normal(val) => val,
            Self::Computed(val) => val,
        }
    }
}

impl PartialOrd for PropFrameKind {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PropFrameKind {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.inner().cmp(&other.inner())
    }
}

impl PartialEq for PropFrameKind {
    fn eq(&self, other: &Self) -> bool {
        self.inner() == other.inner()
    }
}

/// Pair of frame ids spanning a single prop's current interpolation.
#[derive(Debug, Clone, Copy)]
struct PropFrames {
    // The closest frame less than or equal to the target idx.
    lower_idx: Option<PropFrameKind>,
    // The closest frame greater than the target idx.
    upper_idx: Option<PropFrameKind>,
}

/// Lets the animate loop find which keyframes contain a given prop even
/// when the prop appears sparsely across the keyframe set.
#[derive(Debug, Clone, Default)]
pub struct PropCache {
    prop_map: imbl::HashMap<StylePropRef, SmallVec<[PropFrameKind; 5]>>,
    computed_idxs: SmallVec<[u16; 2]>,
}

impl PropCache {
    fn get_prop_frames(&self, prop: StylePropRef, target_idx: u16) -> Option<PropFrames> {
        self.prop_map.get(&prop).map(|frames| {
            match frames.binary_search(&PropFrameKind::Normal(target_idx)) {
                Ok(exact_idx) => {
                    let lower = Some(frames[exact_idx]);
                    let upper = frames.get(exact_idx + 1).copied();
                    PropFrames {
                        lower_idx: lower,
                        upper_idx: upper,
                    }
                }
                Err(pos) => {
                    let lower = if pos > 0 { Some(frames[pos - 1]) } else { None };
                    let upper = frames.get(pos).copied();
                    PropFrames {
                        lower_idx: lower,
                        upper_idx: upper,
                    }
                }
            }
        })
    }

    fn insert_prop(&mut self, prop: StylePropRef, idx: PropFrameKind) {
        match self.prop_map.entry(prop) {
            imbl::hashmap::Entry::Occupied(mut oe) => {
                if let Err(pos) = oe.get().binary_search(&idx) {
                    oe.get_mut().insert(pos, idx)
                }
            }
            imbl::hashmap::Entry::Vacant(ve) => {
                ve.insert(smallvec![idx]);
            }
        }
    }

    fn insert_computed_prop(&mut self, prop: StylePropRef, idx: PropFrameKind) {
        if let imbl::hashmap::Entry::Occupied(mut oe) = self.prop_map.entry(prop) {
            if let Err(pos) = oe.get().binary_search(&idx) {
                oe.get_mut().insert(pos, idx)
            } else {
                unreachable!(
                    "this should err because a computed prop shouldn't be inserted more than once."
                )
            }
        }
    }

    fn remove_prop(&mut self, prop: StylePropRef, idx: u16) {
        if let imbl::hashmap::Entry::Occupied(mut oe) = self.prop_map.entry(prop)
            && let Ok(pos) = oe.get().binary_search(&PropFrameKind::Normal(idx))
        {
            oe.get_mut().remove(pos);
        }
    }

    fn insert_computed(&mut self, idx: u16) {
        if let Err(pos) = self.computed_idxs.binary_search(&idx) {
            self.computed_idxs.insert(pos, idx)
        }
    }

    fn remove_computed(&mut self, idx: u16) {
        if let Ok(pos) = self.computed_idxs.binary_search(&idx) {
            self.computed_idxs.remove(pos);
        }
    }
}

// ============================================================================
// State machine
// ============================================================================

/// Whether the animation is allowed to reverse when the view is being
/// removed/hidden, and whether it's currently doing so.
#[derive(Debug, Clone, Copy)]
pub enum ReverseOnce {
    /// Reversing is disabled for this animation.
    Never,
    /// Reversing is allowed; `Val(true)` means "currently reversing".
    Val(bool),
}

impl ReverseOnce {
    /// Toggle the currently-reversing bit when allowed.
    pub fn set(&mut self, val: bool) {
        if let Self::Val(v) = self {
            *v = val;
        }
    }

    /// Returns `true` if the animation should currently be reversing.
    pub const fn is_rev(self) -> bool {
        match self {
            Self::Never => false,
            Self::Val(v) => v,
        }
    }
}

/// Repeat behavior of an animation.
#[derive(Clone, Debug)]
pub enum RepeatMode {
    /// Loop until stopped externally.
    LoopForever,
    /// Play exactly `times` times, then enter [`AnimStateKind::Completed`].
    Times(usize),
}

#[derive(Debug, Clone)]
pub(crate) enum AnimState {
    Idle,
    Stopped,
    Paused {
        elapsed: Option<Duration>,
    },
    PassInProgress {
        started_on: Instant,
        elapsed: Duration,
    },
    ExtMode {
        started_on: Instant,
        elapsed: Duration,
    },
    PassFinished {
        elapsed: Duration,
        was_in_ext: bool,
    },
    Completed {
        elapsed: Option<Duration>,
        was_reversing: bool,
    },
}

/// Coarse discriminant for an animation's current state.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum AnimStateKind {
    Idle,
    Paused,
    Stopped,
    PassInProgress,
    PassFinished,
    Completed,
}

/// Imperative command that drives the animation state machine.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum AnimStateCommand {
    Pause,
    Resume,
    Start,
    Stop,
    Reverse,
}

/// Events produced by a single call to [`Animation::advance`].
///
/// Hosts inspect these to fire observers (for floem: reactive `Trigger`s;
/// for a native host: platform-specific notifications). Keeping events
/// as a return value — instead of a callback closure on `Animation` —
/// leaves the engine pure data.
#[derive(Debug, Default, Clone, Copy)]
pub struct AnimationEvents {
    /// The animation just entered its first active pass.
    pub started: bool,
    /// The animation's duration just elapsed (easing may still be
    /// settling on `props_in_ext_progress`).
    pub visual_completed: bool,
    /// Every repeat is done; the animation transitioned to
    /// [`AnimStateKind::Completed`].
    pub completed: bool,
}

// ============================================================================
// Animation
// ============================================================================

/// Declarative animation data + state machine. Pure engine side — no
/// reactive hooks and no view-id coupling.
#[derive(Debug, Clone)]
pub struct Animation {
    pub(crate) state: AnimState,
    pub(crate) auto_reverse: bool,
    pub(crate) delay: Duration,
    pub(crate) delay_on_reverse: bool,
    pub(crate) duration: Duration,
    pub(crate) repeat_mode: RepeatMode,
    pub(crate) repeat_count: usize,
    /// Whether the animation should run when the view is being created
    /// (e.g. appearing via a `dyn` container or after being hidden).
    /// Consumed by the host's lifecycle code.
    pub(crate) run_on_remove: bool,
    /// Whether the animation should run when the view is being removed
    /// (hidden / destroyed). Host-driven.
    pub(crate) run_on_create: bool,
    pub(crate) reverse_once: ReverseOnce,
    pub(crate) max_key_frame_num: u16,
    pub(crate) apply_when_finished: bool,
    pub(crate) folded_style: Style,
    pub(crate) key_frames: imbl::HashMap<u16, KeyFrame>,
    pub(crate) props_in_ext_progress:
        imbl::HashMap<StylePropRef, (KeyFrameProp, KeyFrameProp)>,
    pub(crate) cache: PropCache,
    pub(crate) debug_description: Option<String>,
}

impl Default for Animation {
    fn default() -> Self {
        Self {
            state: AnimState::Idle,
            auto_reverse: false,
            delay: Duration::ZERO,
            delay_on_reverse: false,
            duration: Duration::from_millis(200),
            repeat_mode: RepeatMode::Times(1),
            repeat_count: 0,
            run_on_remove: false,
            run_on_create: false,
            reverse_once: ReverseOnce::Val(false),
            max_key_frame_num: 100,
            apply_when_finished: false,
            folded_style: Style::new(),
            cache: Default::default(),
            key_frames: imbl::HashMap::new(),
            props_in_ext_progress: imbl::HashMap::new(),
            debug_description: None,
        }
    }
}

/// # Constructors and quick-setup helpers.
impl Animation {
    /// Empty animation.
    pub fn new() -> Self {
        Self::default()
    }

    /// Configure this animation as a view transition (runs on view
    /// create and remove; keyframes 0 and 100 default to the computed
    /// style).
    pub fn view_transition(self) -> Self {
        self.run_on_create(true)
            .run_on_remove(true)
            .initial_state(AnimStateCommand::Stop)
            .keyframe(0, |f| f.computed_style().ease(Spring::gentle()))
            .keyframe(100, |f| f.computed_style().ease(Spring::gentle()))
    }

    /// View transition with a custom easing on both keyframes.
    pub fn view_transition_with_ease(self, ease: impl Easing + 'static + Clone) -> Self {
        self.view_transition()
            .keyframe(0, |f| f.computed_style().ease(ease.clone()))
            .keyframe(100, |f| f.computed_style().ease(ease.clone()))
    }

    /// View transition animating scale 0% → 100%.
    pub fn scale_effect(self) -> Self {
        self.view_transition()
            .keyframe(0, |f| f.style(|s| s.scale(0.pct())))
            .debug_name("Scale the width and height from zero to the default")
    }

    /// View transition animating size 0×0 → computed.
    pub fn scale_size_effect(self) -> Self {
        self.view_transition()
            .keyframe(0, |f| f.style(|s| s.size(0, 0)))
            .debug_name("Scale the width and height from zero to the default")
    }
}

/// # Builder methods for configuring an `Animation`.
impl Animation {
    /// Build a [`KeyFrame`].
    ///
    /// If a keyframe already exists at `frame_id`, its style is merged
    /// (only set fields override). For full replacement see
    /// [`Animation::keyframe_override`].
    pub fn keyframe(mut self, frame_id: u16, key_frame: impl Fn(KeyFrame) -> KeyFrame) -> Self {
        let frame = key_frame(KeyFrame::new(frame_id));
        if let KeyFrameStyle::Style(ref style) = frame.style {
            self.cache.remove_computed(frame_id);
            for prop in style.style_props() {
                self.cache
                    .insert_prop(prop, PropFrameKind::Normal(frame_id));
            }
        } else {
            self.cache.insert_computed(frame_id);
        }

        match self.key_frames.entry(frame_id) {
            imbl::hashmap::Entry::Occupied(mut oe) => {
                let e_frame = oe.get_mut();
                match (&mut e_frame.style, frame.style) {
                    (KeyFrameStyle::Computed, KeyFrameStyle::Computed) => {}
                    (s @ KeyFrameStyle::Computed, KeyFrameStyle::Style(ns)) => {
                        *s = KeyFrameStyle::Style(ns);
                    }
                    (s @ KeyFrameStyle::Style(_), KeyFrameStyle::Computed) => {
                        *s = KeyFrameStyle::Computed;
                    }
                    (KeyFrameStyle::Style(s), KeyFrameStyle::Style(ns)) => {
                        s.apply_mut(&ns);
                    }
                }
                e_frame.easing = frame.easing;
            }
            imbl::hashmap::Entry::Vacant(ve) => {
                ve.insert(frame);
            }
        }
        self
    }

    /// Build a [`KeyFrame`] replacing any existing one at `frame_id`.
    pub fn keyframe_override(
        mut self,
        frame_id: u16,
        key_frame: impl Fn(KeyFrame) -> KeyFrame,
    ) -> Self {
        let frame = key_frame(KeyFrame::new(frame_id));
        let frame_style = frame.style.clone();
        if let Some(f) = self.key_frames.insert(frame_id, frame)
            && let KeyFrameStyle::Style(style) = f.style
        {
            for prop in style.style_props() {
                self.cache.remove_prop(prop, frame_id);
            }
        }
        if let KeyFrameStyle::Style(style) = frame_style {
            self.cache.insert_computed(frame_id);
            for prop in style.style_props() {
                self.cache
                    .insert_prop(prop, PropFrameKind::Normal(frame_id));
            }
        } else {
            self.cache.remove_computed(frame_id);
        }
        self
    }

    /// Perceived animation duration. The total run extends until all
    /// animating props report `finished` from their easing, which can
    /// exceed `duration` for springs.
    pub const fn duration(mut self, duration: Duration) -> Self {
        self.duration = duration;
        self
    }

    /// Invoke `f` with the current duration so callers can derive values from it.
    pub fn with_duration(self, duration: impl FnOnce(Self, Duration) -> Self) -> Self {
        let d = self.duration;
        duration(self, d)
    }

    /// Conditionally apply `f` when `cond` is `true`.
    pub fn apply_if(self, cond: bool, f: impl FnOnce(Self) -> Self) -> Self {
        if cond { f(self) } else { self }
    }

    /// Run on view create (appearing via dyn container, post-hide).
    pub const fn run_on_create(mut self, run_on_create: bool) -> Self {
        self.run_on_create = run_on_create;
        self
    }

    /// Run on create and NOT on remove.
    pub const fn only_on_create(mut self) -> Self {
        self.run_on_remove = false;
        self.run_on_create = true;
        self
    }

    /// Run on view remove (hidden / destroyed).
    pub const fn run_on_remove(mut self, run_on_remove: bool) -> Self {
        self.run_on_remove = run_on_remove;
        self
    }

    /// Run on remove and NOT on create.
    pub const fn only_on_remove(mut self) -> Self {
        self.run_on_remove = true;
        self.run_on_create = false;
        self
    }

    /// Apply the final keyframe's props even once the animation is
    /// finished.
    pub const fn apply_when_finished(mut self, apply: bool) -> Self {
        self.apply_when_finished = apply;
        self
    }

    /// Enable auto-reverse (reach 100% twice as fast, then animate back).
    pub const fn auto_reverse(mut self, auto_rev: bool) -> Self {
        self.auto_reverse = auto_rev;
        self
    }

    /// Allow the animation to reverse on view remove/hide.
    pub const fn reverse_on_exit(mut self, allow: bool) -> Self {
        if allow {
            self.reverse_once = ReverseOnce::Val(false);
        } else {
            self.reverse_once = ReverseOnce::Never;
        }
        self
    }

    /// Delay before the animation starts.
    pub const fn delay(mut self, delay: Duration) -> Self {
        self.delay = delay;
        self
    }

    /// Also delay when reversing.
    pub const fn delay_on_reverse(mut self, on_reverse: bool) -> Self {
        self.delay_on_reverse = on_reverse;
        self
    }

    /// Loop forever (`true`) or play once (`false`).
    pub const fn repeat(mut self, repeat: bool) -> Self {
        self.repeat_mode = if repeat {
            RepeatMode::LoopForever
        } else {
            RepeatMode::Times(1)
        };
        self
    }

    /// Play exactly `times` times.
    pub const fn repeat_times(mut self, times: usize) -> Self {
        self.repeat_mode = RepeatMode::Times(times);
        self
    }

    /// Keyframe number representing 100% completion. Default is 100.
    /// Increasing this allows more than 100 keyframes (each then
    /// represents a smaller fraction of total progress).
    pub const fn max_key_frame(mut self, max: u16) -> Self {
        self.max_key_frame_num = max;
        self
    }

    /// Apply an initial state command at build time.
    pub fn initial_state(mut self, command: AnimStateCommand) -> Self {
        self.transition(command);
        self
    }

    /// Human-readable description shown in debug output / inspectors.
    pub fn debug_name(mut self, description: impl Into<String>) -> Self {
        match &mut self.debug_description {
            Some(inner_desc) => {
                inner_desc.push_str("; ");
                inner_desc.push_str(&description.into())
            }
            val @ None => *val = Some(description.into()),
        }
        self
    }
}

/// # State inspection.
impl Animation {
    /// Match the current state and return its kind.
    pub const fn state_kind(&self) -> AnimStateKind {
        match self.state {
            AnimState::Idle => AnimStateKind::Idle,
            AnimState::Stopped => AnimStateKind::Stopped,
            AnimState::PassInProgress { .. } => AnimStateKind::PassInProgress,
            AnimState::ExtMode { .. } => AnimStateKind::PassInProgress,
            AnimState::PassFinished { .. } => AnimStateKind::PassFinished,
            AnimState::Completed { .. } => AnimStateKind::Completed,
            AnimState::Paused { .. } => AnimStateKind::Paused,
        }
    }

    /// Total elapsed time since the animation (first) started.
    pub fn elapsed(&self) -> Option<Duration> {
        match &self.state {
            AnimState::Idle | AnimState::Stopped => None,
            AnimState::PassInProgress {
                started_on,
                elapsed,
            }
            | AnimState::ExtMode {
                started_on,
                elapsed,
            } => {
                let duration = Instant::now() - *started_on;
                Some(*elapsed + duration)
            }
            AnimState::PassFinished { elapsed, .. } => Some(*elapsed),
            AnimState::Completed { elapsed, .. } => *elapsed,
            AnimState::Paused { elapsed } => *elapsed,
        }
    }

    /// `true` when `state_kind() == Idle`.
    pub fn is_idle(&self) -> bool {
        self.state_kind() == AnimStateKind::Idle
    }

    /// `true` when a pass is currently running.
    pub fn is_in_progress(&self) -> bool {
        self.state_kind() == AnimStateKind::PassInProgress
    }

    /// `true` when the animation has fully completed.
    pub fn is_completed(&self) -> bool {
        self.state_kind() == AnimStateKind::Completed
    }

    /// `true` when the animation is stopped.
    pub fn is_stopped(&self) -> bool {
        self.state_kind() == AnimStateKind::Stopped
    }

    /// `true` when calling [`Self::advance`] will make progress (either
    /// transition state or interpolate values).
    pub const fn can_advance(&self) -> bool {
        match self.state_kind() {
            AnimStateKind::PassFinished
            | AnimStateKind::PassInProgress
            | AnimStateKind::Idle
            | AnimStateKind::Completed => true,
            AnimStateKind::Paused | AnimStateKind::Stopped => false,
        }
    }

    /// `true` if [`Self::auto_reverse`] is enabled.
    pub const fn is_auto_reverse(&self) -> bool {
        self.auto_reverse
    }

    /// `true` when the folded style should be re-applied even though
    /// [`Self::advance`] won't make progress (paused state, or
    /// `apply_when_finished`).
    pub fn should_apply_folded(&self) -> bool {
        self.apply_when_finished
            || match self.state_kind() {
                AnimStateKind::Paused => true,
                AnimStateKind::Idle
                | AnimStateKind::Stopped
                | AnimStateKind::PassInProgress
                | AnimStateKind::PassFinished
                | AnimStateKind::Completed => false,
            }
    }

    /// Whether the animation is currently playing in reverse.
    pub const fn is_reversing(&self) -> bool {
        self.reverse_once.is_rev()
    }

    /// Whether `run_on_create` is enabled.
    pub const fn runs_on_create(&self) -> bool {
        self.run_on_create
    }

    /// Whether `run_on_remove` is enabled.
    pub const fn runs_on_remove(&self) -> bool {
        self.run_on_remove
    }

    /// Optional debug description set via [`Self::debug_name`].
    pub fn debug_description(&self) -> Option<&str> {
        self.debug_description.as_deref()
    }

    /// Configured perceived duration.
    pub fn get_duration(&self) -> Duration {
        self.duration
    }

    /// Configured repeat mode.
    pub fn get_repeat_mode(&self) -> &RepeatMode {
        &self.repeat_mode
    }
}

/// # Engine tick + state transitions.
impl Animation {
    /// Advance the animation by one frame. Returns which lifecycle
    /// events (if any) fired — hosts propagate these to their own
    /// observers.
    pub fn advance(&mut self) -> AnimationEvents {
        let mut events = AnimationEvents::default();
        let use_delay = self.use_delay();
        match &mut self.state {
            AnimState::Idle => {
                self.start_mut();
                events.started = true;
            }
            AnimState::PassInProgress {
                started_on,
                elapsed,
            } => {
                let now = Instant::now();
                let duration = now - *started_on;
                let og_elapsed = *elapsed;
                *elapsed = duration;

                let temp_elapsed = if *elapsed <= self.delay && use_delay {
                    Duration::ZERO
                } else if use_delay {
                    *elapsed - self.delay
                } else {
                    *elapsed
                };

                if temp_elapsed >= self.duration {
                    if self.props_in_ext_progress.is_empty() {
                        self.state = AnimState::PassFinished {
                            elapsed: *elapsed,
                            was_in_ext: false,
                        };
                    } else {
                        events.visual_completed = true;
                        self.state = AnimState::ExtMode {
                            started_on: *started_on,
                            elapsed: og_elapsed,
                        };
                    }
                }
            }
            AnimState::ExtMode {
                started_on,
                elapsed,
            } => {
                let now = Instant::now();
                let duration = now - *started_on;
                *elapsed = duration;

                if self.props_in_ext_progress.is_empty() {
                    self.state = AnimState::PassFinished {
                        elapsed: *elapsed,
                        was_in_ext: true,
                    };
                }
            }
            AnimState::PassFinished {
                elapsed,
                was_in_ext,
            } => match self.repeat_mode {
                RepeatMode::LoopForever => {
                    if self.reverse_once.is_rev() {
                        self.reverse_once.set(false);
                    } else if self.auto_reverse {
                        self.reverse_once.set(true);
                    }
                    self.state = AnimState::PassInProgress {
                        started_on: Instant::now(),
                        elapsed: Duration::ZERO,
                    }
                }
                RepeatMode::Times(times) => {
                    self.repeat_count += 1;
                    if self.repeat_count >= times {
                        let was_reversing = self.reverse_once.is_rev();
                        self.reverse_once.set(false);
                        events.completed = true;
                        if !*was_in_ext {
                            events.visual_completed = true;
                        }
                        self.state = AnimState::Completed {
                            elapsed: Some(*elapsed),
                            was_reversing,
                        }
                    } else {
                        self.state = AnimState::PassInProgress {
                            started_on: Instant::now(),
                            elapsed: Duration::ZERO,
                        }
                    }
                }
            },
            AnimState::Paused { .. } => {
                debug_assert!(false, "Tried to advance a paused animation")
            }
            AnimState::Stopped => {
                debug_assert!(false, "Tried to advance a stopped animation")
            }
            AnimState::Completed { was_reversing, .. } => {
                if self.auto_reverse && !*was_reversing {
                    self.reverse_mut();
                } else {
                    self.state = AnimState::Stopped;
                }
            }
        }
        events
    }

    /// Apply a state-machine transition command directly.
    pub fn transition(&mut self, command: AnimStateCommand) {
        match command {
            AnimStateCommand::Pause => {
                self.state = AnimState::Paused {
                    elapsed: self.elapsed(),
                }
            }
            AnimStateCommand::Resume => {
                if let AnimState::Paused { elapsed } = &self.state {
                    self.state = AnimState::PassInProgress {
                        started_on: Instant::now(),
                        elapsed: elapsed.unwrap_or(Duration::ZERO),
                    }
                }
            }
            AnimStateCommand::Start => {
                self.reverse_once.set(false);
                Rc::make_mut(&mut self.folded_style.map).clear();
                self.repeat_count = 0;
                self.state = AnimState::PassInProgress {
                    started_on: Instant::now(),
                    elapsed: Duration::ZERO,
                }
            }
            AnimStateCommand::Reverse => {
                self.reverse_once.set(true);
                Rc::make_mut(&mut self.folded_style.map).clear();
                self.repeat_count = 0;
                self.state = AnimState::PassInProgress {
                    started_on: Instant::now(),
                    elapsed: Duration::ZERO,
                }
            }
            AnimStateCommand::Stop => {
                self.repeat_count = 0;
                self.state = AnimState::Stopped;
            }
        }
    }

    fn start_mut(&mut self) {
        self.transition(AnimStateCommand::Start)
    }

    fn reverse_mut(&mut self) {
        self.transition(AnimStateCommand::Reverse)
    }

    /// Total elapsed time normalized to `[0, 1]` (can exceed 1 if duration
    /// has passed and `props_in_ext_progress` is still settling).
    pub fn total_time_percent(&self) -> f64 {
        if self.duration == Duration::ZERO {
            return 0.;
        }
        let mut elapsed = self.elapsed().unwrap_or(Duration::ZERO);
        if self.use_delay() {
            elapsed = elapsed.saturating_sub(self.delay);
        }
        let percent = elapsed.as_secs_f64() / self.duration.as_secs_f64();
        if self.reverse_once.is_rev() {
            1. - percent
        } else {
            percent
        }
    }

    fn use_delay(&self) -> bool {
        !self.is_reversing() || self.delay_on_reverse
    }
}

/// # Interpolation.
impl Animation {
    fn get_current_kf_props(
        &self,
        prop: StylePropRef,
        frame_target: u16,
        computed_style: &Style,
    ) -> Option<(KeyFrameProp, KeyFrameProp)> {
        let PropFrames {
            lower_idx,
            upper_idx,
        } = self.cache.get_prop_frames(prop, frame_target)?;

        let mut upper_computed = false;

        let upper = {
            let upper = upper_idx?;
            let frame = self
                .key_frames
                .get(&upper.inner())
                .expect("If the value is in the cache, it should also be in the key frames");

            let prop = match &frame.style {
                KeyFrameStyle::Computed => {
                    debug_assert!(
                        matches!(upper, PropFrameKind::Computed(_)),
                        "computed frame should have come from matching computed idx"
                    );
                    upper_computed = true;
                    computed_style
                        .map
                        .get(&prop.key)
                        .expect("was in the cache as a computed frame")
                        .clone()
                }
                KeyFrameStyle::Style(s) => s.map.get(&prop.key).expect("same as above").clone(),
            };

            KeyFrameProp {
                id: upper.inner(),
                val: prop,
                easing: frame.easing.clone(),
            }
        };

        let lower = {
            let lower = lower_idx?;
            let frame = self
                .key_frames
                .get(&lower.inner())
                .expect("If the value is in the cache, it should also be in the key frames");

            let prop = match &frame.style {
                KeyFrameStyle::Computed => {
                    debug_assert!(
                        matches!(lower, PropFrameKind::Computed(_)),
                        "computed frame should have come from matching computed idx"
                    );
                    if upper_computed {
                        return None;
                    }
                    computed_style
                        .map
                        .get(&prop.key)
                        .expect("was in the cache as a computed frame")
                        .clone()
                }
                KeyFrameStyle::Style(s) => s.map.get(&prop.key).expect("same as above").clone(),
            };

            KeyFrameProp {
                id: lower.inner(),
                val: prop,
                easing: frame.easing.clone(),
            }
        };

        if self.is_reversing() {
            Some((upper, lower))
        } else {
            Some((lower, upper))
        }
    }

    /// Interpolate every animated prop at the current time and apply
    /// the result to `computed_style`.
    pub fn animate_into(&mut self, computed_style: &mut Style) {
        let computed_idxs = self.cache.computed_idxs.clone();
        for computed_idx in &computed_idxs {
            for prop in computed_style.style_props() {
                self.cache
                    .insert_computed_prop(prop, PropFrameKind::Computed(*computed_idx));
            }
        }
        let local_percents: Vec<_> = self
            .props_in_ext_progress
            .iter()
            .map(|(p, (l, u))| (*p, self.get_local_percent(l.id, u.id)))
            .collect();

        self.props_in_ext_progress.retain(|p, (_l, u)| {
            let local_percent = local_percents
                .iter()
                .find(|&&(prop, _)| prop == *p)
                .map(|&(_, percent)| percent)
                .unwrap_or_default();
            !u.easing.finished(local_percent)
        });
        for (ext_prop, (l, u)) in &self.props_in_ext_progress {
            let local_percent = local_percents
                .iter()
                .find(|&&(prop, _)| prop == *ext_prop)
                .map(|&(_, percent)| percent)
                .unwrap_or_default();

            let eased_time = u.easing.eval(local_percent);
            if let Some(interpolated) =
                (ext_prop.info().interpolate)(&*l.val.clone(), &*u.val.clone(), eased_time)
            {
                Rc::make_mut(&mut self.folded_style.map).insert(ext_prop.key, interpolated);
            }
        }

        let percent = self.total_time_percent();
        let frame_target = (self.max_key_frame_num as f64 * percent).round() as u16;

        let props: Vec<_> = self.cache.prop_map.keys().copied().collect();

        for prop in &props {
            if self.props_in_ext_progress.contains_key(prop) {
                continue;
            }
            let Some((prev, target)) =
                self.get_current_kf_props(*prop, frame_target, computed_style)
            else {
                continue;
            };
            let local_percent = self.get_local_percent(prev.id, target.id);
            let easing = target.easing.clone();
            // TODO: Better way to detect ext-mode entry than just
            // checking after 97%.
            if (local_percent > 0.97) && !easing.finished(local_percent) {
                self.props_in_ext_progress
                    .insert(*prop, (prev.clone(), target.clone()));
            } else {
                self.props_in_ext_progress.remove(prop);
            }
            let eased_time = easing.eval(local_percent);
            if let Some(interpolated) =
                (prop.info().interpolate)(&*prev.val.clone(), &*target.val.clone(), eased_time)
            {
                Rc::make_mut(&mut self.folded_style.map).insert(prop.key, interpolated);
            }
        }

        computed_style.apply_mut(&self.folded_style);

        for computed_idx in computed_idxs {
            for prop in computed_style.style_props() {
                self.cache.remove_prop(prop, computed_idx);
            }
        }
    }

    /// Given a pair of keyframes, return the animation's progress inside
    /// the sub-range they bound.
    pub fn get_local_percent(&self, prev_frame: u16, target_frame: u16) -> f64 {
        let (low_frame, high_frame) = if self.is_reversing() {
            (target_frame as f64, prev_frame as f64)
        } else {
            (prev_frame as f64, target_frame as f64)
        };
        let total_num_frames = self.max_key_frame_num as f64;
        let low_frame_percent = low_frame / total_num_frames;
        let high_frame_percent = high_frame / total_num_frames;
        let keyframe_range = (high_frame_percent.max(0.001) - low_frame_percent.max(0.001)).abs();
        let total_time_percent = self.total_time_percent();
        let local = (total_time_percent - low_frame_percent) / keyframe_range;

        if self.is_reversing() {
            1. - local
        } else {
            local
        }
    }

    /// Apply the last-computed folded style without advancing.
    pub fn apply_folded(&self, computed_style: &mut Style) {
        computed_style.apply_mut(&self.folded_style);
    }
}

/// # Introspection for backends that may offload to native animation
/// engines (iOS CoreAnimation, Android ViewPropertyAnimator, etc.).
impl Animation {
    /// Every prop key this animation currently touches. A native
    /// backend can intersect this set against its compositor-animatable
    /// property list to decide whether to offload or fall back to
    /// CPU ticking.
    pub fn touched_props(&self) -> impl Iterator<Item = StylePropRef> + '_ {
        self.cache.prop_map.keys().copied()
    }
}
