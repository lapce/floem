use crate::style::StyleAnimCtx;
use dyn_clone::DynClone;

pub trait StyleAnimFn: DynClone {
    #[must_use]
    fn call(&self, ctx: StyleAnimCtx) -> StyleAnimCtx;
}

impl<F> StyleAnimFn for F
where
    F: Fn(StyleAnimCtx) -> StyleAnimCtx + Clone,
{
    #[must_use]
    fn call(&self, ctx: StyleAnimCtx) -> StyleAnimCtx {
        self(ctx)
    }
}
pub struct StyleAnim {
    pub driver: Box<dyn AnimDriver>,
    pub anim_fn: Box<dyn StyleAnimFn>,
}

pub fn style_anim(
    driver: impl AnimDriver + Clone + 'static,
    anim_fn: impl Fn(StyleAnimCtx) -> StyleAnimCtx + Clone + 'static,
) -> StyleAnim {
    StyleAnim {
        driver: Box::new(driver),
        anim_fn: Box::new(anim_fn.clone()),
    }
}

impl Clone for StyleAnim {
    fn clone(&self) -> Self {
        Self {
            driver: dyn_clone::clone_box(&*self.driver),
            anim_fn: dyn_clone::clone_box(&*self.anim_fn),
        }
    }
}

pub trait AnimDriver: DynClone {
    fn next_value(&mut self) -> f64;

    fn requests_next_frame(&self) -> bool;

    fn set_enabled(&mut self, enabled: bool, animate: bool);

    fn is_enabled(&self) -> bool;

    fn should_run(&self) -> bool {
        self.is_enabled() || self.requests_next_frame()
    }
}

pub fn passes(passes: u16, v: f64) -> f64 {
    if v != 1.0 {
        (v * passes as f64).rem_euclid(1.0)
    } else {
        1.0
    }
}

pub fn alternating(v: f64) -> f64 {
    let mut v = v * 2.0;
    if v > 1.0 {
        v -= (v - 1.0) * 2.0;
    }
    v
}
