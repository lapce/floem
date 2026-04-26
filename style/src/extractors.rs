//! Prop extractors the style engine runs during property extraction.
//!
//! These `prop_extractor!`-generated structs pull resolved values off the
//! cascaded [`Style`](crate::Style) and cache them as typed state. Hosts
//! read the extracted values during layout / paint / box-tree updates.
//!
//! - [`LayoutProps`] — sizes, padding, margin, insets, gap, flex metrics,
//!   plus font-size / line-height (kept alongside layout because `em` and
//!   `lh` unit resolution needs them).
//! - [`TransformProps`] — translate / scale / rotate, overflow, corner
//!   radii (these change the node's visual box geometry).
//! - [`ViewStyleProps`] — backgrounds, outlines, border colors, shadows.

use peniko::kurbo::{self, Affine, RoundedRect, Vec2};
use taffy::geometry::Rect;
use taffy::prelude::{LengthPercentage, Size};
use taffy::style::{Overflow, Style as TaffyStyle};

use crate::builtin_props::{
    Background, BorderBottom, BorderBottomColor, BorderBottomLeftRadius, BorderBottomRightRadius,
    BorderLeft, BorderLeftColor, BorderProgress, BorderRight, BorderRightColor, BorderTop,
    BorderTopColor, BorderTopLeftRadius, BorderTopRightRadius, BoxShadowProp, ColGap, FlexBasis,
    FlexGrow, FlexShrink, FontFamily, FontSize, FontStyle, FontWeight, Height, InsetBottom,
    InsetLeft, InsetRight, InsetTop, LineHeight, MarginBottom, MarginLeft, MarginRight, MarginTop,
    MaxHeight, MaxWidth, MinHeight, MinWidth, Outline, OutlineColor, OutlineProgress, OverflowX,
    OverflowY, PaddingBottom, PaddingLeft, PaddingRight, PaddingTop, RotateAbout, Rotation, RowGap,
    ScaleAbout, ScaleX, ScaleY, Transform, TranslateX, TranslateY, Width,
};
use crate::components::{Border, BorderColor, BorderRadius};
use crate::prop_extractor;
use crate::unit::FontSizeCx;

prop_extractor! {
    pub FontProps {
        pub size: FontSize,
        pub family: FontFamily,
        pub weight: FontWeight,
        pub style: FontStyle,
    }
}

prop_extractor! {
    pub LayoutProps {
        pub border_left: BorderLeft,
        pub border_top: BorderTop,
        pub border_right: BorderRight,
        pub border_bottom: BorderBottom,

        pub padding_left: PaddingLeft,
        pub padding_top: PaddingTop,
        pub padding_right: PaddingRight,
        pub padding_bottom: PaddingBottom,

        pub margin_left: MarginLeft,
        pub margin_top: MarginTop,
        pub margin_right: MarginRight,
        pub margin_bottom: MarginBottom,

        pub width: Width,
        pub height: Height,

        pub min_width: MinWidth,
        pub min_height: MinHeight,

        pub max_width: MaxWidth,
        pub max_height: MaxHeight,

        pub flex_grow: FlexGrow,
        pub flex_shrink: FlexShrink,
        pub flex_basis: FlexBasis,

        pub inset_left: InsetLeft,
        pub inset_top: InsetTop,
        pub inset_right: InsetRight,
        pub inset_bottom: InsetBottom,

        pub row_gap: RowGap,
        pub col_gap: ColGap,

        // Part of layout props because `em` / `lh` unit resolution needs them.
        pub font_size: FontSize,
        pub line_height: LineHeight,
    }
}

impl LayoutProps {
    pub fn border(&self) -> Border {
        Border {
            left: Some(self.border_left()),
            top: Some(self.border_top()),
            right: Some(self.border_right()),
            bottom: Some(self.border_bottom()),
        }
    }

    pub fn font_size_cx(&self) -> FontSizeCx {
        let font_size = self.font_size();
        let line_height = self.line_height();
        let line_height = line_height.resolve(font_size as f32);
        FontSizeCx::new(font_size, line_height as f64)
    }

    /// Push this extractor's resolved values onto a taffy `Style`. Hosts
    /// call this before handing the taffy style to a taffy layout engine.
    pub fn apply_to_taffy_style(&self, style: &mut TaffyStyle) {
        let resolve_cx = &self.font_size_cx();
        style.size = Size {
            width: self.width().to_taffy_dim(resolve_cx),
            height: self.height().to_taffy_dim(resolve_cx),
        };
        style.min_size = Size {
            width: self.min_width().to_taffy_dim(resolve_cx),
            height: self.min_height().to_taffy_dim(resolve_cx),
        };
        style.max_size = Size {
            width: self.max_width().to_taffy_dim(resolve_cx),
            height: self.max_height().to_taffy_dim(resolve_cx),
        };
        style.flex_grow = self.flex_grow();
        style.flex_shrink = self.flex_shrink();
        style.flex_basis = self.flex_basis().to_taffy_dim(resolve_cx);
        style.border = Rect {
            left: LengthPercentage::length(self.border_left().width as f32),
            top: LengthPercentage::length(self.border_top().width as f32),
            right: LengthPercentage::length(self.border_right().width as f32),
            bottom: LengthPercentage::length(self.border_bottom().width as f32),
        };
        style.padding = Rect {
            left: self.padding_left().to_taffy(resolve_cx),
            top: self.padding_top().to_taffy(resolve_cx),
            right: self.padding_right().to_taffy(resolve_cx),
            bottom: self.padding_bottom().to_taffy(resolve_cx),
        };
        style.margin = Rect {
            left: self.margin_left().to_taffy_len_perc_auto(resolve_cx),
            top: self.margin_top().to_taffy_len_perc_auto(resolve_cx),
            right: self.margin_right().to_taffy_len_perc_auto(resolve_cx),
            bottom: self.margin_bottom().to_taffy_len_perc_auto(resolve_cx),
        };
        style.inset = Rect {
            left: self.inset_left().to_taffy_len_perc_auto(resolve_cx),
            top: self.inset_top().to_taffy_len_perc_auto(resolve_cx),
            right: self.inset_right().to_taffy_len_perc_auto(resolve_cx),
            bottom: self.inset_bottom().to_taffy_len_perc_auto(resolve_cx),
        };
        style.gap = Size {
            width: self.col_gap().to_taffy(resolve_cx),
            height: self.row_gap().to_taffy(resolve_cx),
        };
    }
}

prop_extractor! {
    /// Properties that require a box-tree commit when they change.
    pub TransformProps {
        pub scale_x: ScaleX,
        pub scale_y: ScaleY,

        pub translate_x: TranslateX,
        pub translate_y: TranslateY,

        pub rotation: Rotation,
        pub rotate_about: RotateAbout,
        pub scale_about: ScaleAbout,

        pub transform: Transform,

        pub overflow_x: OverflowX,
        pub overflow_y: OverflowY,
        pub border_top_left_radius: BorderTopLeftRadius,
        pub border_top_right_radius: BorderTopRightRadius,
        pub border_bottom_left_radius: BorderBottomLeftRadius,
        pub border_bottom_right_radius: BorderBottomRightRadius,
    }
}

impl TransformProps {
    pub fn border_radius(&self) -> BorderRadius {
        BorderRadius {
            top_left: Some(self.border_top_left_radius()),
            top_right: Some(self.border_top_right_radius()),
            bottom_left: Some(self.border_bottom_left_radius()),
            bottom_right: Some(self.border_bottom_right_radius()),
        }
    }

    pub fn affine(&self, size: kurbo::Size, resolve_cx: &FontSizeCx) -> Affine {
        let mut result = Affine::IDENTITY;
        // CANONICAL ORDER (matches CSS individual properties):
        // 1. translate → 2. rotate → 3. scale → 4. transform property

        // 1. Translate
        let transform_x = self.translate_x().resolve(size.width, resolve_cx);
        let transform_y = self.translate_y().resolve(size.height, resolve_cx);
        result *= Affine::translate(Vec2 {
            x: transform_x,
            y: transform_y,
        });

        // 2. Rotate (around rotate_about anchor)
        let rotation = self.rotation().to_radians();
        if rotation != 0.0 {
            let rotate_about = self.rotate_about();
            let (rotate_x_frac, rotate_y_frac) = rotate_about.as_fractions();
            let rotate_point = Vec2 {
                x: rotate_x_frac * size.width,
                y: rotate_y_frac * size.height,
            };
            result *= Affine::translate(rotate_point)
                * Affine::rotate(rotation)
                * Affine::translate(-rotate_point);
        }

        // 3. Scale (around scale_about anchor)
        let scale_x = self.scale_x().0 / 100.;
        let scale_y = self.scale_y().0 / 100.;
        if scale_x != 1.0 || scale_y != 1.0 {
            let scale_about = self.scale_about();
            let (scale_x_frac, scale_y_frac) = scale_about.as_fractions();
            let scale_point = Vec2 {
                x: scale_x_frac * size.width,
                y: scale_y_frac * size.height,
            };
            result *= Affine::translate(scale_point)
                * Affine::scale_non_uniform(scale_x, scale_y)
                * Affine::translate(-scale_point);
        }

        // 4. Apply custom transform property last
        result *= self.transform();
        result
    }

    pub fn clip_rect(
        &self,
        mut local_rect: kurbo::Rect,
        resolve_cx: &FontSizeCx,
    ) -> Option<RoundedRect> {
        use Overflow::*;

        let (overflow_x, overflow_y) = (self.overflow_x(), self.overflow_y());

        // No clipping if both are visible.
        if overflow_x == Visible && overflow_y == Visible {
            return None;
        }

        let border_radius = self
            .border_radius()
            .resolve_border_radii(local_rect.size().min_side(), resolve_cx);

        // Extend to infinity on visible axes.
        if overflow_x == Visible {
            local_rect.x0 = f64::NEG_INFINITY;
            local_rect.x1 = f64::INFINITY;
        }
        if overflow_y == Visible {
            local_rect.y0 = f64::NEG_INFINITY;
            local_rect.y1 = f64::INFINITY;
        }

        Some(RoundedRect::from_rect(local_rect, border_radius))
    }
}

prop_extractor! {
    pub ViewStyleProps {
        pub border_top_left_radius: BorderTopLeftRadius,
        pub border_top_right_radius: BorderTopRightRadius,
        pub border_bottom_left_radius: BorderBottomLeftRadius,
        pub border_bottom_right_radius: BorderBottomRightRadius,
        pub border_progress: BorderProgress,

        pub outline: Outline,
        pub outline_color: OutlineColor,
        pub outline_progress: OutlineProgress,
        pub border_left_color: BorderLeftColor,
        pub border_top_color: BorderTopColor,
        pub border_right_color: BorderRightColor,
        pub border_bottom_color: BorderBottomColor,
        pub background: Background,
        pub shadow: BoxShadowProp,
    }
}

impl ViewStyleProps {
    pub fn border_radius(&self) -> BorderRadius {
        BorderRadius {
            top_left: Some(self.border_top_left_radius()),
            top_right: Some(self.border_top_right_radius()),
            bottom_left: Some(self.border_bottom_left_radius()),
            bottom_right: Some(self.border_bottom_right_radius()),
        }
    }

    pub fn border_color(&self) -> BorderColor {
        BorderColor {
            left: self.border_left_color(),
            top: self.border_top_color(),
            right: self.border_right_color(),
            bottom: self.border_bottom_color(),
        }
    }
}
