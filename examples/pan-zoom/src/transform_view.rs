use floem::{
    IntoView, View, ViewId,
    context::{ComputeLayoutCx, UpdateCx},
    kurbo,
    reactive::{RwSignal, SignalGet, SignalUpdate, create_updater},
    unit::Pct,
    views::{Decorators, clip},
};

/// Transform a child view without changing its layout.
/// Possible transformations at the moment are translation and scaling.
/// Mathematically, these transformations are represented by an affine transformation matrix.
///
/// # Translation
///
/// ```rust
/// use floem::{kurbo, views::*};
/// use ui::transform_view;
/// transform_view(button("Button"), || kurbo::Affine::translate((42.0, 42.0)));
/// ```
///
/// # Scaling
///
/// ```rust
/// use floem::{kurbo, views::*};
/// use ui::transform_view;
/// transform_view(button("Button"), || kurbo::Affine::scale(0.5));
/// ```
///
/// Note that while the interface allows for rotation and skewing transformations, these are not implemented, yet.
pub fn transform_view<V: IntoView + 'static>(
    child: V,
    affine: impl Fn() -> kurbo::Affine + 'static,
) -> TransformView {
    let id = floem::ViewId::new();
    let view_transform_signal = RwSignal::new(affine());
    create_updater(affine, move |new_view_transform| {
        id.update_state(new_view_transform);
    });

    // Wrapping child in a container. In case child has margins, this ensures we get the proper dimensions
    // to calculate the correct transformation.
    let child = clip(child);
    let child_center = RwSignal::new(
        child
            .id()
            .layout_rect()
            .with_origin(kurbo::Point::ZERO)
            .center(),
    );
    let child = child.style(move |s| {
        let scale = view_transform_signal.get().scale();
        let (adjusted_tx, adjusted_ty) = view_transform_signal
            .get()
            .adjusted_translate(child_center.get());
        s.scale(scale)
            .translate_x(adjusted_tx)
            .translate_y(adjusted_ty)
    });
    let child_id = child.id();
    id.set_children([child]);

    TransformView {
        id,
        child_id,
        child_center,
        view_transform: view_transform_signal,
    }
}

pub struct TransformView {
    id: ViewId,
    child_id: ViewId,
    view_transform: RwSignal<kurbo::Affine>,
    child_center: RwSignal<kurbo::Point>,
}

impl TransformView {
    fn update_size(&mut self) {
        let child_rect = self.child_id.layout_rect().with_origin(kurbo::Point::ZERO);
        self.child_center.set(child_rect.center());
    }
}

impl View for TransformView {
    fn id(&self) -> ViewId {
        self.id
    }

    fn update(&mut self, cx: &mut UpdateCx, state: Box<dyn std::any::Any>) {
        if let Ok(state) = state.downcast() {
            self.view_transform.set(*state);
            let window_state = &mut cx.window_state;
            window_state.request_compute_layout_recursive(self.id());
            window_state.request_paint(self.id());
        }
    }

    fn compute_layout(&mut self, cx: &mut ComputeLayoutCx) -> Option<kurbo::Rect> {
        self.update_size();
        cx.compute_view_layout(self.child_id);
        None
    }
}

trait ExtractTransform {
    fn scale(&self) -> Pct;
    fn adjusted_translate(&self, center: kurbo::Point) -> (f64, f64);
}

impl ExtractTransform for kurbo::Affine {
    fn scale(&self) -> Pct {
        let coeffs = self.as_coeffs();
        Pct(coeffs[0].hypot(coeffs[1]) * 100.0)
    }

    /// Floem scales the child from its center.
    /// The affine transformation matrix scales the child from the origin.
    /// Hence, we need to adjust the translation to achieve the desired effect.
    fn adjusted_translate(&self, center: kurbo::Point) -> (f64, f64) {
        let scale = self.scale();
        let translation = self.translation();
        let adjusted_tx = translation.x - center.x * (1.0 - (scale.0 / 100.0));
        let adjusted_ty = translation.y - center.y * (1.0 - (scale.0 / 100.0));
        (adjusted_tx, adjusted_ty)
    }
}
