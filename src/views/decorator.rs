use std::{cell::RefCell, collections::HashMap};

use leptos_reactive::create_effect;
use taffy::style::{Dimension, Style};

use crate::{app::AppContext, id::Id, view::View};

pub trait Decorators: View + Sized {
    fn style(self, cx: AppContext, style: impl Fn(AppContext) -> Style + 'static) -> Self {
        let id = self.id();
        create_effect(cx.scope, move |_| {
            let style = style(cx);
            AppContext::add_style(id, style);
        });
        self
    }
}

impl<V: View> Decorators for V {}
