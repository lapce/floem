//! Inspector previews for style property values — floem's, not the engine's.
//!
//! `floem_style` carries only cascade-relevant prop metadata; it does not
//! know how to render a value as a widget. This module owns that entirely:
//!
//! - [`PropDebugView`] — a floem trait a value type implements to produce a
//!   preview [`View`]. Unlike the engine's old `InspectorRender`/`PropDebugView`
//!   pair it returns a `Box<dyn View>` directly — no `Box<dyn Any>` shuffling.
//! - A [`TypeId`]-keyed [registry](register_prop_preview) mapping a *stored*
//!   prop value type to its preview builder. The inspector reads a prop's
//!   value through `floem_style`'s `StylePropInfo::value_as_any` (type-erased)
//!   and dispatches here by the value's `TypeId`. Built-in value types are
//!   seeded on first use; downstream crates register their own custom prop
//!   value types via [`register_prop_preview`].
//!
//! The actual widget bodies still live on [`FloemInspectorRender`]
//! (`inspector_render_impl.rs`); the impls below delegate to it.

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::{LazyLock, RwLock};

use parley::{FontStyle as FontStyleProp, FontWeight as FontWeightProp};
use peniko::kurbo::{self, Affine, Stroke};
use peniko::{Brush, Color, Gradient};
use smallvec::SmallVec;
use taffy::geometry::MinMax;
use taffy::prelude::Line;
use taffy::style::{MaxTrackSizingFunction, MinTrackSizingFunction};
use taffy::GridTemplateComponent;

#[allow(deprecated)]
use floem_style::unit::{Length, LengthAuto, Pct, Pt, Px, PxPct, PxPctAuto};
use floem_style::{
    Border, BorderColor, BorderRadius, BoxShadow, Margin, ObjectFit, ObjectPosition, Padding,
    StylePropValue, Transition,
};

use crate::style::design_system::DesignSystem;
use crate::style::FloemInspectorRender;
use crate::theme::StyleThemeExt;
use crate::view::{IntoView, View};
#[cfg(feature = "editor")]
use crate::views::editor::text::{RenderWhitespace, WrapMethod};
use crate::views::{Decorators, Empty, Label, Stack};
#[cfg(feature = "editor")]
use floem_editor_core::indent::IndentStyle;

/// A floem value type that can render an inspector preview of itself.
///
/// Returns a [`View`] directly (no type erasure). The blanket impls for
/// `Option`/`Vec`/`SmallVec` mean a wrapper value previews by unwrapping to
/// its element type.
pub trait PropDebugView {
    /// Build a preview widget for this value, or `None` if it has no
    /// meaningful inspector rendering.
    fn debug_view(&self) -> Option<Box<dyn View>> {
        None
    }
}

/// Downcast a `Box<dyn Any>` produced by [`FloemInspectorRender`] back to its
/// `Box<dyn View>`. The renderer always boxes a view, so this never falls
/// through in practice.
fn from_any(any: Box<dyn Any>) -> Box<dyn View> {
    any.downcast::<Box<dyn View>>()
        .ok()
        .map(|b| *b)
        .unwrap_or_else(|| Empty::new().into_any())
}

// ── small layout helpers for the composite (Vec/SmallVec) previews ──────────

fn text(s: &str) -> Box<dyn View> {
    Label::new(s.to_string()).into_any()
}

fn muted(s: &str) -> Box<dyn View> {
    Label::new(s.to_string())
        .style(|s| s.with_theme(|s, t| s.color(t.text_muted())))
        .into_any()
}

fn labelled_row(label: &str, content: Box<dyn View>) -> Box<dyn View> {
    Stack::new((
        Label::new(label.to_string()).style(|s| s.with_theme(|s, t| s.color(t.text_muted()))),
        content,
    ))
    .style(|s| s.items_center().gap(8.0).padding(4.0))
    .into_any()
}

fn vertical_list(items: Vec<Box<dyn View>>) -> Box<dyn View> {
    Stack::vertical_from_iter(items)
        .style(|s| s.gap(4.0))
        .into_any()
}

fn horizontal_pair(first: Box<dyn View>, second: Box<dyn View>) -> Box<dyn View> {
    Stack::new((first, second)).style(|s| s.gap(8.0)).into_any()
}

// ── leaf impls (delegate widget construction to FloemInspectorRender) ───────

impl PropDebugView for Color {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        Some(from_any(FloemInspectorRender.color(*self)))
    }
}
impl PropDebugView for Gradient {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        Some(from_any(FloemInspectorRender.gradient(self)))
    }
}
impl PropDebugView for Brush {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        match self {
            Brush::Solid(_) | Brush::Gradient(_) => Some(from_any(FloemInspectorRender.brush(self))),
            Brush::Image(_) => None,
        }
    }
}
impl PropDebugView for Stroke {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        Some(from_any(FloemInspectorRender.stroke(self)))
    }
}
impl PropDebugView for kurbo::Rect {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        Some(from_any(FloemInspectorRender.rect(self)))
    }
}
impl PropDebugView for Affine {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        Some(from_any(FloemInspectorRender.affine(self)))
    }
}
impl PropDebugView for ObjectFit {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        Some(from_any(FloemInspectorRender.object_fit(*self)))
    }
}
impl PropDebugView for ObjectPosition {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        Some(from_any(FloemInspectorRender.object_position(self)))
    }
}
impl PropDebugView for Transition {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        Some(from_any(FloemInspectorRender.transition(self)))
    }
}
impl PropDebugView for FontWeightProp {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        Some(from_any(
            FloemInspectorRender.font_weight(*self, &format!("{self:?}")),
        ))
    }
}
impl PropDebugView for FontStyleProp {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        Some(from_any(
            FloemInspectorRender.font_style(*self, &format!("{self:?}")),
        ))
    }
}

impl PropDebugView for Border {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        Some(from_any(FloemInspectorRender.border(self)))
    }
}
impl PropDebugView for BorderColor {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        Some(from_any(FloemInspectorRender.border_color(self)))
    }
}
impl PropDebugView for BorderRadius {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        Some(from_any(FloemInspectorRender.border_radius(self)))
    }
}
impl PropDebugView for Padding {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        Some(from_any(FloemInspectorRender.padding(self)))
    }
}
impl PropDebugView for Margin {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        Some(from_any(FloemInspectorRender.margin(self)))
    }
}
impl PropDebugView for BoxShadow {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        Some(from_any(FloemInspectorRender.box_shadow(self)))
    }
}

// ── length-like impls (plain formatted text) ────────────────────────────────

impl PropDebugView for Pt {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        Some(text(&format!("{} pt", self.0)))
    }
}
#[allow(deprecated)]
impl PropDebugView for Px {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        Pt(self.0).debug_view()
    }
}
impl PropDebugView for Pct {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        Some(text(&format!("{}%", self.0)))
    }
}
impl PropDebugView for LengthAuto {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        let label = match self {
            Self::Pt(v) => format!("{v} pt"),
            Self::Pct(v) => format!("{v}%"),
            Self::Em(v) => format!("{v} em"),
            Self::Lh(v) => format!("{v} lh"),
            Self::Auto => "auto".to_string(),
        };
        Some(text(&label))
    }
}
#[allow(deprecated)]
impl PropDebugView for PxPctAuto {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        LengthAuto::from(*self).debug_view()
    }
}
impl PropDebugView for Length {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        let label = match self {
            Self::Pt(v) => format!("{v} pt"),
            Self::Pct(v) => format!("{v}%"),
            Self::Em(v) => format!("{v} em"),
            Self::Lh(v) => format!("{v} lh"),
        };
        Some(text(&label))
    }
}
#[allow(deprecated)]
impl PropDebugView for PxPct {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        Length::from(*self).debug_view()
    }
}

// ── floem-owned value types ─────────────────────────────────────────────────

#[cfg(feature = "editor")]
impl PropDebugView for WrapMethod {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        Some(text(&self.to_string()))
    }
}
#[cfg(feature = "editor")]
impl PropDebugView for RenderWhitespace {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        Some(text(&self.to_string()))
    }
}
#[cfg(feature = "editor")]
impl PropDebugView for IndentStyle {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        Some(text(&self.to_string()))
    }
}

// ── blanket impls for the wrapper value types ───────────────────────────────

impl<T: PropDebugView> PropDebugView for Option<T> {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        self.as_ref().and_then(|v| v.debug_view())
    }
}

impl<T: StylePropValue + PropDebugView + 'static> PropDebugView for Vec<T> {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        if self.is_empty() {
            return Some(muted("[]"));
        }
        let items: Vec<Box<dyn View>> = self
            .iter()
            .enumerate()
            .map(|(i, item)| {
                let content = item
                    .debug_view()
                    .unwrap_or_else(|| text(&format!("{item:?}")));
                labelled_row(&format!("[{i}]"), content)
            })
            .collect();
        Some(vertical_list(items))
    }
}

impl<A: smallvec::Array> PropDebugView for SmallVec<A>
where
    <A as smallvec::Array>::Item: StylePropValue + PropDebugView,
{
    fn debug_view(&self) -> Option<Box<dyn View>> {
        if self.is_empty() {
            return Some(text("smallvec\n[]"));
        }
        let summary = text(&if self.spilled() {
            format!("smallvec\n[{}] (heap)", self.len())
        } else {
            format!("smallvec\n[{}] (inline)", self.len())
        });
        let items: Vec<Box<dyn View>> = self
            .iter()
            .enumerate()
            .map(|(i, item)| {
                let content = item
                    .debug_view()
                    .unwrap_or_else(|| text(&format!("{item:?}")));
                labelled_row(&format!("[{i}]"), content)
            })
            .collect();
        Some(horizontal_pair(summary, vertical_list(items)))
    }
}

// Element types that have no preview of their own but appear inside a
// registered `Vec<_>` need a (None) impl so the blanket bound is satisfied.
impl<T, M> PropDebugView for MinMax<T, M> {}
impl<T> PropDebugView for Line<T> {}
impl PropDebugView for GridTemplateComponent<String> {}

// ── registry ────────────────────────────────────────────────────────────────

/// Builds a preview for a type-erased prop value, or `None` if the value
/// isn't of the registered concrete type or has no preview.
pub type PreviewFn = fn(&dyn Any) -> Option<Box<dyn View>>;

fn preview_fn<T: PropDebugView + 'static>() -> PreviewFn {
    |any| any.downcast_ref::<T>().and_then(|v| v.debug_view())
}

static REGISTRY: LazyLock<RwLock<HashMap<TypeId, PreviewFn>>> =
    LazyLock::new(|| RwLock::new(builtin_registrations()));

/// The stored value types of floem's built-in props that have a preview.
/// Keyed on the *stored* type (e.g. `Option<Brush>`, not `Brush`) because
/// that is what a prop's value erases to.
fn builtin_registrations() -> HashMap<TypeId, PreviewFn> {
    let mut m: HashMap<TypeId, PreviewFn> = HashMap::new();
    macro_rules! reg {
        ($($t:ty),* $(,)?) => { $( m.insert(TypeId::of::<$t>(), preview_fn::<$t>()); )* };
    }
    reg!(
        // floem_style built-in value types
        Color,
        Option<Color>,
        Brush,
        Option<Brush>,
        Stroke,
        Affine,
        LengthAuto,
        Length,
        Pct,
        Pt,
        ObjectFit,
        ObjectPosition,
        Option<FontWeightProp>,
        Option<FontStyleProp>,
        SmallVec<[BoxShadow; 3]>,
        Vec<GridTemplateComponent<String>>,
        Vec<MinMax<MinTrackSizingFunction, MaxTrackSizingFunction>>,
        // floem-owned value types
        DesignSystem,
    );
    #[cfg(feature = "editor")]
    reg!(WrapMethod, RenderWhitespace, IndentStyle);
    #[cfg(feature = "localization")]
    m.insert(
        TypeId::of::<crate::views::localization::LocaleMap>(),
        preview_fn::<crate::views::localization::LocaleMap>(),
    );
    m
}

/// Register a preview builder for a custom prop value type `T`. Downstream
/// crates that define their own style props call this (once, at startup) so
/// their values render in the inspector. Built-in types are seeded
/// automatically.
pub fn register_prop_preview<T: PropDebugView + 'static>() {
    REGISTRY
        .write()
        .unwrap()
        .insert(TypeId::of::<T>(), preview_fn::<T>());
}

/// Look up the preview builder for a stored value type by its [`TypeId`].
pub(crate) fn lookup(id: TypeId) -> Option<PreviewFn> {
    REGISTRY.read().unwrap().get(&id).copied()
}

#[cfg(test)]
mod tests {
    use super::lookup;
    use crate::style::Style;
    use floem_style::{PropValueRef, StyleKeyInfo};
    use peniko::color::palette::css;

    /// Every prop whose value has a *rich* (non-text) inspector preview must
    /// have its **stored** value type registered. This deterministically
    /// guards the `Option<_>`/`SmallVec<_>` wrapper-type trap: e.g. `background`
    /// stores `Option<Brush>`, not `Brush`, so registering only `Brush` would
    /// silently lose the preview.
    #[test]
    fn rich_preview_prop_value_types_are_registered() {
        let style = Style::new()
            .background(css::RED) // Option<Brush>
            .color(css::BLUE) // Option<Color>
            .border(2.0) // Stroke
            .border_color(css::GREEN) // Option<Brush>
            .border_radius(4.0) // Length
            .padding(6.0) // Length
            .margin(8.0); // LengthAuto

        let mut checked = 0;
        for (key, value) in style.map.iter() {
            if let StyleKeyInfo::Prop(info) = key.info
                && let PropValueRef::Val(inner) = (info.value_as_any)(&**value)
            {
                assert!(
                    lookup(inner.type_id()).is_some(),
                    "prop `{}` value type is not registered for an inspector preview \
                     (stored-type trap?)",
                    (info.name)()
                );
                checked += 1;
            }
        }
        assert!(checked >= 5, "expected to exercise several props, got {checked}");
    }

    /// A value type with no rich preview (e.g. `f64` for `font_size`) misses
    /// the registry, so the inspector falls back to `debug_any` text.
    #[test]
    fn scalar_value_falls_back_to_text() {
        let style = Style::new().font_size(14.0);
        let mut saw_font_size = false;
        for (key, value) in style.map.iter() {
            if let StyleKeyInfo::Prop(info) = key.info
                && (info.name)().contains("FontSize")
                && let PropValueRef::Val(inner) = (info.value_as_any)(&**value)
            {
                saw_font_size = true;
                assert!(lookup(inner.type_id()).is_none());
            }
        }
        assert!(saw_font_size);
    }
}
