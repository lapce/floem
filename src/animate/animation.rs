use crate::{
    style::{Style, StyleKey, StyleKeyInfo, StylePropRef},
    view_state::StackOffset,
    ViewId,
};

use super::{AnimState, AnimStateCommand, AnimStateKind, Bezier, Easing};
use std::any::Any;
use std::rc::Rc;

use floem_reactive::{create_updater, RwSignal, SignalGet};
use smallvec::SmallVec;
#[cfg(not(target_arch = "wasm32"))]
use std::time::{Duration, Instant};
#[cfg(target_arch = "wasm32")]
use web_time::{Duration, Instant};

#[derive(Clone)]
pub struct KeyFrame {
    id: u32,
    style: Style,
    /// This easing will be used while animating towards this keyframe (or away from this keyframe if the animation is reversing).
    easing: Easing,
}
impl KeyFrame {
    pub fn new(id: u32) -> Self {
        Self {
            id,
            style: Style::default(),
            easing: Easing::default(),
        }
    }

    pub fn style(mut self, style: impl Fn(Style) -> Style) -> Self {
        self.style = style(Style::new());
        self
    }

    /// This easing function will be used while animating towards this keyframe
    pub fn easing(mut self, easing: impl Into<Easing>) -> Self {
        self.easing = easing.into();
        self
    }

    pub fn easing_linear(self) -> Self {
        self.easing(Easing::Linear)
    }

    /// Creates an animation that accelerates and/or decelerates using a custom cubic bezier.
    pub fn easing_bezier(self, curve: Bezier) -> Self {
        self.easing(Easing::CubicBezier(curve))
    }

    pub fn easing_ease(self) -> Self {
        self.easing(Bezier::EASE)
    }

    pub fn easing_in(self) -> Self {
        self.easing(Bezier::EASE_IN)
    }

    pub fn easing_out(self) -> Self {
        self.easing(Bezier::EASE_OUT)
    }

    pub fn easing_in_out(self) -> Self {
        self.easing(Bezier::EASE_IN_OUT)
    }
}

#[derive(Clone, PartialEq)]
pub enum Animate {
    /// This effectively assign the computed style (the style before animations are applied) to keyframe 0
    ///
    /// You can use this if you want the start of the animation the be the style without any animation applied and animate towards your keyframes
    FromDefault,
    /// This effectively assign the computed style (the style before animations are applied) to the maximum keyframe
    ///
    /// You can use this if you want the end of the animation the be the style without any animation applied.
    /// To do this, you would assign your animation style at keyframe 0 and let it animate towards having no animation applied at all
    ToDefault(Easing),
}

type EffectStateVec = SmallVec<[RwSignal<SmallVec<[(ViewId, StackOffset<Animation>); 1]>>; 1]>;
#[derive(Clone)]
pub struct Animation {
    pub(crate) state: AnimState,
    pub(crate) effect_states: EffectStateVec,
    // This easing is used for when animating towards the default style (the style before the animation is applied).
    // pub(crate) easing: Easing,
    pub(crate) auto_reverse: bool,
    pub(crate) delay: Duration,
    pub(crate) duration: Duration,
    pub(crate) repeat_mode: RepeatMode,
    pub(crate) animate: Animate,
    /// How many times the animation has been repeated so far
    pub(crate) repeat_count: usize,
    pub(crate) max_key_frame_num: u32,
    pub(crate) folded_style: Style,
    pub(crate) key_frames: im_rc::OrdMap<u32, KeyFrame>,
    // TODO: keep a lookup of styleprops to the last keyframe with that prop. this would be useful when there are lots of keyframes and sparse props
    // pub(crate)cache: std::collections::HashMap<StylePropRef, u32>,
    pub(crate) on_start_listener: Option<Rc<dyn Fn() + 'static>>,
    pub(crate) on_complete_listener: Option<Rc<dyn Fn() + 'static>>,
    pub(crate) debug_description: Option<String>,
}
impl Default for Animation {
    fn default() -> Self {
        Animation {
            state: AnimState::Idle,
            effect_states: SmallVec::new(),
            auto_reverse: false,
            delay: Duration::ZERO,
            duration: Duration::from_secs(1),
            repeat_mode: RepeatMode::Times(1),
            animate: Animate::FromDefault,
            repeat_count: 0,
            max_key_frame_num: 100,
            folded_style: Style::new(),
            key_frames: im_rc::OrdMap::new(),
            on_start_listener: None,
            on_complete_listener: None,
            debug_description: None,
        }
    }
}
impl Animation {
    pub fn new() -> Self {
        Self::default()
    }
}

pub(crate) fn assert_valid_time(time: f64) {
    assert!(time >= 0.0 || time <= 1.0, "time is {time}");
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

pub fn animation() -> Animation {
    Animation::default()
}

#[derive(Debug, Clone)]
pub enum AnimUpdateMsg {
    Pause,
    Resume,
    Start,
    Stop,
}

#[derive(Clone, Debug)]
pub enum SizeUnit {
    Px,
    Pct,
}

#[derive(Debug, Clone, Copy)]
pub enum AnimDirection {
    Forward,
    Backward,
}

impl Animation {
    pub fn duration(mut self, duration: Duration) -> Self {
        self.duration = duration;
        self
    }

    pub fn is_idle(&self) -> bool {
        self.state_kind() == AnimStateKind::Idle
    }

    pub fn is_in_progress(&self) -> bool {
        self.state_kind() == AnimStateKind::PassInProgress
    }

    pub fn is_completed(&self) -> bool {
        self.state_kind() == AnimStateKind::Completed
    }

    pub fn is_stopped(&self) -> bool {
        self.state_kind() == AnimStateKind::Stopped
    }

    pub fn can_advance(&self) -> bool {
        match self.state_kind() {
            AnimStateKind::PassFinished | AnimStateKind::PassInProgress | AnimStateKind::Idle => {
                true
            }
            AnimStateKind::Paused | AnimStateKind::Stopped | AnimStateKind::Completed => false,
        }
    }

    pub fn is_auto_reverse(&self) -> bool {
        self.auto_reverse
    }

    /// Returns the ID of the animation. Use this when you want to control(stop/pause/resume) the animation
    pub fn on_create(mut self, on_create_fn: impl Fn() + 'static) -> Self {
        self.on_start_listener = Some(Rc::new(on_create_fn));
        self
    }

    /// Returns the ID of the animation. Use this when you want to control(stop/pause/resume) the animation
    pub fn on_complete(mut self, on_create_fn: impl Fn() + 'static) -> Self {
        self.on_start_listener = Some(Rc::new(on_create_fn));
        self
    }

    /// If there is a matching keyframe id, the style in this keyframe will only override the style values in the new style.
    /// If you want the style to completely override style see [Animation::keyframe_override].
    pub fn keyframe(mut self, frame_id: u32, key_frame: impl Fn(KeyFrame) -> KeyFrame) -> Self {
        let frame = key_frame(KeyFrame::new(frame_id));
        match self.key_frames.entry(frame_id) {
            im_rc::ordmap::Entry::Occupied(mut oe) => {
                let e_frame = oe.get_mut();
                e_frame.style.apply_mut(frame.style);
                e_frame.easing = frame.easing;
            }
            im_rc::ordmap::Entry::Vacant(ve) => {
                ve.insert(frame);
            }
        }
        self
    }

    /// If there is a matching keyframe id, the style in this keyframe will completely override the style in the frame that already exists.
    /// If you want the style to only override the new values see [Animation::keyframe].
    pub fn keyframe_override(
        mut self,
        frame_id: u32,
        key_frame: impl Fn(KeyFrame) -> KeyFrame,
    ) -> Self {
        let frame = key_frame(KeyFrame::new(frame_id));
        self.key_frames.insert(frame_id, frame);
        self
    }

    // get the keyframe that is the current target
    pub fn get_current_keyframes(&self) -> (Option<KeyFrame>, Option<KeyFrame>) {
        let percent = self.total_time_percent();
        let frame_target = (self.max_key_frame_num as f64 * percent).floor() as u32;
        let (smaller, target, bigger) = self.key_frames.split_lookup(&frame_target);
        let upper = if let Some(target) = target {
            Some(target)
        } else {
            bigger.values().next().cloned()
        };
        let lower = smaller.values().last().cloned();
        (lower, upper)
    }

    pub fn auto_reverse(mut self, auto_rev: bool) -> Self {
        self.auto_reverse = auto_rev;
        self
    }

    pub fn delay(mut self, delay: Duration) -> Self {
        self.delay = delay;
        self
    }

    pub fn animate(mut self, animate: Animate) -> Self {
        self.animate = animate;
        self
    }

    /// Should the animation repeat forever?
    pub fn repeat(mut self, repeat: bool) -> Self {
        self.repeat_mode = if repeat {
            RepeatMode::LoopForever
        } else {
            RepeatMode::Times(1)
        };
        self
    }

    /// How many passes(loops) of the animation do we want?
    pub fn repeat_times(mut self, times: usize) -> Self {
        self.repeat_mode = RepeatMode::Times(times);
        self
    }

    /// This is used to determine which keyframe is at 100% completion.
    /// The default is 100.
    /// If you need more than 100 keyframes, increase this number, but be aware, the keyframe numbers will then be as a percentage of the maximum
    pub fn max_key_frame(mut self, max: u32) -> Self {
        self.max_key_frame_num = max;
        self
    }

    /// If `apply_initial` is false the initial command will not be applied to the animation.
    /// This is useful if you want the effect to be subscribed to changes but not run the first time.
    pub fn state(
        mut self,
        command: impl Fn() -> AnimStateCommand + 'static,
        apply_inital: bool,
    ) -> Self {
        let states = RwSignal::new(SmallVec::new());
        self.effect_states.push(states);
        let initial_command = create_updater(command, move |command| {
            for (view_id, stack_offset) in states.get_untracked() {
                view_id.update_animation_state(stack_offset, command)
            }
        });
        if apply_inital {
            self.transition(initial_command);
        }
        self
    }

    pub fn pause(self, trigger: impl Fn() + 'static) -> Self {
        self.state(
            move || {
                trigger();
                AnimStateCommand::Pause
            },
            false,
        )
    }
    pub fn resume(self, trigger: impl Fn() + 'static) -> Self {
        self.state(
            move || {
                trigger();
                AnimStateCommand::Resume
            },
            false,
        )
    }
    pub fn start(self, trigger: impl Fn() + 'static) -> Self {
        self.state(
            move || {
                trigger();
                AnimStateCommand::Start
            },
            false,
        )
    }
    pub fn stop(self, trigger: impl Fn() + 'static) -> Self {
        self.state(
            move || {
                trigger();
                AnimStateCommand::Stop
            },
            false,
        )
    }

    pub fn debug_name(mut self, description: impl Into<String>) -> Self {
        match &mut self.debug_description {
            Some(inner_desc) => {
                inner_desc.push_str("; ");
                inner_desc.push_str(&description.into())
            }
            val @ None => *val = Some(description.into()),
        };
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

    #[allow(unused)]
    pub(crate) fn stop_mut(&mut self) {
        self.transition(AnimStateCommand::Stop)
    }

    pub fn state_kind(&self) -> AnimStateKind {
        match self.state {
            AnimState::Idle => AnimStateKind::Idle,
            AnimState::Stopped => AnimStateKind::Stopped,
            AnimState::PassInProgress { .. } => AnimStateKind::PassInProgress,
            AnimState::PassFinished { .. } => AnimStateKind::PassFinished,
            AnimState::Completed { .. } => AnimStateKind::Completed,
            AnimState::Paused { .. } => AnimStateKind::Paused,
        }
    }

    pub fn elapsed(&self) -> Option<Duration> {
        match &self.state {
            AnimState::Idle => None,
            AnimState::Stopped => None,
            AnimState::PassInProgress {
                started_on,
                elapsed,
            } => {
                let duration = Instant::now() - *started_on;
                Some(*elapsed + duration)
            }
            AnimState::PassFinished { elapsed } => Some(*elapsed),
            AnimState::Completed { elapsed, .. } => *elapsed,
            AnimState::Paused { elapsed } => *elapsed,
        }
    }

    pub fn advance(&mut self) {
        match &mut self.state {
            AnimState::Idle => {
                self.start_mut();
                if let Some(ref on_start) = self.on_start_listener {
                    on_start()
                }
            }
            AnimState::PassInProgress {
                started_on,
                mut elapsed,
            } => {
                let now = Instant::now();
                let duration = now - *started_on;
                elapsed += duration;

                let temp_elapsed = if elapsed <= self.delay {
                    // The animation hasn't started yet
                    Duration::ZERO
                } else {
                    elapsed - self.delay
                };

                if temp_elapsed >= self.duration {
                    self.state = AnimState::PassFinished { elapsed };
                }
            }
            AnimState::PassFinished { elapsed } => match self.repeat_mode {
                RepeatMode::LoopForever => {
                    self.state = AnimState::PassInProgress {
                        started_on: Instant::now(),
                        elapsed: Duration::ZERO,
                    }
                }
                RepeatMode::Times(times) => {
                    self.repeat_count += 1;
                    if self.repeat_count >= times {
                        self.state = AnimState::Completed {
                            elapsed: Some(*elapsed),
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
            AnimState::Completed { .. } => {
                if let Some(ref on_complete) = self.on_complete_listener {
                    on_complete()
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

    pub(crate) fn total_time_percent(&self) -> f64 {
        if self.duration == Duration::ZERO {
            return 0.;
        }

        let mut elapsed = self.elapsed().unwrap_or(Duration::ZERO);

        if elapsed < self.delay {
            // The animation hasn't started yet
            return 0.0;
        }
        elapsed -= self.delay;

        if elapsed > self.duration {
            elapsed = self.duration;
        }

        let mut percent = elapsed.as_secs_f64() / self.duration.as_secs_f64();

        if self.auto_reverse {
            // If the animation should auto-reverse, adjust the percent accordingly
            if percent > 0.5 {
                percent = 1.0 - percent;
            }
            percent *= 2.0; // Normalize to [0.0, 1.0] range after reversal adjustment
        }

        percent
    }

    pub(crate) fn animate_into(&mut self, computed_style: &mut Style) {
        enum KeyFrameDefault {
            Keyframe(KeyFrame),
            Default,
        }
        impl KeyFrameDefault {
            fn id(&self) -> u32 {
                match self {
                    KeyFrameDefault::Keyframe(kf) => kf.id,
                    KeyFrameDefault::Default => 0,
                }
            }

            fn get(&self, key: &StylePropRef) -> Option<Rc<dyn Any>> {
                match self {
                    KeyFrameDefault::Keyframe(kf) => kf.style.map.get(&key.key).cloned(),
                    KeyFrameDefault::Default => Some((key.info().default_as_any)()),
                }
            }
        }
        let (lower, upper) = self.get_current_keyframes();

        if lower.is_none() && upper.is_none() && !matches!(self.animate, Animate::ToDefault(..)) {
            // no keyframes have been set
            return;
        }

        let upper = if let Some(upper) = upper {
            upper
        } else if let Animate::ToDefault(ref easing) = self.animate {
            KeyFrame {
                id: self.max_key_frame_num,
                style: computed_style.clone(),
                easing: easing.clone(),
            }
        } else {
            // animation is over. No more keyframes. Just keep applying the last folded style
            computed_style.apply_mut(self.folded_style.clone());
            return;
        };

        let lower = if let Some(lower) = lower {
            KeyFrameDefault::Keyframe(lower)
        } else if self.animate == Animate::FromDefault {
            KeyFrameDefault::Keyframe(KeyFrame {
                id: 0,
                style: computed_style.clone(),
                easing: Default::default(),
            })
        } else {
            KeyFrameDefault::Default
        };

        let eased_time = upper
            .easing
            .apply_easing_fn(self.get_local_percent(lower.id(), upper.id));

        let props = upper
            .style
            .map
            .into_iter()
            .filter_map(|(p, v)| match p.info {
                StyleKeyInfo::Prop(..) => Some((StylePropRef { key: p }, v)),
                _ => None,
            })
            .collect::<Vec<_>>();

        for (prop_ref, to_prop) in props {
            if let Some(from_prop) = lower.get(&prop_ref) {
                // we do the above first check instead of immediately checking all lower because the lower variable might be the computed_style
                if let Some(interpolated) = (prop_ref.info().interpolate)(
                    &*from_prop.clone(),
                    &*to_prop.clone(),
                    eased_time,
                ) {
                    self.folded_style.map.insert(prop_ref.key, interpolated);
                }
            } else {
                // search for prop in lower
                let (from_prop_id, from_prop) = self
                    .find_key_in_lower(&prop_ref.key)
                    .unwrap_or((0, (prop_ref.info().default_as_any)()));
                let eased_time = upper
                    .easing
                    .apply_easing_fn(self.get_local_percent(from_prop_id, upper.id));

                if let Some(interpolated) = (prop_ref.info().interpolate)(
                    &*from_prop.clone(),
                    &*to_prop.clone(),
                    eased_time,
                ) {
                    self.folded_style.map.insert(prop_ref.key, interpolated);
                }
            };
        }

        computed_style.apply_mut(self.folded_style.clone());
    }

    pub(crate) fn get_local_percent(&self, low_frame: u32, high_frame: u32) -> f64 {
        let low_frame = low_frame as f64;
        let high_frame = high_frame as f64;
        let total_num_frames = self.max_key_frame_num as f64;

        let low_frame_percent = low_frame.max(0.01) / total_num_frames;
        let high_frame_percent = high_frame / total_num_frames;
        let keyframe_range = high_frame_percent - low_frame_percent;

        (self.total_time_percent() - low_frame_percent) / keyframe_range
    }

    pub(crate) fn find_key_in_lower(&self, key: &StyleKey) -> Option<(u32, Rc<dyn Any>)> {
        let percent = self.total_time_percent();
        let frame_target = (self.max_key_frame_num as f64 * percent).floor() as u32;
        let (smaller, _bigger) = self.key_frames.split(&frame_target);
        smaller
            .into_iter()
            .rev()
            .find(|(_p, v)| v.style.map.contains_key(key))
            .map(|(p, v)| (p, v.style.map.get(key).unwrap().clone()))
    }
}
