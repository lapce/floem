//! Extension trait providing taffy-style conversion for [`Style`].
//!
//! These inherent methods used to live on `Style` but that became impossible
//! once [`Style`] moved to the `floem_style` crate — the orphan rule forbids
//! defining inherent `impl` blocks on a foreign type. Putting the methods on a
//! trait defined in `floem` keeps the call sites unchanged for code that has
//! `use floem::style::*` in scope, while respecting the crate boundary.

use floem_style::unit::FontSizeCx;
use taffy::{
    prelude::{Rect, Size},
    style::{LengthPercentage, Style as TaffyStyle},
};

use super::{OverflowX, OverflowY, Style};

/// Extension trait for converting a [`Style`] into a [`TaffyStyle`], plus the
/// `font_size_cx` helper used by the conversion.
///
/// Implemented only for [`Style`]. Call `use crate::style::StyleTaffyExt` at the
/// call site to bring the methods into scope.
pub trait StyleTaffyExt {
    /// Build a [`FontSizeCx`] from the style's font-size and line-height props.
    fn font_size_cx(&self) -> FontSizeCx;
    /// Convert to a [`taffy::style::Style`] used for layout computation.
    fn to_taffy_style(&self) -> TaffyStyle;
}

impl StyleTaffyExt for Style {
    fn font_size_cx(&self) -> FontSizeCx {
        let builtin = self.builtin();
        let font_size = builtin.font_size();
        let line_height = builtin.line_height();
        let line_height = line_height.resolve(font_size as f32);
        FontSizeCx::new(font_size, line_height as f64)
    }

    fn to_taffy_style(&self) -> TaffyStyle {
        let style = self.builtin();
        let font_size_cx = self.font_size_cx();
        TaffyStyle {
            display: style.display(),
            overflow: taffy::Point {
                x: self.get(OverflowX),
                y: self.get(OverflowY),
            },
            position: style.position(),
            size: Size {
                width: style.width().to_taffy_dim(&font_size_cx),
                height: style.height().to_taffy_dim(&font_size_cx),
            },
            min_size: Size {
                width: style.min_width().to_taffy_dim(&font_size_cx),
                height: style.min_height().to_taffy_dim(&font_size_cx),
            },
            max_size: Size {
                width: style.max_width().to_taffy_dim(&font_size_cx),
                height: style.max_height().to_taffy_dim(&font_size_cx),
            },
            flex_direction: style.flex_direction(),
            flex_grow: style.flex_grow(),
            flex_shrink: style.flex_shrink(),
            flex_basis: style.flex_basis().to_taffy_dim(&font_size_cx),
            flex_wrap: style.flex_wrap(),
            justify_content: style.justify_content(),
            justify_self: style.justify_self(),
            justify_items: style.justify_items(),
            align_items: style.align_items(),
            align_content: style.align_content(),
            align_self: style.align_self(),
            aspect_ratio: style.aspect_ratio(),
            border: Rect {
                left: LengthPercentage::length(style.border_left().width as f32),
                top: LengthPercentage::length(style.border_top().width as f32),
                right: LengthPercentage::length(style.border_right().width as f32),
                bottom: LengthPercentage::length(style.border_bottom().width as f32),
            },
            padding: Rect {
                left: style.padding_left().to_taffy(&font_size_cx),
                top: style.padding_top().to_taffy(&font_size_cx),
                right: style.padding_right().to_taffy(&font_size_cx),
                bottom: style.padding_bottom().to_taffy(&font_size_cx),
            },
            margin: Rect {
                left: style.margin_left().to_taffy_len_perc_auto(&font_size_cx),
                top: style.margin_top().to_taffy_len_perc_auto(&font_size_cx),
                right: style.margin_right().to_taffy_len_perc_auto(&font_size_cx),
                bottom: style.margin_bottom().to_taffy_len_perc_auto(&font_size_cx),
            },
            inset: Rect {
                left: style.inset_left().to_taffy_len_perc_auto(&font_size_cx),
                top: style.inset_top().to_taffy_len_perc_auto(&font_size_cx),
                right: style.inset_right().to_taffy_len_perc_auto(&font_size_cx),
                bottom: style.inset_bottom().to_taffy_len_perc_auto(&font_size_cx),
            },
            gap: Size {
                width: style.col_gap().to_taffy(&font_size_cx),
                height: style.row_gap().to_taffy(&font_size_cx),
            },
            grid_template_rows: style.grid_template_rows(),
            grid_template_columns: style.grid_template_columns(),
            grid_row: style.grid_row(),
            grid_column: style.grid_column(),
            grid_auto_rows: style.grid_auto_rows(),
            grid_auto_columns: style.grid_auto_columns(),
            grid_auto_flow: style.grid_auto_flow(),
            scrollbar_width: style.scrollbar_width().0 as f32,
            ..Default::default()
        }
    }
}
