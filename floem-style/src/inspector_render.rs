//! Renderer-abstraction for inspector previews.
//!
//! `PropDebugView` impls need to build view widgets (labels, canvases,
//! stacks, tooltips, ...) when a prop is rendered in the inspector. Those
//! widgets live in the `floem` crate, which `floem-style` can't name. To
//! break the cycle, `PropDebugView` impls take a `&dyn InspectorRender` and
//! ask it for a preview widget, returned as `Box<dyn Any>`. The concrete
//! implementor in `floem` downcasts back to `Box<dyn View>` at the call
//! site.
//!
//! Each method here corresponds to ONE kind of inspector preview. Keep the
//! surface small: only add a method when the current impl is
//! widget-heavy enough that keeping its body in `floem-style` would pull
//! in view types. Simpler impls can and do use `text(..)` / `sequence(..)`.

use std::any::Any;

use floem_renderer::text::FontWeight;
use parley::FontStyle;
use peniko::kurbo::{Affine, Rect, Stroke};
use peniko::{Brush, Color, Gradient};

use crate::transition::Transition;
use crate::values::{ObjectFit, ObjectPosition};

/// An abstract renderer for inspector previews. See module docs.
pub trait InspectorRender {
    /// A preview with no content. Use as a fallback when a value has no
    /// meaningful inspector rendering.
    fn empty(&self) -> Box<dyn Any>;

    /// A text preview. Used for all simple "just print a label" impls.
    fn text(&self, s: &str) -> Box<dyn Any>;

    /// A vertical sequence of previews.
    fn sequence(&self, items: Vec<Box<dyn Any>>) -> Box<dyn Any>;

    /// Color swatch with hex/rgba/components tooltip.
    fn color(&self, c: Color) -> Box<dyn Any>;

    /// Gradient preview box plus textual description.
    fn gradient(&self, g: &Gradient) -> Box<dyn Any>;

    /// Brush preview. Solid colors delegate to `color`; gradients to
    /// `gradient`; image brushes are currently not previewed.
    fn brush(&self, b: &Brush) -> Box<dyn Any>;

    /// Stroke line preview with width/join/caps/dash tooltip.
    fn stroke(&self, s: &Stroke) -> Box<dyn Any>;

    /// Rectangle preview: coords, width/height, and a scaled box.
    fn rect(&self, r: &Rect) -> Box<dyn Any>;

    /// 2D affine transform preview: matrix + decomposed translate/rotate/scale.
    fn affine(&self, a: &Affine) -> Box<dyn Any>;

    /// `ObjectFit` preview showing a simulated image fitted into a square.
    fn object_fit(&self, f: ObjectFit) -> Box<dyn Any>;

    /// `ObjectPosition` preview showing the anchor point of an image.
    fn object_position(&self, p: &ObjectPosition) -> Box<dyn Any>;

    /// Transition easing-curve preview with duration/easing tooltip.
    fn transition(&self, t: &Transition) -> Box<dyn Any>;

    /// Text styled with muted/deemphasized color, for placeholders like "[]".
    fn muted_text(&self, s: &str) -> Box<dyn Any>;

    /// A row of `[label] content`, typically for numbered list items.
    fn labelled(&self, label: &str, content: Box<dyn Any>) -> Box<dyn Any>;

    /// Vertical list of pre-rendered child items with a small gap.
    fn vertical_list(&self, items: Vec<Box<dyn Any>>) -> Box<dyn Any>;

    /// Two debug views side by side (summary + details).
    fn horizontal_pair(&self, first: Box<dyn Any>, second: Box<dyn Any>) -> Box<dyn Any>;

    /// Text rendered using a concrete `FontWeight` (for weight previews).
    fn font_weight(&self, weight: FontWeight, label: &str) -> Box<dyn Any>;

    /// Text rendered using a concrete `FontStyle` (for italic previews).
    fn font_style(&self, style: FontStyle, label: &str) -> Box<dyn Any>;
}
