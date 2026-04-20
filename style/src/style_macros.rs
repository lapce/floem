//! Style system macros usable from `floem_style`.
//!
//! [`prop!`] expands to define a zero-sized prop type and a [`StyleProp`]
//! impl that installs a static [`StyleKeyInfo::Prop`].
//! [`style_debug_group!`] expands to a similar zero-sized group marker.
//! [`prop_extractor!`] expands to a struct that collects a set of
//! [`StyleProp`]s and reads them off a [`Style`] with transition
//! bookkeeping; the generated convenience methods drive transitions
//! through the [`PropExtractorCx`](crate::PropExtractorCx) trait so the
//! macro is host-neutral.
//!
//! All three macros live in this crate (rather than `floem`) because their
//! expansions reference types in `floem_style` — `Style`, `StyleMapValue`,
//! `StyleValue`, `PropDebugView`, `StyleProp`, `ExtractorField`, and
//! `PropExtractorCx`.

#[macro_export]
macro_rules! style_debug_group {
    ($(#[$meta:meta])* $v:vis $name:ident $(, inherited = $inherited:ident)?, members = [$($prop:ty),* $(,)?], view = $view:path) => {
        $(#[$meta])*
        #[derive(Default, Copy, Clone)]
        $v struct $name;

        impl $crate::StyleDebugGroup for $name {
            fn key() -> $crate::StyleKey {
                static INFO: $crate::StyleKeyInfo =
                    $crate::StyleKeyInfo::DebugGroup(
                        $crate::StyleDebugGroupInfo::new::<$name>(
                            $crate::style_debug_group!(@inherited $($inherited)?),
                            || vec![$(<$prop as $crate::StyleProp>::key()),*],
                            |style| {
                                let style = style
                                    .downcast_ref::<$crate::Style>()
                                    .expect("debug_view called with non-Style argument");
                                $view(style).map(|view| {
                                    Box::new(view) as Box<dyn std::any::Any>
                                })
                            },
                        )
                    );
                $crate::StyleKey { info: &INFO }
            }

            fn member_props() -> Vec<$crate::StyleKey> {
                vec![$(<$prop as $crate::StyleProp>::key()),*]
            }

            fn debug_view(style: &dyn std::any::Any) -> Option<Box<dyn std::any::Any>> {
                let style = style
                    .downcast_ref::<$crate::Style>()
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
        impl $crate::StyleProp for $name {
            type Type = $ty;
            fn key() -> $crate::StyleKey {
                static TRANSITION_INFO: $crate::StyleKeyInfo = $crate::StyleKeyInfo::Transition;
                static INFO: $crate::StyleKeyInfo = $crate::StyleKeyInfo::Prop($crate::StylePropInfo {
                    name: || std::any::type_name::<$name>(),
                    inherited: $crate::prop!([impl inherited][$($options)*]),
                    default_as_any: || std::rc::Rc::new($crate::StyleMapValue::Val($name::default_value())),
                    interpolate: |val1, val2, time| {
                        use std::any::{Any, type_name};
                        if let (Some(v1), Some(v2)) = (
                            val1.downcast_ref::<$crate::StyleMapValue<$ty>>(),
                            val2.downcast_ref::<$crate::StyleMapValue<$ty>>(),
                        ) {
                            if let (
                                $crate::StyleMapValue::Val(v1) | $crate::StyleMapValue::Animated(v1),
                                $crate::StyleMapValue::Val(v2) | $crate::StyleMapValue::Animated(v2),
                            ) = (v1, v2)
                            {
                                <$ty as $crate::StylePropValue>::interpolate(v1, v2, time)
                                    .map(|val| std::rc::Rc::new($crate::StyleMapValue::Animated(val)) as std::rc::Rc<dyn Any>)
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
                        if let Some(v) = val.downcast_ref::<$crate::StyleMapValue<$ty>>() {
                            match v {
                                $crate::StyleMapValue::Val(v) | $crate::StyleMapValue::Animated(v) => format!("{v:?}"),
                                $crate::StyleMapValue::Context(_) => "Context(..)".to_owned(),
                                $crate::StyleMapValue::Unset => "Unset".to_owned(),
                            }
                        } else {
                            panic!(
                                "expected type {} for property {}",
                                type_name::<$ty>(),
                                type_name::<$name>(),
                            )
                        }
                    },
                    debug_view: |val, r| {
                        if let Some(v) = val.downcast_ref::<$crate::StyleMapValue<$ty>>() {
                            match v {
                                $crate::StyleMapValue::Val(v) | $crate::StyleMapValue::Animated(v) => {
                                    <$ty as $crate::PropDebugView>::debug_view(v, r)
                                }
                                $crate::StyleMapValue::Context(_) => Some(r.text("Context(..)")),
                                $crate::StyleMapValue::Unset => Some(r.text("Unset")),
                            }
                        } else {
                            panic!(
                                "expected type {} for property {}",
                                std::any::type_name::<$ty>(),
                                std::any::type_name::<$name>(),
                            )
                        }
                    },
                    transition_key: $crate::StyleKey { info: &TRANSITION_INFO },
                    hash_any: |val| {
                        use std::any::type_name;
                        if let Some(v) = val.downcast_ref::<$crate::StyleMapValue<$ty>>() {
                            match v {
                                $crate::StyleMapValue::Val(v) | $crate::StyleMapValue::Animated(v) => <$ty as $crate::StylePropValue>::content_hash(v),
                                $crate::StyleMapValue::Context(_) => 1,
                                $crate::StyleMapValue::Unset => 0,
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
                            val1.downcast_ref::<$crate::StyleMapValue<$ty>>(),
                            val2.downcast_ref::<$crate::StyleMapValue<$ty>>(),
                        ) {
                            match (v1, v2) {
                                (
                                    $crate::StyleMapValue::Val(a) | $crate::StyleMapValue::Animated(a),
                                    $crate::StyleMapValue::Val(b) | $crate::StyleMapValue::Animated(b),
                                ) => a == b,
                                ($crate::StyleMapValue::Unset, $crate::StyleMapValue::Unset) => true,
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
                            .downcast_ref::<$crate::Style>()
                            .expect("resolve_inherited_any called with non-Style argument");
                        let resolved = match val.downcast_ref::<$crate::StyleMapValue<$ty>>().unwrap_or_else(|| {
                            panic!(
                                "expected type {} for property {}",
                                type_name::<$ty>(),
                                type_name::<$name>(),
                            )
                        }) {
                            $crate::StyleMapValue::Val(value) | $crate::StyleMapValue::Animated(value) => {
                                $crate::StyleMapValue::Val(value.clone())
                            }
                            $crate::StyleMapValue::Context(context_value) => {
                                $crate::StyleMapValue::Val(
                                    style.resolve_context(context_value),
                                )
                            }
                            $crate::StyleMapValue::Unset => $crate::StyleMapValue::Unset,
                        };
                        std::rc::Rc::new(resolved)
                    },
                });
                $crate::StyleKey { info: &INFO }
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
            $($prop_vis:vis $field:ident: $prop:ty),*
            $(,)?
        }
    ) => {
        #[derive(Debug, Clone)]
        $(#[$attrs])?
        $vis struct $name {
            $(
                $prop_vis $field: $crate::ExtractorField<$prop>,
            )*
        }

        impl $name {
            /// Read every field of this extractor from `style`, advancing
            /// any in-flight transitions against `cx.now()`. Returns
            /// whether the aggregate resolved value changed. Sets
            /// `*transitioning` to `true` if any field is still animating
            /// after this pass — the caller is expected to schedule a
            /// re-cascade on the owning node when that happens.
            #[allow(dead_code)]
            $vis fn read_style(
                &mut self,
                cx: &dyn $crate::PropExtractorCx,
                style: &$crate::Style,
                transitioning: &mut bool,
            ) -> bool {
                let now = $crate::PropExtractorCx::now(cx);
                let mut changed = false;
                $(
                    changed |= self.$field.read(style, &now, transitioning);
                )*
                changed
            }

            /// Read every field of this extractor from `cx.direct_style()`.
            /// See [`read_style`] for the transition contract.
            #[allow(dead_code)]
            $vis fn read(
                &mut self,
                cx: &dyn $crate::PropExtractorCx,
                transitioning: &mut bool,
            ) -> bool {
                self.read_explicit(
                    $crate::PropExtractorCx::direct_style(cx),
                    &$crate::PropExtractorCx::now(cx),
                    transitioning,
                )
            }

            #[allow(dead_code)]
            $vis fn read_explicit(
                &mut self,
                style: &$crate::Style,
                #[cfg(not(target_arch = "wasm32"))]
                now: &std::time::Instant,
                #[cfg(target_arch = "wasm32")]
                now: &web_time::Instant,
                request_transition: &mut bool,
            ) -> bool {
                false $(| self.$field.read(style, now, request_transition))*
            }

            $($prop_vis fn $field(&self) -> <$prop as $crate::StyleProp>::Type
            {
                self.$field.get()
            })*
        }

        impl Default for $name {
            fn default() -> Self {
                Self {
                    $(
                        $field: $crate::ExtractorField::new(),
                    )*
                }
            }
        }
    };
}

