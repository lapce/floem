//! `StylePropValue` impls for taffy types.

use taffy::GridTemplateComponent;
use taffy::geometry::{MinMax, Size};
use taffy::prelude::{GridPlacement, Line};
use taffy::style::{
    AlignContent, AlignItems, BoxSizing, Display, FlexDirection, FlexWrap, LengthPercentage,
    MaxTrackSizingFunction, MinTrackSizingFunction, Overflow, Position,
};

use crate::prop_value::{StylePropValue, hash_value};

// Taffy enums — use discriminant-based hashing (no Hash derive available).
// For simple enums with no data fields, discriminant alone is a perfect hash.
// For enums with data, we combine discriminant with field hashes.

impl StylePropValue for Overflow {
    fn content_hash(&self) -> u64 {
        hash_value(&std::mem::discriminant(self))
    }
}
impl StylePropValue for Display {
    fn content_hash(&self) -> u64 {
        hash_value(&std::mem::discriminant(self))
    }
}
impl StylePropValue for Position {
    fn content_hash(&self) -> u64 {
        hash_value(&std::mem::discriminant(self))
    }
}
impl StylePropValue for FlexDirection {
    fn content_hash(&self) -> u64 {
        hash_value(&std::mem::discriminant(self))
    }
}
impl StylePropValue for FlexWrap {
    fn content_hash(&self) -> u64 {
        hash_value(&std::mem::discriminant(self))
    }
}
impl StylePropValue for AlignItems {
    fn content_hash(&self) -> u64 {
        hash_value(&std::mem::discriminant(self))
    }
}
impl StylePropValue for BoxSizing {
    fn content_hash(&self) -> u64 {
        hash_value(&std::mem::discriminant(self))
    }
}
impl StylePropValue for AlignContent {
    fn content_hash(&self) -> u64 {
        hash_value(&std::mem::discriminant(self))
    }
}
impl StylePropValue for GridTemplateComponent<String> {}
impl StylePropValue for MinTrackSizingFunction {}
impl StylePropValue for MaxTrackSizingFunction {}
impl<T: StylePropValue, M: StylePropValue> StylePropValue for MinMax<T, M> {
    fn content_hash(&self) -> u64 {
        hash_value(&(self.min.content_hash(), self.max.content_hash()))
    }
}
impl<T: StylePropValue> StylePropValue for Line<T> {
    fn content_hash(&self) -> u64 {
        hash_value(&(self.start.content_hash(), self.end.content_hash()))
    }
}
impl StylePropValue for taffy::GridAutoFlow {
    fn content_hash(&self) -> u64 {
        hash_value(&std::mem::discriminant(self))
    }
}
impl StylePropValue for GridPlacement {}
impl StylePropValue for Size<LengthPercentage> {}
