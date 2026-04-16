//! `StylePropValue` impl for `unic_langid::LanguageIdentifier`, gated behind
//! the `localization` feature.

use unic_langid::LanguageIdentifier;

use crate::prop_value::StylePropValue;

impl StylePropValue for LanguageIdentifier {
    fn interpolate(&self, _other: &Self, _value: f64) -> Option<Self> {
        None
    }
}
