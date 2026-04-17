//! `PropDebugView` impls for types not owned by the `floem` crate.
//!
//! These live here (rather than in `floem`) because the orphan rule requires
//! the impls to be in the crate that owns the trait. The impls are
//! renderer-agnostic: they call methods on `&dyn InspectorRender` instead of
//! constructing any concrete view type.

use std::any::Any;

use floem_renderer::text::{FontWeight, LineHeightValue};
use parley::{Alignment, FontStyle};
use peniko::kurbo::{self, Affine, Stroke};
use peniko::{Brush, Color, Gradient};
use smallvec::SmallVec;
use taffy::geometry::{MinMax, Size};
use taffy::prelude::{GridPlacement, Line};
use taffy::style::{
    AlignContent, AlignItems, BoxSizing, Display, FlexDirection, FlexWrap, LengthPercentage,
    MaxTrackSizingFunction, MinTrackSizingFunction, Overflow, Position,
};
use taffy::{GridAutoFlow, GridTemplateComponent};

#[cfg(not(target_arch = "wasm32"))]
use std::time::Duration;
#[cfg(target_arch = "wasm32")]
use web_time::Duration;

use crate::debug_view::PropDebugView;
use crate::inspector_render::InspectorRender;
use crate::prop_value::StylePropValue;
#[allow(deprecated)]
use crate::unit::{AnchorAbout, Angle, Length, LengthAuto, Pct, Pt, Px, PxPct, PxPctAuto};
use crate::values::{ObjectFit, ObjectPosition};

crate::no_debug_view!(
    i32,
    bool,
    f32,
    u16,
    usize,
    f64,
    Overflow,
    Display,
    Position,
    FlexDirection,
    FlexWrap,
    AlignItems,
    BoxSizing,
    AlignContent,
    GridTemplateComponent<String>,
    MinTrackSizingFunction,
    MaxTrackSizingFunction,
    GridAutoFlow,
    GridPlacement,
    String,
    Alignment,
    LineHeightValue,
    Size<LengthPercentage>,
    Angle,
    AnchorAbout,
    Duration,
);

impl<T, M> PropDebugView for MinMax<T, M> {}
impl<T> PropDebugView for Line<T> {}

impl PropDebugView for ObjectFit {
    fn debug_view(&self, r: &dyn InspectorRender) -> Option<Box<dyn Any>> {
        Some(r.object_fit(*self))
    }
}

impl PropDebugView for ObjectPosition {
    fn debug_view(&self, r: &dyn InspectorRender) -> Option<Box<dyn Any>> {
        Some(r.object_position(self))
    }
}

impl<T: PropDebugView> PropDebugView for Option<T> {
    fn debug_view(&self, r: &dyn InspectorRender) -> Option<Box<dyn Any>> {
        self.as_ref().and_then(|v| v.debug_view(r))
    }
}

impl<T: StylePropValue + PropDebugView + 'static> PropDebugView for Vec<T> {
    fn debug_view(&self, r: &dyn InspectorRender) -> Option<Box<dyn Any>> {
        if self.is_empty() {
            return Some(r.muted_text("[]"));
        }

        let items: Vec<Box<dyn Any>> = self
            .iter()
            .enumerate()
            .map(|(i, item)| {
                let content = item
                    .debug_view(r)
                    .unwrap_or_else(|| r.text(&format!("{:?}", item)));
                r.labelled(&format!("[{}]", i), content)
            })
            .collect();

        Some(r.vertical_list(items))
    }
}

impl<A: smallvec::Array> PropDebugView for SmallVec<A>
where
    <A as smallvec::Array>::Item: StylePropValue + PropDebugView,
{
    fn debug_view(&self, r: &dyn InspectorRender) -> Option<Box<dyn Any>> {
        if self.is_empty() {
            return Some(r.text("smallvec\n[]"));
        }

        let count = self.len();
        let is_spilled = self.spilled();

        let summary_label = if is_spilled {
            format!("smallvec\n[{}] (heap)", count)
        } else {
            format!("smallvec\n[{}] (inline)", count)
        };
        let summary = r.text(&summary_label);

        let items: Vec<Box<dyn Any>> = self
            .iter()
            .enumerate()
            .map(|(i, item)| {
                let content = item
                    .debug_view(r)
                    .unwrap_or_else(|| r.text(&format!("{:?}", item)));
                r.labelled(&format!("[{}]", i), content)
            })
            .collect();
        let details = r.vertical_list(items);

        Some(r.horizontal_pair(summary, details))
    }
}

impl PropDebugView for FontWeight {
    fn debug_view(&self, r: &dyn InspectorRender) -> Option<Box<dyn Any>> {
        Some(r.font_weight(*self, &format!("{self:?}")))
    }
}

impl PropDebugView for FontStyle {
    fn debug_view(&self, r: &dyn InspectorRender) -> Option<Box<dyn Any>> {
        Some(r.font_style(*self, &format!("{self:?}")))
    }
}

impl PropDebugView for Pt {
    fn debug_view(&self, r: &dyn InspectorRender) -> Option<Box<dyn Any>> {
        Some(r.text(&format!("{} pt", self.0)))
    }
}
#[allow(deprecated)]
impl PropDebugView for Px {
    fn debug_view(&self, r: &dyn InspectorRender) -> Option<Box<dyn Any>> {
        Pt(self.0).debug_view(r)
    }
}
impl PropDebugView for Pct {
    fn debug_view(&self, r: &dyn InspectorRender) -> Option<Box<dyn Any>> {
        Some(r.text(&format!("{}%", self.0)))
    }
}
impl PropDebugView for LengthAuto {
    fn debug_view(&self, r: &dyn InspectorRender) -> Option<Box<dyn Any>> {
        let label = match self {
            Self::Pt(v) => format!("{v} pt"),
            Self::Pct(v) => format!("{v}%"),
            Self::Em(v) => format!("{v} em"),
            Self::Lh(v) => format!("{v} lh"),
            Self::Auto => "auto".to_string(),
        };
        Some(r.text(&label))
    }
}
#[allow(deprecated)]
impl PropDebugView for PxPctAuto {
    fn debug_view(&self, r: &dyn InspectorRender) -> Option<Box<dyn Any>> {
        LengthAuto::from(*self).debug_view(r)
    }
}
impl PropDebugView for Length {
    fn debug_view(&self, r: &dyn InspectorRender) -> Option<Box<dyn Any>> {
        let label = match self {
            Self::Pt(v) => format!("{v} pt"),
            Self::Pct(v) => format!("{v}%"),
            Self::Em(v) => format!("{v} em"),
            Self::Lh(v) => format!("{v} lh"),
        };
        Some(r.text(&label))
    }
}
#[allow(deprecated)]
impl PropDebugView for PxPct {
    fn debug_view(&self, r: &dyn InspectorRender) -> Option<Box<dyn Any>> {
        Length::from(*self).debug_view(r)
    }
}

impl PropDebugView for Color {
    fn debug_view(&self, r: &dyn InspectorRender) -> Option<Box<dyn Any>> {
        Some(r.color(*self))
    }
}

impl PropDebugView for Gradient {
    fn debug_view(&self, r: &dyn InspectorRender) -> Option<Box<dyn Any>> {
        Some(r.gradient(self))
    }
}

impl PropDebugView for Stroke {
    fn debug_view(&self, r: &dyn InspectorRender) -> Option<Box<dyn Any>> {
        Some(r.stroke(self))
    }
}

impl PropDebugView for Brush {
    fn debug_view(&self, r: &dyn InspectorRender) -> Option<Box<dyn Any>> {
        match self {
            Brush::Solid(_) | Brush::Gradient(_) => Some(r.brush(self)),
            Brush::Image(_) => None,
        }
    }
}

impl PropDebugView for kurbo::Rect {
    fn debug_view(&self, r: &dyn InspectorRender) -> Option<Box<dyn Any>> {
        Some(r.rect(self))
    }
}

impl PropDebugView for Affine {
    fn debug_view(&self, r: &dyn InspectorRender) -> Option<Box<dyn Any>> {
        Some(r.affine(self))
    }
}
