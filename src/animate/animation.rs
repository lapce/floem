use super::{
    anim_val::AnimValue, AnimId, AnimPropKind, AnimState, AnimStateKind, AnimatedProp, Easing,
    EasingFn, EasingMode,
};
use std::{borrow::BorrowMut, collections::HashMap, time::Duration, time::Instant};

use leptos_reactive::create_effect;
use vello::peniko::Color;

use crate::ViewContext;

#[derive(Clone, Debug)]
pub struct Animation {
    pub(crate) id: AnimId,
    pub(crate) state: AnimState,
    pub(crate) easing: Easing,
    pub(crate) auto_reverse: bool,
    pub(crate) skip: Option<Duration>,
    pub(crate) duration: Duration,
    pub(crate) repeat_mode: RepeatMode,
    pub(crate) repeat_count: usize,
    pub(crate) animated_props: HashMap<AnimPropKind, AnimatedProp>,
}

pub(crate) fn assert_valid_time(time: f64) {
    assert!(time >= 0.0 || time <= 1.0);
}

/// See [`Self::advance`].
#[derive(Clone, Debug)]
pub enum RepeatMode {
    /// Once started, the animation will juggle between [`AnimState::PassInProgress`] and [`AnimState::PassFinished`],
    /// but will never reach [`AnimState::Completed`]
    LoopForever,
    /// How many passes do we want, i.e. how many times do we repeat the animation?
    /// On every pass, we animate until `elapsed >= duration`, then we reset elapsed time to 0 and increment `repeat_count` is
    /// increased by 1. This process is repeated until `repeat_count >= times`, and then the animation is set
    /// to [`AnimState::Completed`].
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
    }
}

#[derive(Debug, Clone)]
pub enum AnimUpdateMsg {
    Prop {
        id: AnimId,
        kind: AnimPropKind,
        val: AnimValue,
    },
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
        matches!(self.state_kind(), AnimStateKind::Idle)
    }

    pub fn is_in_progress(&self) -> bool {
        matches!(self.state_kind(), AnimStateKind::PassInProgress)
    }

    pub fn is_completed(&self) -> bool {
        matches!(self.state_kind(), AnimStateKind::Completed)
    }

    pub fn is_auto_reverse(&self) -> bool {
        self.auto_reverse
    }

    // pub fn scale(self, scale: f64) -> Self {
    //     todo!()
    // }

    pub fn border_radius(self, border_radius_fn: impl Fn() -> f64 + 'static) -> Self {
        let cx = ViewContext::get_current();
        create_effect(cx.scope, move |_| {
            let border_radius = border_radius_fn();

            self.id
                .update_prop(AnimPropKind::BorderRadius, AnimValue::Float(border_radius));
        });

        self
    }

    pub fn color(self, color_fn: impl Fn() -> Color + 'static) -> Self {
        let cx = ViewContext::get_current();
        create_effect(cx.scope, move |_| {
            let color = color_fn();

            self.id
                .update_prop(AnimPropKind::Color, AnimValue::Color(color));
        });

        self
    }

    pub fn border_color(self, bord_color_fn: impl Fn() -> Color + 'static) -> Self {
        let cx = ViewContext::get_current();
        create_effect(cx.scope, move |_| {
            let border_color = bord_color_fn();

            self.id
                .update_prop(AnimPropKind::BorderColor, AnimValue::Color(border_color));
        });

        self
    }

    pub fn background(self, bg_fn: impl Fn() -> Color + 'static) -> Self {
        let cx = ViewContext::get_current();
        create_effect(cx.scope, move |_| {
            let background = bg_fn();

            self.id
                .update_prop(AnimPropKind::Background, AnimValue::Color(background));
        });

        self
    }

    pub fn width(self, width_fn: impl Fn() -> f64 + 'static) -> Self {
        let cx = ViewContext::get_current();
        create_effect(cx.scope, move |_| {
            let to_width = width_fn();

            self.id
                .update_prop(AnimPropKind::Width, AnimValue::Float(to_width));
        });

        self
    }

    pub fn height(self, height_fn: impl Fn() -> f64 + 'static) -> Self {
        let cx = ViewContext::get_current();
        create_effect(cx.scope, move |_| {
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

    pub fn begin(&mut self) {
        self.repeat_count = 0;
        self.state = AnimState::PassInProgress {
            started_on: Instant::now(),
            elapsed: Duration::ZERO,
        }
    }

    pub fn stop(&mut self) {
        match &mut self.state {
            AnimState::Idle | AnimState::Completed { .. } | AnimState::PassFinished { .. } => {}
            AnimState::PassInProgress {
                started_on,
                elapsed,
            } => {
                let duration = Instant::now() - *started_on;
                let elapsed = *elapsed + duration;
                self.state = AnimState::Completed {
                    elapsed: Some(elapsed),
                }
            }
        }
    }

    pub fn state_kind(&self) -> AnimStateKind {
        match self.state {
            AnimState::Idle => AnimStateKind::Idle,
            AnimState::PassInProgress { .. } => AnimStateKind::PassInProgress,
            AnimState::PassFinished { .. } => AnimStateKind::PassFinished,
            AnimState::Completed { .. } => AnimStateKind::Completed,
        }
    }

    pub fn elapsed(&self) -> Option<Duration> {
        match &self.state {
            AnimState::Idle => None,
            AnimState::PassInProgress {
                started_on,
                elapsed,
            } => {
                let duration = Instant::now() - started_on.clone();
                Some(*elapsed + duration)
            }
            AnimState::PassFinished { elapsed } => Some(elapsed.clone()),
            AnimState::Completed { elapsed, .. } => elapsed.clone(),
        }
    }

    pub fn advance(&mut self) {
        match &mut self.state {
            AnimState::Idle => {
                self.begin();
            }
            AnimState::PassInProgress {
                started_on,
                mut elapsed,
            } => {
                let now = Instant::now();
                let duration = now - started_on.clone();
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
        let prop = self.animated_props.get(&prop_kind).unwrap();

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
