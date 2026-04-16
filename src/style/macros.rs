//! Style system macros.
//!
//! These macros construct the type-erased static metadata used by the style
//! engine (`StyleKey`, `StylePropInfo`, `StyleClassInfo`, `StyleDebugGroupInfo`)
//! for user-defined properties, classes, and debug groups. They live in the
//! `floem` crate (rather than `floem-style`) because their expansions embed
//! `$crate::views::Label` fallbacks in debug-view closures — that path only
//! resolves from `floem`.

#[macro_export]
macro_rules! style_class {
    ($(#[$meta:meta])* $v:vis $name:ident) => {
        $(#[$meta])*
        #[derive(Default, Copy, Clone)]
        $v struct $name;

        impl $crate::style::StyleClass for $name {
            fn key() -> $crate::style::StyleKey {
                static INFO: $crate::style::StyleKeyInfo = $crate::style::StyleKeyInfo::Class(
                    $crate::style::StyleClassInfo::new::<$name>()
                );
                $crate::style::StyleKey { info: &INFO }
            }
        }
    };
}

#[macro_export]
macro_rules! style_debug_group {
    ($(#[$meta:meta])* $v:vis $name:ident $(, inherited = $inherited:ident)?, members = [$($prop:ty),* $(,)?], view = $view:path) => {
        $(#[$meta])*
        #[derive(Default, Copy, Clone)]
        $v struct $name;

        impl $crate::style::StyleDebugGroup for $name {
            fn key() -> $crate::style::StyleKey {
                static INFO: $crate::style::StyleKeyInfo =
                    $crate::style::StyleKeyInfo::DebugGroup(
                        $crate::style::StyleDebugGroupInfo::new::<$name>(
                            style_debug_group!(@inherited $($inherited)?),
                            || vec![$(<$prop as $crate::style::StyleProp>::key()),*],
                            |style| {
                                let style = style
                                    .downcast_ref::<$crate::style::Style>()
                                    .expect("debug_view called with non-Style argument");
                                $view(style).map(|view| {
                                    Box::new(view) as Box<dyn std::any::Any>
                                })
                            },
                        )
                    );
                $crate::style::StyleKey { info: &INFO }
            }

            fn member_props() -> Vec<$crate::style::StyleKey> {
                vec![$(<$prop as $crate::style::StyleProp>::key()),*]
            }

            fn debug_view(style: &dyn std::any::Any) -> Option<Box<dyn std::any::Any>> {
                let style = style
                    .downcast_ref::<$crate::style::Style>()
                    .expect("debug_view called with non-Style argument");
                $view(style).map(|view| Box::new(view) as Box<dyn std::any::Any>)
            }
        }
    };
    (@inherited inherited) => {
        true
    };
    (@inherited) => {
        false
    };
}

#[macro_export]
macro_rules! prop {
    ($(#[$meta:meta])* $v:vis $name:ident: $ty:ty { $($options:tt)* } = $default:expr
    ) => {
        $(#[$meta])*
        #[derive(Default, Copy, Clone)]
        #[allow(missing_docs)]
        $v struct $name;
        impl $crate::style::StyleProp for $name {
            type Type = $ty;
            fn key() -> $crate::style::StyleKey {
                static TRANSITION_INFO: $crate::style::StyleKeyInfo = $crate::style::StyleKeyInfo::Transition;
                static INFO: $crate::style::StyleKeyInfo = $crate::style::StyleKeyInfo::Prop($crate::style::StylePropInfo {
                    name: || std::any::type_name::<$name>(),
                    inherited: prop!([impl inherited][$($options)*]),
                    default_as_any: || std::rc::Rc::new($crate::style::StyleMapValue::Val($name::default_value())),
                    interpolate: |val1, val2, time| {
                        use std::any::{Any, type_name};
                        if let (Some(v1), Some(v2)) = (
                            val1.downcast_ref::<$crate::style::StyleMapValue<$ty>>(),
                            val2.downcast_ref::<$crate::style::StyleMapValue<$ty>>(),
                        ) {
                            if let (
                                $crate::style::StyleMapValue::Val(v1) | $crate::style::StyleMapValue::Animated(v1),
                                $crate::style::StyleMapValue::Val(v2) | $crate::style::StyleMapValue::Animated(v2),
                            ) = (v1, v2)
                            {
                                <$ty as $crate::style::StylePropValue>::interpolate(v1, v2, time)
                                    .map(|val| std::rc::Rc::new($crate::style::StyleMapValue::Animated(val)) as std::rc::Rc<dyn Any>)
                            } else {
                                None
                            }
                        } else {
                            panic!(
                                "expected type {} for property {}. Got typeids {:?} and {:?}",
                                type_name::<$ty>(),
                                type_name::<$name>(),
                                val1.type_id(),
                                val2.type_id()
                            )
                        }
                    },
                    debug_any: |val| {
                        use std::any::type_name;
                        if let Some(v) = val.downcast_ref::<$crate::style::StyleMapValue<$ty>>() {
                            match v {
                                $crate::style::StyleMapValue::Val(v) | $crate::style::StyleMapValue::Animated(v) => format!("{v:?}"),
                                $crate::style::StyleMapValue::Context(_) => "Context(..)".to_owned(),
                                $crate::style::StyleMapValue::Unset => "Unset".to_owned(),
                            }
                        } else {
                            panic!(
                                "expected type {} for property {}",
                                type_name::<$ty>(),
                                type_name::<$name>(),
                            )
                        }
                    },
                    debug_view: |val| {
                        if let Some(v) = val.downcast_ref::<$crate::style::StyleMapValue<$ty>>() {
                            match v {
                                $crate::style::StyleMapValue::Val(v) | $crate::style::StyleMapValue::Animated(v) => {
                                    <$ty as $crate::style::PropDebugView>::debug_view(v)
                                        .map(|view| Box::new(view) as Box<dyn std::any::Any>)
                                }
                                $crate::style::StyleMapValue::Context(_) => Some(Box::new(
                                    <$crate::views::Label as $crate::view::IntoView>::into_any(
                                        $crate::views::Label::new("Context(..)")
                                    )
                                ) as Box<dyn std::any::Any>),
                                $crate::style::StyleMapValue::Unset => Some(Box::new(
                                    <$crate::views::Label as $crate::view::IntoView>::into_any(
                                        $crate::views::Label::new("Unset")
                                    )
                                ) as Box<dyn std::any::Any>),
                            }
                        } else {
                            panic!(
                                "expected type {} for property {}",
                                std::any::type_name::<$ty>(),
                                std::any::type_name::<$name>(),
                            )
                        }
                    },
                    transition_key: $crate::style::StyleKey { info: &TRANSITION_INFO },
                    hash_any: |val| {
                        use std::any::type_name;
                        if let Some(v) = val.downcast_ref::<$crate::style::StyleMapValue<$ty>>() {
                            match v {
                                $crate::style::StyleMapValue::Val(v) | $crate::style::StyleMapValue::Animated(v) => <$ty as $crate::style::StylePropValue>::content_hash(v),
                                $crate::style::StyleMapValue::Context(_) => 1,
                                $crate::style::StyleMapValue::Unset => 0,
                            }
                        } else {
                            panic!(
                                "expected type {} for property {}",
                                type_name::<$ty>(),
                                type_name::<$name>(),
                            )
                        }
                    },
                    eq_any: |val1, val2| {
                        use std::any::type_name;
                        if let (Some(v1), Some(v2)) = (
                            val1.downcast_ref::<$crate::style::StyleMapValue<$ty>>(),
                            val2.downcast_ref::<$crate::style::StyleMapValue<$ty>>(),
                        ) {
                            match (v1, v2) {
                                (
                                    $crate::style::StyleMapValue::Val(a) | $crate::style::StyleMapValue::Animated(a),
                                    $crate::style::StyleMapValue::Val(b) | $crate::style::StyleMapValue::Animated(b),
                                ) => a == b,
                                ($crate::style::StyleMapValue::Unset, $crate::style::StyleMapValue::Unset) => true,
                                _ => false,
                            }
                        } else {
                            panic!(
                                "expected type {} for property {}. Got typeids {:?} and {:?}",
                                type_name::<$ty>(),
                                type_name::<$name>(),
                                val1.type_id(),
                                val2.type_id()
                            )
                        }
                    },
                    resolve_inherited_any: |val, style| {
                        use std::any::type_name;
                        let style = style
                            .downcast_ref::<$crate::style::Style>()
                            .expect("resolve_inherited_any called with non-Style argument");
                        let resolved = match val.downcast_ref::<$crate::style::StyleMapValue<$ty>>().unwrap_or_else(|| {
                            panic!(
                                "expected type {} for property {}",
                                type_name::<$ty>(),
                                type_name::<$name>(),
                            )
                        }) {
                            $crate::style::StyleMapValue::Val(value) | $crate::style::StyleMapValue::Animated(value) => {
                                $crate::style::StyleMapValue::Val(value.clone())
                            }
                            $crate::style::StyleMapValue::Context(context_value) => {
                                $crate::style::StyleMapValue::Val(context_value.resolve(style))
                            }
                            $crate::style::StyleMapValue::Unset => $crate::style::StyleMapValue::Unset,
                        };
                        std::rc::Rc::new(resolved)
                    },
                });
                $crate::style::StyleKey { info: &INFO }
            }
            fn default_value() -> Self::Type {
                $default
            }
        }
    };
    ([impl inherited][inherited]) => {
        true
    };
    ([impl inherited][]) => {
        false
    };
}

#[macro_export]
macro_rules! prop_extractor {
    (
        $(#[$attrs:meta])* $vis:vis $name:ident {
            $($prop_vis:vis $prop:ident: $reader:ty),*
            $(,)?
        }
    ) => {
        #[derive(Debug, Clone)]
        $(#[$attrs])?
        $vis struct $name {
            $(
                $prop_vis $prop: $crate::style::ExtractorField<$reader>,
            )*
        }

        impl $name {
            #[allow(dead_code)]
            $vis fn read_style(&mut self, cx: &mut $crate::context::StyleCx, style: &$crate::style::Style) -> bool {
                self.read_style_for(cx, style, cx.current_view().get_element_id())
            }

            #[allow(dead_code)]
            $vis fn read_style_for(
                &mut self,
                cx: &mut $crate::context::StyleCx,
                style: &$crate::style::Style,
                target: impl Into<$crate::ElementId>,
            ) -> bool {
                let mut transition = false;
                let changed = false $(
                    | self.$prop.read(style, &cx.now(), &mut transition)
                )*;
                if transition {
                    cx.request_transition_for(target);
                }
                changed
            }

           #[allow(dead_code)]
            $vis fn read(&mut self, cx: &mut $crate::context::StyleCx) -> bool {
                self.read_for(cx, cx.current_view().get_element_id())
            }

           #[allow(dead_code)]
            $vis fn read_for(
                &mut self,
                cx: &mut $crate::context::StyleCx,
                target: impl Into<$crate::ElementId>,
            ) -> bool {
                let mut transition = false;
                let changed = self.read_explicit(
                    &cx.direct_style(),
                    &cx.now(),
                    &mut transition,
                );
                if transition {
                    cx.request_transition_for(target);
                }
                changed
            }

            #[allow(dead_code)]
            $vis fn read_explicit(
                &mut self,
                style: &$crate::style::Style,
                #[cfg(not(target_arch = "wasm32"))]
                now: &std::time::Instant,
                #[cfg(target_arch = "wasm32")]
                now: &web_time::Instant,
                request_transition: &mut bool
            ) -> bool {
                false $(| self.$prop.read(style, now, request_transition))*
            }

            $($prop_vis fn $prop(&self) -> <$reader as $crate::style::StylePropReader>::Type
            {
                self.$prop.get()
            })*
        }

        impl Default for $name {
            fn default() -> Self {
                Self {
                    $(
                        $prop: $crate::style::ExtractorField::new(),
                    )*
                }
            }
        }
    };
}
