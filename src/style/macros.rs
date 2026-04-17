//! Style-system macros that still live on the `floem` side.
//!
//! The `prop!` and `style_debug_group!` macros moved to the `floem_style` crate
//! (their expansions only reference types from that crate). `prop_extractor!`
//! stays here because its expansion references `$crate::context::StyleCx`,
//! which is a floem type.

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
