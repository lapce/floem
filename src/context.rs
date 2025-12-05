use crate::{
    VisualId,
    action::exec_after,
    animate::{AnimStateKind, RepeatMode},
    easing::{Easing, Linear},
    event::EventPropagation,
    id::ViewId,
    inspector::CaptureState,
    menu::Menu,
    renderer::Renderer,
    responsive::ScreenSizeBp,
    style::{Style, StyleClassRef, StyleProp, resolve_nested_maps},
    view::{View, paint_bg, paint_border, paint_outline},
    window_state::WindowState,
};
use floem_reactive::Scope;
use floem_renderer::{
    Renderer as FloemRenderer,
    gpu_resources::{GpuResourceError, GpuResources},
};
use peniko::BlendMode;
use peniko::kurbo::{Affine, Point, Rect, RoundedRect, Shape, Size, Vec2};
use smallvec::SmallVec;
use std::{
    ops::{Deref, DerefMut},
    rc::Rc,
    sync::Arc,
};
use taffy::Layout;
use winit::window::Window;

#[cfg(not(target_arch = "wasm32"))]
use std::time::{Duration, Instant};
#[cfg(target_arch = "wasm32")]
use web_time::{Duration, Instant};

#[cfg(feature = "crossbeam")]
use crossbeam::channel::Receiver;
#[cfg(not(feature = "crossbeam"))]
use std::sync::mpsc::Receiver;

pub type EventCallback = dyn FnMut(&mut dyn View, &mut EventCx) -> EventPropagation;
pub type ResizeCallback = dyn Fn(Rect);
pub type MenuCallback = dyn Fn() -> Menu;

#[derive(Default)]
pub(crate) struct ResizeListeners {
    pub(crate) rect: Rect,
    pub(crate) callbacks: Vec<Rc<ResizeCallback>>,
}

/// Listeners for when the view moves to a different position in the window
#[derive(Default)]
pub(crate) struct MoveListeners {
    pub(crate) window_origin: Point,
    pub(crate) callbacks: Vec<Rc<dyn Fn(Point)>>,
}

pub(crate) type CleanupListeners = Vec<Rc<dyn Fn()>>;

pub struct DragState {
    pub(crate) id: ViewId,
    pub(crate) offset: Vec2,
    pub(crate) released_at: Option<Instant>,
    pub(crate) release_location: Option<Point>,
}

pub(crate) enum FrameUpdate {
    Style(ViewId),
    Layout,
    Paint(ViewId),
}

mod event;
pub use event::*;

#[derive(Debug, Default)]
pub struct InteractionState {
    pub is_hovered: bool,
    pub is_selected: bool,
    pub is_disabled: bool,
    pub is_focused: bool,
    pub is_clicking: bool,
    pub is_dark_mode: bool,
    pub is_file_hover: bool,
    pub using_keyboard_navigation: bool,
    pub is_hidden: bool,
}

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct InheritedInteractionCx {
    pub(crate) disabled: bool,
    pub(crate) hidden: bool,
    pub(crate) selected: bool,
}
impl InheritedInteractionCx {
    fn clear(&mut self) {
        self.hidden = false;
        self.disabled = false;
        self.selected = false;
    }
}
impl std::ops::BitOrAssign for InheritedInteractionCx {
    fn bitor_assign(&mut self, rhs: Self) {
        self.disabled |= rhs.disabled;
        self.hidden |= rhs.hidden;
        self.selected |= rhs.selected;
    }
}
impl std::ops::BitOr for InheritedInteractionCx {
    type Output = Self;

    fn bitor(mut self, rhs: Self) -> Self::Output {
        self |= rhs;
        self
    }
}

pub struct StyleCx<'a> {
    pub window_state: &'a mut WindowState,
    pub target: VisualId,
    /// current_view is used in prop readers for sending frame updates to request styles for the next frame if there are transitions
    pub(crate) current_view: ViewId,
    /// current is used as context for carrying inherited properties between views
    pub(crate) current: Style,
    pub(crate) direct: Style,
    pub(crate) now: Instant,
    pub disabled: bool,
    pub hidden: bool,
    pub selected: bool,
}
impl StyleCx<'_> {
    fn interaction_cx(&self) -> InheritedInteractionCx {
        InheritedInteractionCx {
            disabled: self.disabled,
            hidden: self.hidden,
            selected: self.selected,
        }
    }
}

impl<'a> StyleCx<'a> {
    pub(crate) fn new(
        window_state: &'a mut WindowState,
        now: Instant,
        view_id: ViewId,
        target: VisualId,
        default_style: impl FnOnce() -> Style,
    ) -> Self {
        let parent_cx = if let Some(p) = view_id.parent() {
            // First check if style_cx is already set
            if let Some(style_cx) = p.state().borrow().style_cx.clone() {
                style_cx
            } else if p.parent().is_some() {
                // If not set and we're not at root, recursively compute it
                let parent_target = p.visual_id();
                let mut parent_style_cx =
                    StyleCx::new(window_state, now, p, parent_target, default_style);
                parent_style_cx.style_target();
                p.state()
                    .borrow()
                    .style_cx
                    .clone()
                    .expect("cached in style_target")
            } else {
                // We're at root, set it to default
                let default = default_style();
                p.state().borrow_mut().style_cx = Some(default.clone());
                default
            }
        } else {
            default_style()
        };

        let parent_style_interaction_cx = view_id
            .parent()
            .map(|p| {
                let parent_state = p.state();
                let parent_state = parent_state.borrow();
                parent_state.style_interaction_cx
            })
            .unwrap_or_default();

        Self {
            window_state,
            target,
            current_view: view_id,
            current: parent_cx,
            direct: Default::default(),
            now: Instant::now(),
            disabled: parent_style_interaction_cx.disabled,
            hidden: parent_style_interaction_cx.hidden,
            selected: parent_style_interaction_cx.selected,
        }
    }

    pub fn get_interact_state(&self, id: impl Into<VisualId>) -> InteractionState {
        let id = id.into();
        let icx = id
            .view_of()
            .map(|v| {
                let view_state = v.state();
                let view_state = view_state.borrow();
                view_state.parent_set_style_interaction_cx
            })
            .unwrap_or_default();
        InteractionState {
            is_selected: self.selected | icx.selected,
            is_disabled: self.disabled | icx.disabled,
            is_hidden: self.hidden | icx.hidden,
            is_hovered: self.window_state.is_hovered(id),
            is_focused: self.window_state.is_focused(id),
            is_clicking: self.window_state.is_clicking(id) || self.window_state.active == Some(id),
            is_dark_mode: self.window_state.is_dark_mode(),
            is_file_hover: self.window_state.is_file_hover(id),
            using_keyboard_navigation: self.window_state.keyboard_navigation,
        }
    }

    pub fn style_target(&mut self) {
        let view_id = self.current_view;
        let view = view_id.view();
        let visual_id = view_id.visual_id();
        let view_state = view_id.state();

        {
            let mut view_state = view_state.borrow_mut();
            if self.window_state.view_style_dirty.contains(&view_id) {
                self.window_state.view_style_dirty.remove(&view_id);
                if let Some(view_style) = view.borrow().view_style() {
                    let offset = view_state.view_style_offset;
                    view_state.style.set(offset, view_style);
                }
            }

            // Propagate style requests to children if needed.
            if view_state.request_style_recursive {
                view_state.request_style_recursive = false;
                let children = view_id.children();
                for child in children {
                    let view_state = child.state();
                    let mut state = view_state.borrow_mut();
                    let visual_id = state.visual_id;
                    state.request_style_recursive = true;
                    self.window_state.style_dirty.insert(visual_id);
                }
            }
        }

        let view_interact_state = self.get_interact_state(visual_id);
        let view_class = view.borrow().view_class();

        let (mut new_frame, classes_applied) = self.compute_combined(
            view_interact_state,
            self.window_state.screen_size_bp,
            view_class,
            &self.current.clone(),
        );

        if classes_applied {
            let children = view_id.children();
            for child in children {
                let view_state = child.state();
                let mut state = view_state.borrow_mut();
                let visual_id = state.visual_id;
                state.request_style_recursive = true;
                self.window_state.style_dirty.insert(visual_id);
            }
        }

        self.direct = view_state.borrow().combined_style.clone();
        self.current.apply_mut(self.direct.inherited());
        let computed_style = self.current.clone().apply(self.direct.clone());
        CaptureState::capture_style(view_id, self, computed_style.clone());
        view_state.borrow_mut().computed_style = computed_style.clone();
        self.hidden |= computed_style.builtin().is_hidden()
            | (computed_style.builtin().display() == taffy::Display::None);
        self.disabled |= computed_style.builtin().is_disabled();
        self.selected |= computed_style.builtin().is_selected();
        view_state.borrow_mut().style_cx = Some(self.current.clone());
        view_state.borrow_mut().style_interaction_cx = self.interaction_cx();

        {
            let mut view_state = view_state.borrow_mut();
            // Extract the relevant layout properties so the content rect can be calculated
            // when painting.
            view_state.layout_props.read_explicit(
                &self.direct,
                &self.current,
                &self.now,
                &mut new_frame,
            );
            if new_frame {
                // If any transitioning layout props, schedule layout.
                self.window_state.schedule_layout();
            }

            view_state.view_style_props.read_explicit(
                &self.direct,
                &self.current,
                &self.now,
                &mut new_frame,
            );

            if view_state.view_transform_props.read_explicit(
                &self.direct,
                &self.current,
                &self.now,
                &mut new_frame,
            ) {
                self.window_state.schedule_layout();
            }

            if view_state.box_tree_props.read_explicit(
                &self.direct,
                &self.current,
                &self.now,
                &mut new_frame,
            ) {
                let visual_id = view_state.visual_id;
                let box_tree = view_id.box_tree();
                let mut box_tree = box_tree.borrow_mut();
                view_state
                    .box_tree_props
                    .set_box_tree(visual_id, &mut box_tree);
                self.window_state.schedule_layout();
            }

            view_state.style_cursor = computed_style.builtin().cursor();

            if new_frame && !self.hidden {
                self.window_state.schedule_style(view_id);
            }
        }

        // If there's any changes to the Taffy style, request layout.
        let layout_style = view_state.borrow().layout_props.to_style();
        let taffy_style = self.direct.clone().apply(layout_style).to_taffy_style();
        if taffy_style != view_state.borrow().taffy_style {
            view_state.borrow_mut().taffy_style = taffy_style.clone();
            let node = view_state.borrow().node;
            let _ = view_id.taffy().borrow_mut().set_style(node, taffy_style);
            self.window_state.schedule_layout();
        }

        view.borrow_mut().style_pass(self);

        let mut is_hidden_state = view_state.borrow().is_hidden_state;
        let computed_display = view_state.borrow().combined_style.builtin().display();
        is_hidden_state.transition(
            computed_display,
            || {
                let count = animations_on_remove(view_id, Scope::current());
                view_state.borrow_mut().num_waiting_animations = count;
                count > 0
            },
            || {
                animations_on_create(view_id);
            },
            || {
                stop_reset_remove_animations(view_id);
            },
            || view_state.borrow().num_waiting_animations,
        );

        view_state.borrow_mut().is_hidden_state = is_hidden_state;
        let modified = view_state
            .borrow()
            .combined_style
            .clone()
            .apply_opt(is_hidden_state.get_display(), Style::display);

        view_state.borrow_mut().combined_style = modified;
    }

    /// the first returned bool is new_frame. the second is classes_applied
    /// Returns `true` if a new frame is requested.
    ///
    // The context has the nested maps of classes and inherited properties
    fn compute_combined(
        &mut self,
        interact_state: InteractionState,
        screen_size_bp: ScreenSizeBp,
        view_class: Option<StyleClassRef>,
        context: &Style,
    ) -> (bool, bool) {
        let mut new_frame = false;

        // Build the initial combined style
        let mut combined_style = Style::new();

        let mut classes: SmallVec<[_; 4]> = SmallVec::new();

        // Apply view class if provided
        if let Some(view_class) = view_class {
            classes.insert(0, view_class);
        }

        let state = self.current_view.state();
        let mut state = state.borrow_mut();

        for class in &state.classes {
            classes.push(*class);
        }

        let mut new_context = context.clone();
        let mut new_classes = false;

        let (resolved_style, classes_applied) = resolve_nested_maps(
            combined_style,
            &interact_state,
            screen_size_bp,
            &classes,
            &mut new_context,
        );
        combined_style = resolved_style;
        new_classes |= classes_applied;

        let self_style = state.style();

        combined_style.apply_mut(self_style.clone());

        let (resolved_style, classes_applied) = resolve_nested_maps(
            combined_style,
            &interact_state,
            screen_size_bp,
            &classes,
            &mut new_context,
        );
        combined_style = resolved_style;
        new_classes |= classes_applied;

        // Process animations
        for animation in state
            .animations
            .stack
            .iter_mut()
            .filter(|anim| anim.can_advance() || anim.should_apply_folded())
        {
            if animation.can_advance() {
                new_frame = true;
                animation.animate_into(&mut combined_style);
                animation.advance();
            } else {
                animation.apply_folded(&mut combined_style)
            }
            debug_assert!(!animation.is_idle());
        }

        // Apply visibility
        if interact_state.is_hidden {
            combined_style = combined_style.hide();
        }

        state.combined_style = combined_style;
        (new_frame, new_classes)
    }

    pub fn now(&self) -> Instant {
        self.now
    }

    pub fn get_prop<P: StyleProp>(&self, _prop: P) -> Option<P::Type> {
        self.direct
            .get_prop::<P>()
            .or_else(|| self.current.get_prop::<P>())
    }

    pub fn style(&self) -> Style {
        self.current.clone().apply(self.direct.clone())
    }

    pub fn direct_style(&self) -> &Style {
        &self.direct
    }

    pub fn indirect_style(&self) -> &Style {
        &self.current
    }

    pub fn request_transition(&mut self) {
        let id = self.current_view;
        self.window_state.schedule_style(id);
    }
}

std::thread_local! {
    /// Holds the ID of a View being painted very briefly if it is being rendered as
    /// a moving drag image.  Since that is a relatively unusual thing to need, it
    /// makes more sense to use a thread local for it and avoid cluttering the fields
    /// and memory footprint of PaintCx or PaintState or ViewId with a field for it.
    /// This is ephemerally set before paint calls that are painting the view in a
    /// location other than its natural one for purposes of drag and drop.
    static CURRENT_DRAG_PAINTING_ID : std::cell::Cell<Option<ViewId>> = const { std::cell::Cell::new(None) };
}

pub struct PaintCx<'a> {
    pub window_state: &'a mut WindowState,
    pub(crate) paint_state: &'a mut PaintState,
    pub(crate) transform: Affine,
    pub(crate) clip: Option<RoundedRect>,
    pub(crate) z_index: Option<i32>,
    pub(crate) saved_clips: Vec<Option<RoundedRect>>,
    pub(crate) saved_z_indexes: Vec<Option<i32>>,
    pub gpu_resources: Option<GpuResources>,
    pub window: Arc<dyn Window>,
    #[cfg(feature = "vello")]
    pub layer_count: usize,
    #[cfg(feature = "vello")]
    pub saved_layer_counts: Vec<usize>,
}

impl PaintCx<'_> {
    pub fn save(&mut self) {
        self.saved_clips.push(self.clip);
        self.saved_z_indexes.push(self.z_index);
        #[cfg(feature = "vello")]
        self.saved_layer_counts.push(self.layer_count);
    }

    pub fn restore(&mut self) {
        #[cfg(feature = "vello")]
        {
            let saved_count = self.saved_layer_counts.pop().unwrap_or_default();
            while self.layer_count > saved_count {
                self.pop_layer();
                self.layer_count -= 1;
            }
        }

        self.clip = self.saved_clips.pop().unwrap_or_default();
        self.z_index = self.saved_z_indexes.pop().unwrap_or_default();
        if let Some(z_index) = self.z_index {
            self.paint_state.renderer_mut().set_z_index(z_index);
        } else {
            self.paint_state.renderer_mut().set_z_index(0);
        }

        #[cfg(not(feature = "vello"))]
        {
            if let Some(rect) = self.clip {
                self.paint_state.renderer_mut().clip(&rect);
            } else {
                self.paint_state.renderer_mut().clear_clip();
            }
        }
    }

    /// Allows a `View` to determine if it is being called in order to
    /// paint a *draggable* image of itself during a drag (likely
    /// `draggable()` was called on the `View` or `ViewId`) as opposed
    /// to a normal paint in order to alter the way it renders itself.
    pub fn is_drag_paint(&self, id: ViewId) -> bool {
        // This could be an associated function, but it is likely
        // a Good Thing to restrict access to cases when the caller actually
        // has a PaintCx, and that doesn't make it a breaking change to
        // use instance methods in the future.
        if let Some(dragging) = CURRENT_DRAG_PAINTING_ID.get() {
            return dragging == id;
        }
        false
    }

    /// paint the children of this view
    pub fn paint_children(&mut self, id: ViewId) {
        let children = id.children();
        for child in children {
            self.paint_view(child);
        }
    }

    /// The entry point for painting a view. You shouldn't need to implement this yourself. Instead, implement [`View::paint`].
    /// It handles the internal work before and after painting [`View::paint`] implementations.
    /// It is responsible for
    /// - managing hidden status
    /// - clipping
    /// - painting computed styles like background color, border, font-styles, and z-index and handling painting requirements of drag and drop
    pub fn paint_view(&mut self, id: ViewId) {
        if id.is_hidden() {
            return;
        }
        let view = id.view();
        let view_state = id.state();
        // Apply this view's accumulated transform from the cache
        // This transform already includes all ancestor transforms, so children
        // will set their own transforms when paint_view is called on them
        let transform = id.world_transform();
        if let Some(transform) = transform {
            self.set_transform(transform);
        }

        // Save clip and z-index state (but not transform, since we're using absolute transforms)
        self.save();
        let alpha = view_state.borrow().computed_style.builtin().opacity();
        let size = id.layout_rect_local().size();
        if alpha != 1. {
            self.push_layer(
                BlendMode::default(),
                alpha,
                Affine::IDENTITY,
                &size.to_rect(),
            );
        }

        let is_empty = self
            .clip
            .map(|rect| rect.rect().intersect(size.to_rect()).is_zero_area())
            .unwrap_or(false);

        if !is_empty {
            let style = view_state.borrow().combined_style.clone();
            let view_style_props = view_state.borrow().view_style_props.clone();
            let layout_props = view_state.borrow().layout_props.clone();

            if let Some(z_index) = style.builtin().z_index() {
                self.set_z_index(z_index);
            }

            paint_bg(self, &view_style_props, size);

            // Paint the view's content and children in the transformed space
            view.borrow_mut().paint(self);

            paint_border(self, &layout_props, &view_style_props, size);
            paint_outline(self, &view_style_props, size);
        }

        let mut drag_set_to_none = false;

        if let Some(dragging) = self.window_state.dragging.as_ref() {
            if dragging.id == id {
                let transform = if let Some((released_at, release_location)) =
                    dragging.released_at.zip(dragging.release_location)
                {
                    let easing = Linear;
                    const ANIMATION_DURATION_MS: f64 = 300.0;
                    let elapsed = released_at.elapsed().as_millis() as f64;
                    let progress = elapsed / ANIMATION_DURATION_MS;

                    if !(easing.finished(progress)) {
                        let offset_scale = 1.0 - easing.eval(progress);
                        let release_offset = release_location.to_vec2() - dragging.offset;

                        exec_after(Duration::from_millis(8), move |_| {
                            id.request_paint();
                        });

                        Some(self.transform * Affine::translate(release_offset * offset_scale))
                    } else {
                        drag_set_to_none = true;
                        None
                    }
                } else {
                    let translation = self.window_state.last_pointer.to_vec2() - dragging.offset;
                    Some(self.transform.with_translation(translation))
                };

                if let Some(transform) = transform {
                    self.save();
                    self.transform = transform;
                    self.paint_state
                        .renderer_mut()
                        .set_transform(self.transform);
                    self.set_z_index(1000);
                    self.clear_clip();

                    let style = view_state.borrow().combined_style.clone();
                    let mut view_style_props = view_state.borrow().view_style_props.clone();

                    if let Some(dragging_style) = view_state.borrow().dragging_style.clone() {
                        let style = style.apply(dragging_style);
                        let mut _new_frame = false;
                        view_style_props.read_explicit(
                            &style,
                            &style,
                            &Instant::now(),
                            &mut _new_frame,
                        );
                    }

                    let layout_props = view_state.borrow().layout_props.clone();

                    CURRENT_DRAG_PAINTING_ID.set(Some(id));

                    paint_bg(self, &view_style_props, size);
                    view.borrow_mut().paint(self);
                    paint_border(self, &layout_props, &view_style_props, size);
                    paint_outline(self, &view_style_props, size);

                    self.restore();

                    CURRENT_DRAG_PAINTING_ID.take();
                }
            }
        }

        if drag_set_to_none {
            self.window_state.dragging = None;
        }
        if alpha != 1. {
            self.pop_layer();
        }

        // Restore clip and z-index
        self.restore();
        if let Some(parent) = id.parent() {
            let transform = parent.world_transform();
            if let Some(transform) = transform {
                self.set_transform(transform);
            }
        }
    }

    /// Clip the drawing area to the given shape.
    pub fn clip(&mut self, shape: &impl Shape) {
        #[cfg(feature = "vello")]
        {
            use peniko::Mix;

            self.push_layer(Mix::Normal, 1.0, Affine::IDENTITY, shape);
            self.layer_count += 1;
            self.clip = Some(shape.bounding_box().to_rounded_rect(0.0));
        }

        #[cfg(not(feature = "vello"))]
        {
            let rect = if let Some(rect) = shape.as_rect() {
                rect.to_rounded_rect(0.0)
            } else if let Some(rect) = shape.as_rounded_rect() {
                rect
            } else {
                let rect = shape.bounding_box();
                rect.to_rounded_rect(0.0)
            };

            let rect = if let Some(existing) = self.clip {
                let rect = existing.rect().intersect(rect.rect());
                self.paint_state.renderer_mut().clip(&rect);
                rect.to_rounded_rect(0.0)
            } else {
                self.paint_state.renderer_mut().clip(&shape);
                rect
            };

            self.clip = Some(rect);
        }
    }

    /// Remove clipping so the entire window can be rendered to.
    pub fn clear_clip(&mut self) {
        self.clip = None;
        self.paint_state.renderer_mut().clear_clip();
    }

    pub(crate) fn set_z_index(&mut self, z_index: i32) {
        self.z_index = Some(z_index);
        self.paint_state.renderer_mut().set_z_index(z_index);
    }

    pub fn is_focused(&self, id: impl Into<VisualId>) -> bool {
        self.window_state.is_focused(id)
    }
}

// TODO: should this be private?
pub enum PaintState {
    /// The renderer is not yet initialized. This state is used to wait for the GPU resources to be acquired.
    PendingGpuResources {
        window: Arc<dyn Window>,
        rx: Receiver<Result<(GpuResources, wgpu::Surface<'static>), GpuResourceError>>,
        font_embolden: f32,
        /// This field holds an instance of `Renderer::Uninitialized` until the GPU resources are acquired,
        /// which will be returned in `PaintState::renderer` and `PaintState::renderer_mut`.
        /// All calls to renderer methods will be no-ops until the renderer is initialized.
        ///
        /// Previously, `PaintState::renderer` and `PaintState::renderer_mut` would panic if called when the renderer was uninitialized.
        /// However, this turned out to be hard to handle properly and led to panics, especially since the rest of the application code can't control when the renderer is initialized.
        renderer: crate::renderer::Renderer,
    },
    /// The renderer is initialized and ready to paint.
    Initialized { renderer: crate::renderer::Renderer },
}

impl PaintState {
    pub fn new_pending(
        window: Arc<dyn Window>,
        rx: Receiver<Result<(GpuResources, wgpu::Surface<'static>), GpuResourceError>>,
        scale: f64,
        size: Size,
        font_embolden: f32,
    ) -> Self {
        Self::PendingGpuResources {
            window,
            rx,
            font_embolden,
            renderer: Renderer::Uninitialized { scale, size },
        }
    }

    pub fn new(
        window: Arc<dyn Window>,
        surface: wgpu::Surface<'static>,
        gpu_resources: GpuResources,
        scale: f64,
        size: Size,
        font_embolden: f32,
    ) -> Self {
        let renderer = crate::renderer::Renderer::new(
            window.clone(),
            gpu_resources,
            surface,
            scale,
            size,
            font_embolden,
        );
        Self::Initialized { renderer }
    }

    pub(crate) fn renderer(&self) -> &crate::renderer::Renderer {
        match self {
            PaintState::PendingGpuResources { renderer, .. } => renderer,
            PaintState::Initialized { renderer } => renderer,
        }
    }

    pub(crate) fn renderer_mut(&mut self) -> &mut crate::renderer::Renderer {
        match self {
            PaintState::PendingGpuResources { renderer, .. } => renderer,
            PaintState::Initialized { renderer } => renderer,
        }
    }

    pub(crate) fn resize(&mut self, scale: f64, size: Size) {
        self.renderer_mut().resize(scale, size);
    }

    pub(crate) fn set_scale(&mut self, scale: f64) {
        self.renderer_mut().set_scale(scale);
    }
}

/// Layout context that caches computed layout rects for a view.
///
/// **DO NOT STORE THIS**. It will become invalid each time that layout is recomputed.
/// Instead, build a new one at each pass such as update, event, or paint when you want one.
///
/// This context provides cached access to various layout rectangles for a view,
/// avoiding redundant calculations when multiple layout queries are needed.
///
/// # Coordinate Spaces
///
/// Layout rectangles come in two coordinate spaces:
/// - **Parent-relative**: Position is relative to the parent view's origin
/// - **Local**: Position is relative to the view's own origin (always starts at 0,0)
///
/// # Why Local Coordinates?
///
/// Local coordinate methods (`raw_layout_rect_local`, `raw_content_rect_local`) are
/// typically the most useful because they represent the view's coordinate space where
/// (0, 0) is the top-left corner. This matches how events are transformed via
/// `window_event_to_view()`, which converts window coordinates to view-local space.
///
/// When you transform an event to view-local space, it accounts for:
/// - All ancestor layout positions and transforms
/// - All ancestor scroll offsets
/// - This view's own layout position, transform, and scroll offset
///
/// The resulting event coordinates are in the same space as `raw_layout_rect_local()`,
/// making hit testing straightforward: just check if the event point is within the
/// local rect's bounds.
///
/// # Parent-Relative Coordinates
///
/// Parent-relative methods (`raw_layout_rect`, `raw_view_rect`, `raw_content_rect`)
/// are useful when positioning or measuring views relative to their parent, such as
/// when implementing custom layout logic.
///
/// # Visual Transforms
///
/// These methods return layout information without visual transforms applied. Visual
/// transforms (rotation, scale, skew) are already accounted for in the accumulated
/// transform cache used by `window_event_to_view()` and `view_event_to_window()`.
///
/// For painting, you typically don't need to apply transforms manually since PaintCx
/// already handles the transform stack.
///
/// # Example
///
/// ```rust
/// # use floem::{prelude::*, context::LayoutCx};
/// let view = empty();
/// let id = view.id();
///
/// // a layout would need to be computed before the layout_cx has anything useful
/// let mut layout_cx = LayoutCx::new(id);
///
/// // Get the view's size in its own coordinate space
/// let local_rect = layout_cx.raw_layout_rect_local();
/// let width = local_rect.width();
/// let height = local_rect.height();
///
/// // Get the content area (excluding borders and padding) for positioning children
/// let content_area = layout_cx.raw_content_rect_local();
/// ```
pub struct LayoutCx {
    view_id: ViewId,
    // Lazy cached fields
    raw_layout: Option<Option<Layout>>,
    raw_layout_rect: Option<Rect>,
    raw_layout_rect_local: Option<Rect>,
    raw_content_rect: Option<Rect>,
    raw_content_rect_local: Option<Rect>,
}

impl LayoutCx {
    pub fn new(view_id: ViewId) -> Self {
        Self {
            view_id,
            raw_layout: None,
            raw_layout_rect: None,
            raw_layout_rect_local: None,
            raw_content_rect: None,
            raw_content_rect_local: None,
        }
    }

    /// Returns the Taffy layout for this view.
    ///
    /// The layout includes the view's size and position relative to its parent view.
    /// This is the layout information from Taffy without any adjustments for
    /// borders, padding, or other styling properties.
    pub fn layout(&mut self) -> Option<&Layout> {
        self.raw_layout
            .get_or_insert_with(|| self.view_id.layout())
            .as_ref()
    }

    /// Returns the layout rect in the view's local coordinate space.
    ///
    /// This is the correct rect to use for hit testing against events that have been
    /// transformed through `window_event_to_view()`. The rect always starts at (0, 0)
    /// and extends to (width, height), representing the view's natural coordinate space.
    ///
    /// When an event is transformed from window space to view-local space, it accounts
    /// for all ancestor positions, transforms, and scroll offsets, plus this view's own
    /// position, transform, and scroll offset. The resulting event coordinates are in
    /// the same space as this rect, making hit testing a simple bounds check.
    pub fn layout_rect_local(&mut self) -> Rect {
        *self
            .raw_layout_rect_local
            .get_or_insert_with(|| self.view_id.layout_rect_local())
    }

    /// Returns the content rect in the view's local coordinate space.
    ///
    /// The content rect excludes borders and padding, representing the area where
    /// child content should be positioned. Like `raw_layout_rect_local()`, this is
    /// in the view's local coordinate space starting at an offset that accounts for
    /// borders and padding.
    pub fn content_rect_local(&mut self) -> Rect {
        *self
            .raw_content_rect_local
            .get_or_insert_with(|| self.view_id.content_rect_local())
    }

    /// Returns the content rect relative to the parent view.
    ///
    /// The content rect excludes borders and padding. The position is relative to
    /// the parent view's origin, useful for parent-driven layout calculations.
    pub fn content_rect(&mut self) -> Rect {
        *self
            .raw_content_rect
            .get_or_insert_with(|| self.view_id.content_rect())
    }

    /// Returns the layout rect relative to the parent view.
    ///
    /// The position is relative to the parent view's origin, useful for measuring
    /// and positioning views within their parent's coordinate space.
    pub fn layout_rect(&mut self) -> Rect {
        *self
            .raw_layout_rect
            .get_or_insert_with(|| self.view_id.layout_rect())
    }
}

pub struct UpdateCx<'a> {
    pub window_state: &'a mut WindowState,
    pub target: VisualId,
}

impl Deref for PaintCx<'_> {
    type Target = crate::renderer::Renderer;

    fn deref(&self) -> &Self::Target {
        self.paint_state.renderer()
    }
}

impl DerefMut for PaintCx<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.paint_state.renderer_mut()
    }
}

fn animations_on_remove(id: ViewId, scope: Scope) -> u16 {
    let mut wait_for = 0;
    let state = id.state();
    let mut state = state.borrow_mut();
    state.num_waiting_animations = 0;
    let animations = &mut state.animations.stack;
    let mut request_style = false;
    for anim in animations {
        if anim.run_on_remove && !matches!(anim.repeat_mode, RepeatMode::LoopForever) {
            anim.reverse_mut();
            request_style = true;
            wait_for += 1;
            let trigger = anim.on_visual_complete;
            scope.create_updater(
                move || trigger.track(),
                move |_| {
                    id.transition_anim_complete();
                },
            );
        }
    }
    drop(state);
    if request_style {
        id.request_style();
    }

    id.children()
        .into_iter()
        .fold(wait_for, |acc, id| acc + animations_on_remove(id, scope))
}
fn stop_reset_remove_animations(id: ViewId) {
    let state = id.state();
    let mut state = state.borrow_mut();
    let animations = &mut state.animations.stack;
    let mut request_style = false;
    for anim in animations {
        if anim.run_on_remove
            && anim.state_kind() == AnimStateKind::PassInProgress
            && !matches!(anim.repeat_mode, RepeatMode::LoopForever)
        {
            anim.start_mut();
            request_style = true;
        }
    }
    drop(state);
    if request_style {
        id.request_style();
    }

    id.children()
        .into_iter()
        .for_each(stop_reset_remove_animations)
}

fn animations_on_create(id: ViewId) {
    let state = id.state();
    let mut state = state.borrow_mut();
    state.num_waiting_animations = 0;
    let animations = &mut state.animations.stack;
    let mut request_style = false;
    for anim in animations {
        if anim.run_on_create && !matches!(anim.repeat_mode, RepeatMode::LoopForever) {
            anim.start_mut();
            request_style = true;
        }
    }
    drop(state);
    if request_style {
        id.request_style();
    }

    id.children().into_iter().for_each(animations_on_create);
}
