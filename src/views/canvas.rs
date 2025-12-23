use floem_reactive::SignalTracker;
use peniko::kurbo::Size;

use crate::{context::PaintCx, view::ViewId, view::View};

/// A Canvas view. See the docs for [canvas()].
#[allow(clippy::type_complexity)]
pub struct Canvas {
    id: ViewId,
    paint_fn: Box<dyn Fn(&mut PaintCx, Size)>,
    size: Size,
    tracker: Option<SignalTracker>,
}

/// Creates a new Canvas view that can be used for custom painting
///
/// A [`Canvas`] provides a low-level interface for custom drawing operations. The supplied
/// paint function will be called whenever the view needs to be rendered, and any signals accessed
/// within the paint function will automatically trigger repaints when they change.
///
///
/// # Example
/// ```rust
/// use floem::prelude::*;
/// use palette::css;
/// use peniko::kurbo::Rect;
/// canvas(move |cx, size| {
///     cx.fill(
///         &Rect::ZERO
///             .with_size(size)
///             .to_rounded_rect(8.),
///         css::PURPLE,
///         0.,
///     );
/// })
/// .style(|s| s.size(100, 300));
/// ```
pub fn canvas(paint: impl Fn(&mut PaintCx, Size) + 'static) -> Canvas {
    let id = ViewId::new();

    Canvas {
        id,
        paint_fn: Box::new(paint),
        size: Default::default(),
        tracker: None,
    }
}

impl View for Canvas {
    fn id(&self) -> ViewId {
        self.id
    }

    fn debug_name(&self) -> std::borrow::Cow<'static, str> {
        "Canvas".into()
    }

    fn compute_layout(
        &mut self,
        _cx: &mut crate::context::ComputeLayoutCx,
    ) -> Option<peniko::kurbo::Rect> {
        self.size = self.id.get_size().unwrap_or_default();
        None
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        let id = self.id;
        let paint = &self.paint_fn;

        if self.tracker.is_none() {
            self.tracker = Some(SignalTracker::new(move || {
                id.request_paint();
            }));
        }

        let tracker = self.tracker.as_ref().unwrap();
        tracker.track(|| {
            paint(cx, self.size);
        });
    }
}
