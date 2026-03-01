#![deny(missing_docs)]

//! Animations

pub mod easing;

pub use easing::{Bezier, Easing, Linear, Spring, Step, StepPosition};

use crate::{
    ViewId,
    style::{Style, StylePropRef},
    unit::UnitExt,
    view::StackOffset,
};

use std::any::Any;
use std::rc::Rc;

use crate::platform::{Duration, Instant};
use floem_reactive::{RwSignal, SignalGet, Trigger, UpdaterEffect};
use smallvec::{SmallVec, smallvec};

/// Holds a resolved prop, along with the associated frame id and easing function
#[derive(Clone, Debug)]
pub struct KeyFrameProp {
    // the style prop value. This will either come from an animation frameor it will be pulled from the computed style
    val: Rc<dyn Any>,
    // the frame id
    id: u16,
    /// This easing will be used while animating towards this keyframe. while this prop is the lower one this easing function will not be used.
    easing: Rc<dyn Easing>,
}

/// Defines whether the style in a key frame should be stored in the frame or it it should be pulled from the computed style
#[derive(Clone, Debug)]
pub enum KeyFrameStyle {
    /// when computed style, props will be pulled from the computed style
    Computed,
    /// When using style, the props will be stored in the key frame
    Style(Style),
}
impl From<Style> for KeyFrameStyle {
    fn from(value: Style) -> Self {
        Self::Style(value)
    }
}

/// Holds the style properties for a keyframe as well as the easing function that should be used when animating towards this frame
#[derive(Clone, Debug)]
pub struct KeyFrame {
    #[allow(unused)]
    /// the key frame id. should be less than the maximum key frame number for a given animation
    id: u16,
    style: KeyFrameStyle,
    /// This easing will be used while animating towards this keyframe.
    easing: Rc<dyn Easing>,
}
impl KeyFrame {
    /// Create a new keyframe with the given id
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
            KeyFrameStyle::Style(s) => s.apply_mut(style),
        }
        self
    }

    /// Set this keyframe to pull its props from the computed style. The will completely overwrite any previously applied styles to this keyframe.
    pub fn computed_style(mut self) -> Self {
        self.style = KeyFrameStyle::Computed;
        self
    }

    /// This easing function will be used while animating towards this keyframe
    pub fn ease(mut self, easing: impl Easing + 'static) -> Self {
        self.easing = Rc::new(easing);
        self
    }

    /// Sets the easing function to the bezier ease in and out
    pub fn ease_in_out(self) -> Self {
        self.ease(Bezier::ease_in_out())
    }

    /// Sets the easing function to the default spring
    pub fn ease_spring(self) -> Self {
        self.ease(Spring::default())
    }

    /// Sets the easing function to a linear easing
    pub fn ease_linear(self) -> Self {
        self.ease(Linear)
    }

    /// Sets the easing function to the bezier ease in
    pub fn ease_in(self) -> Self {
        self.ease(Bezier::ease_in())
    }

    /// Sets the easing function to the bezier ease out
    pub fn ease_out(self) -> Self {
        self.ease(Bezier::ease_out())
    }
}

/// Holds frame ids and marks if the frame is supposed to pull its props from a style or from the computed style
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

/// Holds the pair of frame ids that a single prop is animating between
#[derive(Debug, Clone, Copy)]
struct PropFrames {
    // the closeset frame to the target idx that is less than or equal to current
    lower_idx: Option<PropFrameKind>,
    // the closeset frame to the target idx that is greater than current
    upper_idx: Option<PropFrameKind>,
}

/// This cache enables looking up which keyframes contain a given prop, enabling animation of individual props,
/// even if they are sparsely located in the keyframes, with multiple keyframes between each instance of the prop
#[derive(Debug, Clone, Default)]
pub(crate) struct PropCache {
    /// A map of style properties to a list of all frame ids containing that prop
    prop_map: imbl::HashMap<StylePropRef, SmallVec<[PropFrameKind; 5]>>,
    /// a cached list of all keyframes that use the computed style instead of a separate style
    computed_idxs: SmallVec<[u16; 2]>,
}
impl PropCache {
    /// Find the pair of frames for a given prop at some given target index.
    /// This will find the pair of frames with one lower than the target and one higher than the target.
    /// If it cannot find both, it returns none.
    fn get_prop_frames(&self, prop: StylePropRef, target_idx: u16) -> Option<PropFrames> {
        self.prop_map.get(&prop).map(|frames| {
            match frames.binary_search(&PropFrameKind::Normal(target_idx)) {
                Ok(exact_idx) => {
                    // Exact match found: lower is the exact match, upper is the next frame if it exists
                    let lower = Some(frames[exact_idx]);
                    let upper = frames.get(exact_idx + 1).copied();
                    PropFrames {
                        lower_idx: lower,
                        upper_idx: upper,
                    }
                }
                Err(pos) => {
                    // No exact match found
                    let lower = if pos > 0 {
                        Some(frames[pos - 1]) // Largest smaller frame
                    } else {
                        None
                    };
                    let upper = frames.get(pos).copied(); // Smallest larger frame, if it exists
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
        // computed props are inserted at the start of each call of `animate_into`.
        // Therefore, if the cache does not already contain references to a prop, there will be nothing to animate between and we just don't insert anything.
        if let imbl::hashmap::Entry::Occupied(mut oe) = self.prop_map.entry(prop) {
            if let Err(pos) = oe.get().binary_search(&idx) {
                oe.get_mut().insert(pos, idx)
            } else {
                unreachable!(
                    "this should err because a computed prop shouldn't be inserted more than once. "
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

    // mark a frame id as for a computed style
    fn insert_computed(&mut self, idx: u16) {
        if let Err(pos) = self.computed_idxs.binary_search(&idx) {
            self.computed_idxs.insert(pos, idx)
        }
    }

    // removed a frame id from being marked as for a computed style
    fn remove_computed(&mut self, idx: u16) {
        if let Ok(pos) = self.computed_idxs.binary_search(&idx) {
            self.computed_idxs.remove(pos);
        }
    }
}

/// Holds the allowance and state of the reverse once property of an animation.
/// Reversing an animation is attempted when animation is being removed or hidden.
#[derive(Debug, Clone, Copy)]
pub enum ReverseOnce {
    /// When `Never`, the animation will not be allowed to be set to be in reverse mode
    Never,
    /// When `Val`, the animation is allowed to be set to reverse until finished.
    /// When `Val(true)` the animation will actually reverse
    Val(bool),
}
impl ReverseOnce {
    /// If the reverse once is not `Never` this will set the animation to start or end reversing until finished
    pub fn set(&mut self, val: bool) {
        if let Self::Val(v) = self {
            *v = val;
        }
    }

    /// return true if the animation should be reversing
    pub const fn is_rev(self) -> bool {
        match self {
            Self::Never => false,
            Self::Val(v) => v,
        }
    }
}

/// The mode to specify how the animation should repeat. See also [`Animation::advance`]
#[derive(Clone, Debug)]
pub enum RepeatMode {
    // Once started, the animation will juggle between [`AnimState::PassInProgress`] and [`AnimState::PassFinished`],
    // but will never reach [`AnimState::Completed`]
    /// Repeat the animation forever
    LoopForever,
    // On every pass, we animate until `elapsed >= duration`, then we reset elapsed time to 0 and increment `repeat_count` is
    // increased by 1. This process is repeated until `repeat_count >= times`, and then the animation is set
    // to [`AnimState::Completed`].
    /// Repeat the animation the specified number of times before the animation enters a Complete state
    Times(usize),
}

#[derive(Debug, Clone)]
pub(crate) enum AnimState {
    Idle,
    Stopped,
    Paused {
        elapsed: Option<Duration>,
    },
    /// How many passes(loops) there will be is controlled by the [`RepeatMode`] of the animation.
    /// By default, the animation will only have a single pass,
    /// but it can be set to [`RepeatMode::LoopForever`] to loop indefinitely.
    PassInProgress {
        started_on: Instant,
        elapsed: Duration,
    },
    ExtMode {
        started_on: Instant,
        elapsed: Duration,
    },
    /// Depending on the [`RepeatMode`] of the animation, we either go back to `PassInProgress`
    /// or advance to `Completed`.
    PassFinished {
        elapsed: Duration,
        was_in_ext: bool,
    },
    // NOTE: If animation has `RepeatMode::LoopForever`, this state will never be reached.
    Completed {
        elapsed: Option<Duration>,
        was_reversing: bool,
    },
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
/// Represents the different states an animation can be in.
pub enum AnimStateKind {
    /// The animation is idle and has not started yet.
    Idle,
    /// The animation is paused and can be resumed.
    Paused,
    /// The animation is stopped and cannot be resumed.
    Stopped,
    /// The animation is currently in progress.
    ///
    /// In this state the animation is actively animating the properties of the view.
    PassInProgress,
    /// The animation has finished a pass but may repeat based on the repeat mode.
    PassFinished,
    /// The animation has completed all its passes and will not run again until started.
    Completed,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
/// Commands to control the state of an animation
pub enum AnimStateCommand {
    /// Pause the animation
    Pause,
    /// Resume the animation
    Resume,
    /// Start the animation
    Start,
    /// Stop the animation
    Stop,
    /// Start the animation in reverse
    Reverse,
}

type EffectStateVec = SmallVec<[RwSignal<SmallVec<[(ViewId, StackOffset<Animation>); 1]>>; 1]>;

/// The main animation struct
///
/// Use [`Animation::new`] or the [`Decorators::animation`](crate::views::Decorators::animation) method to build an animation.
#[derive(Debug, Clone)]
pub struct Animation {
    pub(crate) state: AnimState,
    pub(crate) effect_states: EffectStateVec,
    pub(crate) auto_reverse: bool,
    pub(crate) delay: Duration,
    pub(crate) delay_on_reverse: bool,
    pub(crate) duration: Duration,
    pub(crate) repeat_mode: RepeatMode,
    /// How many times the animation has been repeated so far
    pub(crate) repeat_count: usize,
    /// run on remove and run on create should be checked for and respected by any view that dynamically creates sub views
    pub(crate) run_on_remove: bool,
    pub(crate) run_on_create: bool,
    pub(crate) reverse_once: ReverseOnce,
    pub(crate) max_key_frame_num: u16,
    pub(crate) apply_when_finished: bool,
    pub(crate) folded_style: Style,
    pub(crate) key_frames: imbl::HashMap<u16, KeyFrame>,
    // frames should be added to this if when they are the lower frame, they return not done. check/run them before other frames
    pub(crate) props_in_ext_progress: imbl::HashMap<StylePropRef, (KeyFrameProp, KeyFrameProp)>,
    pub(crate) cache: PropCache,
    /// This will fire at the start of each cycle of an animation.
    pub(crate) on_start: Trigger,
    /// This trigger will fire at the completion of an animations duration.
    /// Animations are allowed to go on for longer than their duration, until the easing reports finished.
    /// When waiting for the completion of an animation (such as to remove a view), this trigger should be preferred.
    pub(crate) on_visual_complete: Trigger,
    /// This trigger will fire at the total completion of an animation when the easing function of all props report finished.
    pub(crate) on_complete: Trigger,
    pub(crate) debug_description: Option<String>,
}
impl Default for Animation {
    fn default() -> Self {
        Self {
            state: AnimState::Idle,
            effect_states: SmallVec::new(),
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
            on_start: Trigger::new(),
            on_complete: Trigger::new(),
            on_visual_complete: Trigger::new(),
            debug_description: None,
        }
    }
}

/// # Methods for creating an animation, including methods that quickly initialize the animation for specific uses
impl Animation {
    /// Create a new animation
    pub fn new() -> Self {
        Self::default()
    }

    /// Quickly set a few properties on an animation to set up an animation to be used as a view transition (on creation and removal).
    /// (Sets keyframes 0 and 100 to use the computed style until overridden)
    pub fn view_transition(self) -> Self {
        self.run_on_create(true)
            .run_on_remove(true)
            .initial_state(AnimStateCommand::Stop)
            .keyframe(0, |f| f.computed_style().ease(Spring::gentle()))
            .keyframe(100, |f| f.computed_style().ease(Spring::gentle()))
    }

    /// Quickly set an animation to be a view transition and override the default easing function on keyframes 0 and 100.
    pub fn view_transition_with_ease(self, ease: impl Easing + 'static + Clone) -> Self {
        self.view_transition()
            .keyframe(0, |f| f.computed_style().ease(ease.clone()))
            .keyframe(100, |f| f.computed_style().ease(ease.clone()))
    }

    /// Quickly set an animation to be a view transition and set the animation to animate from scale 0% to the "normal" computed style of a view (the view with no animations applied).
    pub fn scale_effect(self) -> Self {
        self.view_transition()
            .keyframe(0, |f| f.style(|s| s.scale(0.pct())))
            .debug_name("Scale the width and height from zero to the default")
    }

    /// Quickly set an animation to be a view transition and set the animation to animate from size(0, 0) to the "normal" computed style of a view (the view with no animations applied).
    pub fn scale_size_effect(self) -> Self {
        self.view_transition()
            .keyframe(0, |f| f.style(|s| s.size(0, 0)))
            .debug_name("Scale the width and height from zero to the default")
    }
}

/// # Methods for setting properties on an `Animation`
impl Animation {
    /// Build a [`KeyFrame`]
    ///
    /// If there is a matching keyframe id, the style in this keyframe will only override the style values in the new style.
    /// If you want the style to completely override style see [`Animation::keyframe_override`].
    pub fn keyframe(mut self, frame_id: u16, key_frame: impl Fn(KeyFrame) -> KeyFrame) -> Self {
        let frame = key_frame(KeyFrame::new(frame_id));
        if let KeyFrameStyle::Style(ref style) = frame.style {
            // this frame id now contains a style, so remove this frame id from being marked as computed (if it was).
            self.cache.remove_computed(frame_id);
            for prop in style.style_props() {
                // mark that this frame contains the referenced props
                self.cache
                    .insert_prop(prop, PropFrameKind::Normal(frame_id));
            }
        } else {
            self.cache.insert_computed(frame_id);
        }

        // mutate this keyframe's style to be updated with the new style
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
                        s.apply_mut(ns);
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

    /// Build and overwrite a [`KeyFrame`]
    ///
    /// If there is a matching keyframe id, the style in this keyframe will completely override the style in the frame that already exists.
    /// If you want the style to only override the new values see [`Animation::keyframe`].
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

    /// Sets the perceived duration of the animation.
    ///
    /// The total duration of an animation will run until all animating props return `finished`.
    /// This is useful for spring animations which don't conform well to strict ending times.
    pub const fn duration(mut self, duration: Duration) -> Self {
        self.duration = duration;
        self
    }

    /// Set properties on the animation while having access to the current duration.
    pub fn with_duration(self, duration: impl FnOnce(Self, Duration) -> Self) -> Self {
        let d = self.duration;
        duration(self, d)
    }

    /// Conditionally apply properties to this animation if the condition is `true`.
    pub fn apply_if(self, cond: bool, f: impl FnOnce(Self) -> Self) -> Self {
        if cond { f(self) } else { self }
    }

    /// Provides access to the on create trigger by calling the closure in once and then returning self.
    pub fn on_create(self, on_create: impl FnOnce(Trigger) + 'static) -> Self {
        on_create(self.on_start);
        self
    }

    /// Provides access to the on visual complete trigger by calling the closure once and then returning self.
    pub fn on_visual_complete(self, on_visual_complete: impl FnOnce(Trigger) + 'static) -> Self {
        on_visual_complete(self.on_visual_complete);
        self
    }

    /// Provides access to the on complete trigger by calling the closure once and then returning self.
    pub fn on_complete(self, on_complete: impl FnOnce(Trigger) + 'static) -> Self {
        on_complete(self.on_complete);
        self
    }

    /// Set whether this animation should run when being created.
    ///
    /// I.e when being created by a dyn container or when being shown after being hidden.
    pub const fn run_on_create(mut self, run_on_create: bool) -> Self {
        self.run_on_create = run_on_create;
        self
    }

    /// Set whether this animation should run when being created and not when being removed.
    pub const fn only_on_create(mut self) -> Self {
        self.run_on_remove = false;
        self.run_on_create = true;
        self
    }

    /// Set whether this animation should run when being removed.
    /// I.e when being removed by a dyn container or when being hidden.
    pub const fn run_on_remove(mut self, run_on_remove: bool) -> Self {
        self.run_on_remove = run_on_remove;
        self
    }

    /// Set whether this animation should run when being removed and not when being created.
    pub const fn only_on_remove(mut self) -> Self {
        self.run_on_remove = true;
        self.run_on_create = false;
        self
    }

    /// Set whether the properties from the final keyframe of this animation should be applied even when the animation is finished.
    pub const fn apply_when_finished(mut self, apply: bool) -> Self {
        self.apply_when_finished = apply;
        self
    }

    /// Sets if this animation should auto reverse.
    /// If true, the animation will reach the final key frame twice as fast and then animate backwards
    pub const fn auto_reverse(mut self, auto_rev: bool) -> Self {
        self.auto_reverse = auto_rev;
        self
    }

    /// Sets if this animation should be allowed to be reversed when the view is being removed or hidden.
    pub const fn reverse_on_exit(mut self, allow: bool) -> Self {
        if allow {
            self.reverse_once = ReverseOnce::Val(false);
        } else {
            self.reverse_once = ReverseOnce::Never;
        }
        self
    }

    /// Sets a delay for how long the animation should wait before starting.
    pub const fn delay(mut self, delay: Duration) -> Self {
        self.delay = delay;
        self
    }

    /// Sets whether the animation should delay when reversing.
    pub const fn delay_on_reverse(mut self, on_reverse: bool) -> Self {
        self.delay_on_reverse = on_reverse;
        self
    }

    /// Sets if the animation should the repeat forever.
    pub const fn repeat(mut self, repeat: bool) -> Self {
        self.repeat_mode = if repeat {
            RepeatMode::LoopForever
        } else {
            RepeatMode::Times(1)
        };
        self
    }

    /// Sets the number of times the animation should repeat.
    pub const fn repeat_times(mut self, times: usize) -> Self {
        self.repeat_mode = RepeatMode::Times(times);
        self
    }

    /// This is used to determine which keyframe is at 100% completion.
    ///
    /// The default is 100.
    ///
    /// If you need more than 100 keyframes, increase this number, but be aware, the keyframe numbers will then be as a percentage of the maximum.
    ///
    /// *This does not move existing keyframes.*
    pub const fn max_key_frame(mut self, max: u16) -> Self {
        self.max_key_frame_num = max;
        self
    }

    /// Mutably sets the initial state of the animation
    pub fn initial_state(mut self, command: AnimStateCommand) -> Self {
        self.transition(command);
        self
    }

    /// If `apply_initial` is false the initial command will not be applied to the animation.
    /// This is useful if you want the effect to be subscribed to changes but not run the first time.
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
            self.transition(initial_command);
        }
        self
    }

    /// The animation will receive a pause command any time the trigger function tracks any reactive updates.
    pub fn pause(self, trigger: impl Fn() + 'static) -> Self {
        self.state(
            move || {
                trigger();
                AnimStateCommand::Pause
            },
            false,
        )
    }

    /// The animation will receive a resume command any time the trigger function tracks any reactive updates.
    pub fn resume(self, trigger: impl Fn() + 'static) -> Self {
        self.state(
            move || {
                trigger();
                AnimStateCommand::Resume
            },
            false,
        )
    }

    /// The animation will receive a start command any time the trigger function tracks any reactive updates.
    pub fn start(self, trigger: impl Fn() + 'static) -> Self {
        self.state(
            move || {
                trigger();
                AnimStateCommand::Start
            },
            false,
        )
    }

    /// The animation will receive a reverse command any time the trigger function tracks any reactive updates.
    ///
    /// This will start the animation in reverse
    pub fn reverse(self, trigger: impl Fn() + 'static) -> Self {
        self.state(
            move || {
                trigger();
                AnimStateCommand::Reverse
            },
            false,
        )
    }

    /// The animation will receive a stop command any time the trigger function tracks any reactive updates.
    pub fn stop(self, trigger: impl Fn() + 'static) -> Self {
        self.state(
            move || {
                trigger();
                AnimStateCommand::Stop
            },
            false,
        )
    }

    /// Add a debug description to the animation
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

    #[allow(unused)]
    pub(crate) fn pause_mut(mut self) {
        self.transition(AnimStateCommand::Pause)
    }

    #[allow(unused)]
    pub(crate) fn resume_mut(mut self) {
        self.transition(AnimStateCommand::Resume)
    }

    pub(crate) fn start_mut(&mut self) {
        self.transition(AnimStateCommand::Start)
    }

    pub(crate) fn reverse_mut(&mut self) {
        self.transition(AnimStateCommand::Reverse)
    }

    #[allow(unused)]
    pub(crate) fn stop_mut(&mut self) {
        self.transition(AnimStateCommand::Stop)
    }

    /// Matches the current state of the animation and returns the kind of state it is in.
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

    /// Returns the current amount of time that has elapsed since the animation started.
    pub fn elapsed(&self) -> Option<Duration> {
        match &self.state {
            AnimState::Idle => None,
            AnimState::Stopped => None,
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

    /// Advance the animation.
    pub fn advance(&mut self) {
        let use_delay = self.use_delay();
        match &mut self.state {
            AnimState::Idle => {
                self.start_mut();
                self.on_start.notify();
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
                    // The animation hasn't started yet
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
                        self.on_visual_complete.notify();
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
                        self.on_complete.notify();
                        if !*was_in_ext {
                            self.on_visual_complete.notify();
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
    }

    pub(crate) fn transition(&mut self, command: AnimStateCommand) {
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
                self.folded_style.map.clear();
                self.repeat_count = 0;
                self.state = AnimState::PassInProgress {
                    started_on: Instant::now(),
                    elapsed: Duration::ZERO,
                }
            }
            AnimStateCommand::Reverse => {
                self.reverse_once.set(true);
                self.folded_style.map.clear();
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

    /// Get the total time the animation has been running as a percent (0. - 1.)
    pub(crate) fn total_time_percent(&self) -> f64 {
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

    fn is_reversing(&self) -> bool {
        self.reverse_once.is_rev()
    }

    fn use_delay(&self) -> bool {
        // going forward or if we are still supposed to delay on reverse
        !self.is_reversing() || self.delay_on_reverse
    }

    /// Get the lower and upper keyframe ids from the cache for a prop and then resolve those id's into a pair of `KeyFrameProp`s that contain the prop value and easing function
    pub(crate) fn get_current_kf_props(
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
                        // both computed. nothing to animate
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

    /// While advancing, this function can mutably apply it's animated props to a style.
    pub fn animate_into(&mut self, computed_style: &mut Style) {
        // TODO: OPTIMIZE. I've tried to make this efficient, but it would be good to work this over for eficiency because it is called on every frame during an animation.
        // Some work is repeated and could be improved.

        let computed_idxs = self.cache.computed_idxs.clone();
        for computed_idx in &computed_idxs {
            // we add all of the props from the computed style to the cache because the computed style could change inbetween every frame.
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
                self.folded_style.map.insert(ext_prop.key, interpolated);
            }
        }

        let percent = self.total_time_percent();
        let frame_target = (self.max_key_frame_num as f64 * percent).round() as u16;

        let props = self.cache.prop_map.keys();

        for prop in props {
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
            // TODO: Find a better way to find when an animation should enter ext mode rather than just starting to check after 97%.
            // this could miss getting a prop into ext mode
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
                self.folded_style.map.insert(prop.key, interpolated);
            }
        }

        computed_style.apply_mut(self.folded_style.clone());

        // we remove all of the props in the computed style from the cache because the computed style could change inbetween every frame.
        for computed_idx in computed_idxs {
            for prop in computed_style.style_props() {
                self.cache.remove_prop(prop, computed_idx);
            }
        }
    }

    /// For a given pair of frame ids, find where the full animation progress is within the subrange of the frame id pair.
    pub(crate) fn get_local_percent(&self, prev_frame: u16, target_frame: u16) -> f64 {
        // undo the frame change that get current key_frame props does so that low is actually lower
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

    /// returns `true` if the animation is in the idle state
    pub fn is_idle(&self) -> bool {
        self.state_kind() == AnimStateKind::Idle
    }

    /// returns `true` if the animation is in the pass in progress state
    pub fn is_in_progress(&self) -> bool {
        self.state_kind() == AnimStateKind::PassInProgress
    }

    /// returns `true` if the animation is in the completed state
    pub fn is_completed(&self) -> bool {
        self.state_kind() == AnimStateKind::Completed
    }

    /// returns `true` if the animation is in the stopped state
    pub fn is_stopped(&self) -> bool {
        self.state_kind() == AnimStateKind::Stopped
    }

    /// returns true if the animation can advance, which either means the animation will transition states, or properties can be animated and updated
    pub const fn can_advance(&self) -> bool {
        match self.state_kind() {
            AnimStateKind::PassFinished
            | AnimStateKind::PassInProgress
            | AnimStateKind::Idle
            | AnimStateKind::Completed => true,
            AnimStateKind::Paused | AnimStateKind::Stopped => false,
        }
    }

    /// returns true if the animation should auto reverse
    pub const fn is_auto_reverse(&self) -> bool {
        self.auto_reverse
    }

    /// Returns true if the internal folded style of the animation should be applied.
    ///
    /// This is used when the animation cannot advance but the folded style should still be applied.
    /// For example, when the animation is paused or when `apply_when_finished` is set.
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

    /// Apply the folded (last computed) style values to the given computed style.
    ///
    /// This is used when the animation is paused or completed but should still
    /// apply its last interpolated values.
    pub fn apply_folded(&self, computed_style: &mut Style) {
        computed_style.apply_mut(self.folded_style.clone());
    }
}
