//! `StyleDebugViewExt` — the inspector entry point for rendering a `Style`
//! as a tree of debug rows.
//!
//! This is an extension trait rather than an inherent method on `Style` so
//! that `Style` itself can eventually move into `floem_style` (which doesn't
//! know about `View` or the widget layer). The implementation body continues
//! to live in `src/style/values.rs` alongside the `style_debug_*` helper
//! functions it drives.

use crate::View;
use crate::style::Style;

pub trait StyleDebugViewExt {
    /// Render this style as a tabbed inspector view (properties, selectors,
    /// classes). `direct_style`, when provided, marks props that were set
    /// directly on the view vs inherited from context.
    fn debug_view(&self, direct_style: Option<&Style>) -> Box<dyn View>;
}
