//! `StylePropValue` and `PropDebugView` impls for
//! `unic_langid::LanguageIdentifier`, gated behind the `localization` feature.

use std::any::Any;

use unic_langid::LanguageIdentifier;

use crate::debug_view::PropDebugView;
use crate::inspector_render::InspectorRender;
use crate::prop_value::StylePropValue;

impl StylePropValue for LanguageIdentifier {
    fn interpolate(&self, _other: &Self, _value: f64) -> Option<Self> {
        None
    }
}

impl PropDebugView for LanguageIdentifier {
    fn debug_view(&self, r: &dyn InspectorRender) -> Option<Box<dyn Any>> {
        Some(r.text(&format!("{self:?}")))
    }
}
