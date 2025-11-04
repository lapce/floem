use std::{collections::VecDeque, time::Duration};

use floem::{
    IntoView, View,
    action::exec_after,
    event::{Event, EventPropagation},
    kurbo::{self, Vec2},
    ui_events::pointer::{PointerButtonEvent, PointerEvent, PointerUpdate},
};

const VELOCITY_HISTORY_SIZE: usize = 8;
/// The interval at which the view should update animations.
/// Set to roughly 60 FPS.
const UPDATE_INTERVAL: Duration = Duration::from_millis(16);

const ZOOM_ANIMATION_DURATION: Duration = Duration::from_millis(333);

/// Duration over which the velocity should decay to zero after a drag ends.
const DRAG_END_ANIMATION_DURATION: Duration = Duration::from_secs(1);

pub fn pan_zoom_view<V: IntoView + 'static>(
    view_transform: kurbo::Affine,
    child: V,
) -> PanZoomView {
    let id = floem::ViewId::new();

    let child = child.into_view();
    id.set_children([child]);

    PanZoomView {
        id,
        onpanzoom: None,
        view_transform,
        cursor_pos: None,
        drag_cursor_pos: None,
        last_drag_cursor_pos: None,
        dragging: false,
        drag_velocity: kurbo::Vec2::default(),
        last_update: std::time::Instant::now(),
        drag_end_time: None,
        drag_velocities: VecDeque::with_capacity(VELOCITY_HISTORY_SIZE),
        current_scale: 1.0,
        target_scale: 1.0,
        zoom_start_time: None,
    }
}

pub struct PanZoomView {
    id: floem::ViewId,
    /// Callback to be called when the view is panned/zoomed and therefore the requested viewport changed.
    onpanzoom: Option<Box<dyn Fn(kurbo::Affine)>>,
    /// The affine transformation (scale, rotation, translation) that represents the current viewport.
    /// In particular, this represents the viewport-to-world transformation.
    /// To transform a point from world space to viewport space, use `view_transform.inverse() * point`.
    view_transform: kurbo::Affine,
    /// Most recent cursor position in screen space.
    cursor_pos: Option<kurbo::Point>,
    /// Most recent cursor position in screen space while dragging.
    /// Updated on every pointer move event.
    drag_cursor_pos: Option<kurbo::Point>,
    /// Previous cursor position in screen space while dragging.
    /// Updated periodically to calculate drag velocity.
    last_drag_cursor_pos: Option<kurbo::Point>,
    /// Whether the view is currently being dragged.
    dragging: bool,
    /// The velocity of the drag in viewport space.
    drag_velocity: kurbo::Vec2,
    /// History of last drag velocities
    /// The drag velocity is calculated as a moving average of the instantaneous velocities.
    drag_velocities: VecDeque<kurbo::Vec2>,
    /// The time of the last update of the view (used for animation of drag velocity)
    last_update: std::time::Instant,
    /// Time when the last drag ended
    drag_end_time: Option<std::time::Instant>,
    /// The current scale of the view. Needed for the zoom animation, which interpolates between the current and target scale.
    current_scale: f64,
    /// The target scale of the view. The view will zoom in or out to reach this scale.
    target_scale: f64,
    /// The time when the zoom animation started. The zoom animation will finish at `zoom_start_time + ZOOM_ANIMATION_DURATION`.
    zoom_start_time: Option<std::time::Instant>,
}

impl PanZoomView {
    pub fn on_pan_zoom(mut self, onpanzoom: impl Fn(kurbo::Affine) + 'static) -> Self {
        self.onpanzoom = Some(Box::new(onpanzoom));
        self
    }
}

impl View for PanZoomView {
    fn id(&self) -> floem::ViewId {
        self.id
    }

    fn event_before_children(
        &mut self,
        _cx: &mut floem::context::EventCx,
        event: &Event,
    ) -> EventPropagation {
        match event {
            Event::Pointer(PointerEvent::Down(PointerButtonEvent { state, .. })) => {
                self.dragging = true;
                self.drag_cursor_pos = Some(state.logical_point());
                self.drag_velocities.clear();
                self.schedule_update();
            }
            Event::Pointer(PointerEvent::Move(PointerUpdate { current, .. })) => {
                self.cursor_pos = Some(current.logical_point());
                if !self.dragging {
                    return EventPropagation::Continue;
                }

                let current_cursor_pos = current.logical_point();
                let Some(previous_cursor_pos) = self.drag_cursor_pos else {
                    self.drag_cursor_pos = Some(current_cursor_pos);
                    return EventPropagation::Continue;
                };

                // Calculate and apply drag delta in viewport space
                let delta = self.screen_to_viewport(previous_cursor_pos)
                    - self.screen_to_viewport(current_cursor_pos);
                self.set_view_transform(kurbo::Affine::translate(delta) * self.view_transform);
                self.drag_cursor_pos = Some(current_cursor_pos);

                self.id().request_paint();

                return EventPropagation::Stop;
            }
            Event::Pointer(PointerEvent::Up(_)) => {
                self.dragging = false;
                self.drag_cursor_pos = None;

                let now = std::time::Instant::now();
                let dt = (now - self.last_update).as_secs_f64();
                if dt < 1. {
                    self.drag_velocity = self.drag_velocity();
                    self.drag_end_time = Some(now);
                    self.schedule_update();
                } else {
                    self.drag_velocity = kurbo::Vec2::ZERO;
                }
            }
            e @ Event::Pointer(PointerEvent::Scroll(_)) => {
                if let Some(delta) = e.pixel_scroll_delta_vec2() {
                    let scale = 1. + delta.y / 250.;

                    self.target_scale *= scale;
                    if self.zoom_start_time.is_none() {
                        self.zoom_start_time = Some(std::time::Instant::now());
                        self.schedule_update();
                    } else {
                        self.zoom_start_time = Some(std::time::Instant::now());
                    }
                }
            }
            _ => {}
        }
        EventPropagation::Continue
    }

    fn update(&mut self, _cx: &mut floem::context::UpdateCx, _state: Box<dyn std::any::Any>) {
        let update = self.measure_drag_velocity();

        let mut repaint = false;
        repaint |= self.run_zoom_animation();
        repaint |= self.run_drag_end_animation();

        if repaint {
            self.id().request_paint();
        }
        if update || repaint {
            self.schedule_update();
        }
    }
}

impl PanZoomView {
    /// Transforms a point from screen space to viewport space.
    /// The difference between screen space and viewport space is that screen space is unaware of rotation and scale.
    /// This is useful for handling pointer events.
    fn screen_to_viewport(&self, point: kurbo::Point) -> kurbo::Point {
        // To transform a point from screen space to viewport space, we need to apply rotation and scale of the view.
        // We can achieve this by setting the translation of the world-to-viewport matrix to zero.
        self.view_transform.with_translation(Vec2::ZERO) * point
    }

    fn schedule_update(&mut self) {
        let id = self.id();
        exec_after(UPDATE_INTERVAL, move |_| {
            id.update_state(Box::new(()));
        });
    }

    /// Calculate the moving average of the drag velocity history
    /// We use a weighted average to give more importance to recent velocities.
    fn drag_velocity(&self) -> kurbo::Vec2 {
        let mut drag_velocity = kurbo::Vec2::ZERO;
        let mut total_weight = 0.0;
        for (i, velocity) in self.drag_velocities.iter().enumerate() {
            let weight = (i + 1) as f64;
            drag_velocity += *velocity * weight;
            total_weight += weight;
        }
        drag_velocity / total_weight
    }

    fn measure_drag_velocity(&mut self) -> bool {
        if !self.dragging {
            return false;
        }

        let Some(prev_cursor_pos) = self.last_drag_cursor_pos else {
            self.last_drag_cursor_pos = self.drag_cursor_pos;
            return false;
        };
        let Some(cursor_pos) = self.drag_cursor_pos else {
            return false;
        };
        self.last_drag_cursor_pos = self.drag_cursor_pos;

        let delta = self.screen_to_viewport(prev_cursor_pos) - self.screen_to_viewport(cursor_pos);

        // Calculate drag velocity
        let now = std::time::Instant::now();
        let mut dt = (now - self.last_update).as_secs_f64();
        self.last_update = now;

        // Clamp dt to avoid very small values
        let min_dt = 0.01;
        if dt < min_dt {
            dt = min_dt;
        }

        // Add the new velocity to the history
        let instant_velocity = delta / dt;
        if self.drag_velocities.len() > VELOCITY_HISTORY_SIZE {
            self.drag_velocities.pop_front();
        }
        self.drag_velocities.push_back(instant_velocity);

        true
    }

    fn run_drag_end_animation(&mut self) -> bool {
        if self.dragging || self.drag_velocity == kurbo::Vec2::ZERO {
            return false;
        }

        let now = std::time::Instant::now();
        let dt = (now - self.last_update).as_secs_f64();
        self.last_update = now;

        let delta = self.drag_velocity * dt;
        self.set_view_transform(kurbo::Affine::translate(delta) * self.view_transform);

        let start_time = self.drag_end_time.unwrap_or(now);
        let elapsed = (now - start_time).as_secs_f64();

        // Apply exponential decay to the velocity
        let decay_factor = (1.0 - elapsed / DRAG_END_ANIMATION_DURATION.as_secs_f64()).max(0.0);
        self.drag_velocity *= decay_factor;

        if self.drag_velocity.length() < 0.1 {
            self.drag_velocity = kurbo::Vec2::ZERO;
        }

        true
    }

    fn run_zoom_animation(&mut self) -> bool {
        let Some(zoom_start_time) = self.zoom_start_time else {
            return false;
        };

        let now = std::time::Instant::now();
        let elapsed = (now - zoom_start_time).as_secs_f64();
        let progress = (elapsed / ZOOM_ANIMATION_DURATION.as_secs_f64()).min(1.0);

        // Linear interpolation to calculate current scale
        let current_scale_factor =
            self.current_scale + (self.target_scale - self.current_scale) * progress;
        let scale_factor = current_scale_factor / self.current_scale;

        // Zoom around the cursor position
        let cursor_pos = self.cursor_pos.unwrap_or_default();
        let cursor_pos_viewport = self.screen_to_viewport(cursor_pos);
        let world_translation = self.view_transform * kurbo::Point::ZERO;
        let view_transform_without_translation = self.view_transform.with_translation(Vec2::ZERO);

        let scale_around_cursor = kurbo::Affine::translate(cursor_pos_viewport.to_vec2())
            * kurbo::Affine::scale(scale_factor)
            * kurbo::Affine::translate(-cursor_pos_viewport.to_vec2());

        self.set_view_transform(
            kurbo::Affine::translate(world_translation.to_vec2())
                * scale_around_cursor
                * view_transform_without_translation,
        );

        self.current_scale = current_scale_factor;

        if progress >= 1.0 {
            // Animation completed
            self.zoom_start_time = None;
        }

        true
    }

    /// Update the view transform matrix and trigger the callback
    fn set_view_transform(&mut self, view_transform: kurbo::Affine) {
        self.view_transform = view_transform;
        if let Some(onpanzoom) = &self.onpanzoom {
            onpanzoom(view_transform);
        }
    }
}
