use crate::style::{Background, BorderColor, BorderRadius, TextColor};

use super::{
    anim_val::AnimValue, AnimId, AnimPropKind, AnimState, AnimStateKind, AnimatedProp, Easing,
    EasingFn, EasingMode,
};
use std::{borrow::BorrowMut, collections::HashMap, rc::Rc};

#[cfg(not(target_arch = "wasm32"))]
use std::time::{Duration, Instant};
#[cfg(target_arch = "wasm32")]
use web_time::{Duration, Instant};

use floem_reactive::create_effect;
use peniko::{Brush, Color};

#[derive(Clone)]
pub struct Animation {
    pub(crate) id: AnimId,
    pub(crate) state: AnimState,
    pub(crate) easing: Easing,
    pub(crate) auto_reverse: bool,
    pub(crate) skip: Option<Duration>,
    pub(crate) duration: Duration,
    pub(crate) repeat_mode: RepeatMode,
    /// How many times the animation has been repeated so far
    pub(crate) repeat_count: usize,
    pub(crate) animated_props: HashMap<AnimPropKind, AnimatedProp>,
    pub(crate) on_create_listener: Option<Rc<dyn Fn(AnimId) + 'static>>,
}

pub(crate) fn assert_valid_time(time: f64) {
    assert!(time >= 0.0 || time <= 1.0);
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
    Animation {
        id: AnimId::next(),
        state: AnimState::Idle,
        easing: Easing::default(),
        auto_reverse: false,
        skip: None,
        duration: Duration::from_secs(1),
        repeat_mode: RepeatMode::Times(1),
        repeat_count: 0,
        animated_props: HashMap::new(),
        on_create_listener: None,
    }
}

#[derive(Debug, Clone)]
pub enum AnimUpdateMsg {
    Prop {
        id: AnimId,
        kind: AnimPropKind,
        val: AnimValue,
    },
    Pause(AnimId),
    Resume(AnimId),
    Start(AnimId),
    Stop(AnimId),
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
    pub fn id(&self) -> AnimId {
        self.id
    }

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
    pub fn on_create(mut self, on_create_fn: impl Fn(AnimId) + 'static) -> Self {
        self.on_create_listener = Some(Rc::new(on_create_fn));
        self
    }

    // pub fn scale(self, scale: f64) -> Self {
    //     todo!()
    // }

    pub fn border_radius(self, border_radius_fn: impl Fn() -> f64 + 'static) -> Self {
        create_effect(move |_| {
            let border_radius = border_radius_fn();

            self.id
                .update_style_prop(BorderRadius, border_radius.into());
        });

        self
    }

    pub fn color(self, color_fn: impl Fn() -> Color + 'static) -> Self {
        create_effect(move |_| {
            let color = color_fn();

            self.id.update_style_prop(TextColor, Some(color));
        });

        self
    }

    pub fn border_color<B: Into<Brush>>(self, bord_color_fn: impl Fn() -> B + 'static) -> Self {
        create_effect(move |_| {
            let border_color = bord_color_fn().into();

            self.id.update_style_prop(BorderColor, border_color);
        });

        self
    }

    pub fn background<B: Into<Brush>>(self, bg_fn: impl Fn() -> B + 'static) -> Self {
        create_effect(move |_| {
            let background = bg_fn().into();

            self.id.update_style_prop(Background, Some(background));
        });

        self
    }

    pub fn width(self, width_fn: impl Fn() -> f64 + 'static) -> Self {
        create_effect(move |_| {
            let to_width = width_fn();

            self.id
                .update_prop(AnimPropKind::Width, AnimValue::Float(to_width));
        });

        self
    }

    pub fn height(self, height_fn: impl Fn() -> f64 + 'static) -> Self {
        create_effect(move |_| {
            let height = height_fn();

            self.id
                .update_prop(AnimPropKind::Height, AnimValue::Float(height));
        });

        self
    }

    pub fn auto_reverse(mut self, auto_rev: bool) -> Self {
        self.auto_reverse = auto_rev;
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

    pub fn easing_fn(mut self, easing_fn: EasingFn) -> Self {
        self.easing.func = easing_fn;
        self
    }

    pub fn ease_mode(mut self, mode: EasingMode) -> Self {
        self.easing.mode = mode;
        self
    }

    pub fn ease_in(self) -> Self {
        self.ease_mode(EasingMode::In)
    }

    pub fn ease_out(self) -> Self {
        self.ease_mode(EasingMode::Out)
    }

    pub fn ease_in_out(self) -> Self {
        self.ease_mode(EasingMode::InOut)
    }

    pub fn pause(&mut self) {
        debug_assert!(
            self.state_kind() != AnimStateKind::Paused,
            "Tried to pause an already paused animation"
        );
        self.state = AnimState::Paused {
            elapsed: self.elapsed(),
        };
    }

    pub(crate) fn resume(&mut self) {
        debug_assert!(
            self.state_kind() == AnimStateKind::Paused,
            "Tried to resume an animation that is not paused"
        );
        if let AnimState::Paused { elapsed } = &self.state {
            self.state = AnimState::PassInProgress {
                started_on: Instant::now(),
                elapsed: elapsed.unwrap_or(Duration::ZERO),
            }
        }
    }

    pub fn start(&mut self) {
        self.repeat_count = 0;
        self.state = AnimState::PassInProgress {
            started_on: Instant::now(),
            elapsed: Duration::ZERO,
        }
    }

    pub fn stop(&mut self) {
        self.repeat_count = 0;
        self.state = AnimState::Stopped;
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
                self.start();
            }
            AnimState::PassInProgress {
                started_on,
                mut elapsed,
            } => {
                let now = Instant::now();
                let duration = now - *started_on;
                elapsed += duration;

                if elapsed >= self.duration {
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
            AnimState::Completed { .. } => {}
        }
    }

    pub(crate) fn props(&self) -> &HashMap<AnimPropKind, AnimatedProp> {
        &self.animated_props
    }

    pub(crate) fn props_mut(&mut self) -> &mut HashMap<AnimPropKind, AnimatedProp> {
        self.animated_props.borrow_mut()
    }

    pub(crate) fn animate_prop(&self, elapsed: Duration, prop_kind: &AnimPropKind) -> AnimValue {
        let mut elapsed = elapsed;
        let prop = self.animated_props.get(prop_kind).unwrap();

        if let Some(skip) = self.skip {
            elapsed += skip;
        }

        if self.duration == Duration::ZERO {
            return prop.from();
        }

        if elapsed > self.duration {
            elapsed = self.duration;
        }

        let time = elapsed.as_secs_f64() / self.duration.as_secs_f64();
        let time = self.easing.ease(time);
        assert_valid_time(time);

        if self.auto_reverse {
            if time > 0.5 {
                prop.animate(time * 2.0 - 1.0, AnimDirection::Backward)
            } else {
                prop.animate(time * 2.0, AnimDirection::Forward)
            }
        } else {
            prop.animate(time, AnimDirection::Forward)
        }
    }
}
