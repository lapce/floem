use std::{
    any::Any,
    rc::Rc,
    time::{Duration, Instant},
};

use peniko::kurbo::Point;
use ui_events::pointer::{
    PointerButton, PointerButtonEvent, PointerInfo, PointerState, PointerUpdate,
};

use crate::{
    BoxTree, ElementId,
    action::{TimerToken, add_update_message},
    animate::easing::{Easing, Linear},
    event::{
        DragCancelEvent, DragEndEvent, DragEnterEvent, DragLeaveEvent, DragMoveEvent,
        DragSourceEvent, DragStartEvent, DragTargetEvent,
    },
    message::UpdateMessage,
    unit::Pct,
};

/// Default duration for drag preview return animation in milliseconds.
const DEFAULT_ANIMATION_DURATION_MS: f64 = 300.0;

/// Configuration for draggable behavior and animations.
#[derive(Clone, Debug)]
pub struct DragConfig {
    /// Minimum distance (in logical pixels) pointer must move before drag starts.
    pub threshold: f64,
    /// Duration of the return animation when drag is released.
    pub animation_duration: Duration,
    /// Easing function for the return animation.
    pub easing: Rc<dyn Easing>,
    /// Optional custom data to pass through drag events
    pub custom_data: Option<Rc<dyn Any>>,
    /// If true, track drop targets and emit DragTargetEvents (Enter/Move/Leave/Drop).
    /// If false, only emit DragSourceEvents (Start/Move/Drop/Cancel) - simpler state machine.
    pub track_targets: bool,
}

impl DragConfig {
    /// Create a new drag configuration with custom parameters.
    pub fn new(
        threshold: f64,
        animation_duration: Duration,
        easing: impl Easing + 'static,
    ) -> Self {
        Self {
            threshold,
            animation_duration,
            easing: Rc::new(easing),
            custom_data: None,
            track_targets: true,
        }
    }

    /// Set the drag threshold distance.
    pub fn with_threshold(mut self, threshold: f64) -> Self {
        self.threshold = threshold;
        self
    }

    /// Set the animation duration.
    pub fn with_animation_duration(mut self, duration: Duration) -> Self {
        self.animation_duration = duration;
        self
    }

    /// Set the easing function.
    pub fn with_easing(mut self, easing: impl Easing + 'static) -> Self {
        self.easing = Rc::new(easing);
        self
    }

    /// Attach custom data to pass through drag events.
    pub fn with_custom_data(mut self, data: impl Any + 'static) -> Self {
        self.custom_data = Some(Rc::new(data));
        self
    }

    /// Set whether to track drop targets.
    ///
    /// When `true` (default), emits DragTargetEvents (Enter/Move/Leave/Drop) to elements
    /// under the dragged element, and DragSourceEvents (Enter/Leave) when hovering targets.
    ///
    /// When `false`, only emits DragSourceEvents (Start/Move/Drop/Cancel) - a simpler
    /// state machine useful for pan/zoom, sliders, and custom drag interactions.
    pub fn with_track_targets(mut self, track_targets: bool) -> Self {
        self.track_targets = track_targets;
        self
    }
}

impl Default for DragConfig {
    fn default() -> Self {
        Self {
            threshold: 3.0,
            animation_duration: Duration::from_millis(DEFAULT_ANIMATION_DURATION_MS as u64),
            easing: Rc::new(Linear),
            custom_data: None,
            track_targets: true,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct DraggingPreview {
    pub element_id: ElementId,
    pub drag_point_pct: (Pct, Pct),
}

/// Tracks drag state for elements.
///
/// # Drag Lifecycle
///
/// 1. **Pointer Capture**: An element must first gain pointer capture via
///    `cx.set_pointer_capture()` in response to a pointer down event.
///
/// 2. **Request Drag**: When the element receives `PointerCaptureEvent::Gained`,
///    it can call `cx.request_drag()` to begin tracking a potential drag.
///
/// 3. **Threshold Tracking**: The drag tracker monitors pointer movement. Once
///    the pointer moves beyond the threshold distance, the drag activates.
///
/// 4. **Drag Start**: When threshold is exceeded, `DragSourceEvent::Start` is dispatched
///    to the dragging element.
///
/// 5. **Drag Movement**: While dragging, `DragSourceEvent::Move` is sent to the dragging
///    element, and `DragTargetEvent::Enter`/`Leave` are sent to drop targets.
///
/// 6. **Drag End**: On pointer up, either `DragSourceEvent::Drop` and `DragTargetEvent::Drop`
///    (if over a valid drop target) or `DragSourceEvent::Cancel` (if not) is dispatched.
///
/// # Example
///
/// ```rust
/// impl MyView {
///     fn event(&mut self, cx: &mut EventCx) -> EventPropagation {
///         match &cx.event {
///             Event::Pointer(PointerEvent::Down(pe)) => {
///                 // Request pointer capture first
///                 if let Some(pointer_id) = pe.pointer.pointer_id {
///                     cx.set_pointer_capture(pointer_id);
///                 }
///             }
///             Event::PointerCapture(PointerCaptureEvent::Gained(_)) => {
///                 // Now request drag
///                 cx.request_drag();
///             }
///             Event::DragSource(DragSourceEvent::Start(_)) => {
///                 println!("Drag started!");
///             }
///             Event::DragSource(DragSourceEvent::Drop(_)) => {
///                 println!("Dropped!");
///             }
///             Event::DragTarget(DragTargetEvent::Drop(e)) => {
///                 // Accept the drop
///                 cx.prevent_default();
///                 println!("Drop accepted from element {:?}", e.other_element);
///             }
///             _ => {}
///         }
///         EventPropagation::Continue
///     }
/// }
/// ```
pub(crate) struct DragTracker {
    /// Pending drag request (element has requested drag, tracking threshold).
    /// Only elements with pointer capture can request drag.
    pub pending_drag: Option<PendingDragState>,

    /// The currently active drag (after threshold exceeded).
    /// Only one drag can be active at a time per window.
    pub active_drag: Option<ActiveDragState>,

    /// Hover state for tracking drag enter/leave events on drop targets.
    pub hover_state: understory_event_state::hover::HoverState<ElementId>,
}

/// State of a pending drag request (before threshold exceeded).
#[derive(Clone, Debug)]
pub(crate) struct PendingDragState {
    /// The element that requested drag
    pub element_id: ElementId,
    /// Pointer state when drag was requested
    pub start_state: PointerState,
    /// Which button initiated the drag
    pub button: Option<PointerButton>,
    /// Minimum distance (in logical pixels) pointer must move before drag starts.
    pub threshold: f64,
    /// When true, the view will be moved to be under the cursor automatically by floem.
    pub use_default_preview: bool,
    /// Duration of return animation
    pub animation_duration: Duration,
    /// Easing function for return animation
    pub easing: Rc<dyn Easing>,
    /// Custom data to pass through drag events
    pub custom_data: Option<Rc<dyn Any>>,
    /// Whether to track drop targets and emit target events
    pub track_targets: bool,
}

/// State of an active drag (threshold exceeded).
#[derive(Clone, Debug)]
pub(crate) struct ActiveDragState {
    /// The element being dragged
    pub element_id: ElementId,
    /// Current pointer state
    pub current_state: PointerState,
    /// Pointer state when drag started
    pub start_state: PointerState,
    /// Which button is being held
    pub button: Option<PointerButton>,
    /// When drag was released (for animation)
    pub released_at: Option<Instant>,
    /// Where drag was released (for animation)
    pub release_location: Option<Point>,
    /// Duration of the return animation
    pub animation_duration: Duration,
    /// Easing function for the return animation
    pub easing: Rc<dyn Easing>,
    /// Timer token for scheduled animation frames
    pub animation_timer: Option<TimerToken>,
    /// Natural layout position before drag transform (for animation)
    pub natural_position: Option<Point>,
    /// Last translation we applied via set_world_translation (for detecting layout changes)
    pub last_applied_translation: Option<Point>,

    pub dragging_preview: Option<DraggingPreview>,
    /// Custom data to pass through drag events
    pub custom_data: Option<Rc<dyn Any>>,
    /// Whether to track drop targets and emit target events
    pub track_targets: bool,
}
impl ActiveDragState {
    /// Returns true if the animation has completed.
    pub fn is_animation_complete(&self) -> bool {
        let Some(released_at) = self.released_at else {
            return false;
        };

        let elapsed = released_at.elapsed().as_secs_f64();
        let duration_secs = self.animation_duration.as_secs_f64();
        let time = elapsed / duration_secs;

        self.easing.finished(time)
    }

    /// Update natural position by detecting layout changes.
    ///
    /// Compares the current transform translation to the last applied translation.
    /// If they differ, layout has changed and natural position is updated.
    ///
    /// # Arguments
    /// * `current_transform` - The current world transform from compute_world_transform()
    ///
    /// # Returns
    /// The natural position to use for positioning calculations
    pub fn update_and_get_natural_position(
        &mut self,
        current_transform: peniko::kurbo::Affine,
    ) -> Point {
        // Extract current translation from transform
        let current_translation = Point::new(
            current_transform.translation().x,
            current_transform.translation().y,
        );

        // Detect if layout changed by comparing to last applied translation
        let layout_changed = self
            .last_applied_translation
            .map(|last| {
                // If current translation doesn't match what we applied, layout changed
                (current_translation.x - last.x).abs() > 0.01
                    || (current_translation.y - last.y).abs() > 0.01
            })
            .unwrap_or(true); // First time, always update

        // Update natural position if layout changed
        if layout_changed {
            self.natural_position = Some(current_translation);
        }

        // Return natural position to use
        self.natural_position.unwrap_or(current_translation)
    }

    /// Record the translation that was just applied.
    ///
    /// This is used for detecting layout changes on the next frame.
    pub fn record_applied_translation(&mut self, translation: Point) {
        self.last_applied_translation = Some(translation);
    }

    /// Returns true if an animation frame should be scheduled.
    pub fn should_schedule_animation_frame(&self) -> bool {
        self.released_at.is_some() && !self.is_animation_complete()
    }

    /// Calculate the position for the drag preview.
    ///
    /// # Arguments
    /// * `natural_position` - The natural layout position (top-left corner) of the element
    /// * `drag_point_offset` - The offset from top-left to where the user grabbed the element
    ///
    /// # Returns
    /// The position to set via `set_world_translation()`
    pub fn calculate_position(&self, natural_position: Point, drag_point_offset: Point) -> Point {
        if let Some(released_at) = self.released_at
            && !self.is_animation_complete()
        {
            // Animating: interpolate element top-left from release position to natural position
            let release_top_left = Point::new(
                self.release_location.unwrap().x - drag_point_offset.x,
                self.release_location.unwrap().y - drag_point_offset.y,
            );

            // Calculate animation progress
            let elapsed = released_at.elapsed().as_secs_f64();
            let duration_secs = self.animation_duration.as_secs_f64();
            let time = elapsed / duration_secs;
            let progress = self.easing.eval(time); // 0.0 at start, 1.0 at end

            // Interpolate from release position to natural position
            Point::new(
                release_top_left.x + (natural_position.x - release_top_left.x) * progress,
                release_top_left.y + (natural_position.y - release_top_left.y) * progress,
            )
        } else if self.released_at.is_some() {
            // Animation complete: position at natural layout position (top-left corner)
            natural_position
        } else {
            // Normal drag: position at cursor (subtract drag_point_offset to position top-left)
            let cursor_point = self.current_state.logical_point();
            Point::new(
                cursor_point.x - drag_point_offset.x,
                cursor_point.y - drag_point_offset.y,
            )
        }
    }
}

/// Events that need to be dispatched, with target element IDs.
pub(crate) enum DragEventDispatch {
    Source(ElementId, DragSourceEvent),
    Target(ElementId, DragTargetEvent),
}

impl DragTracker {
    pub fn new() -> Self {
        Self {
            pending_drag: None,
            active_drag: None,
            hover_state: understory_event_state::hover::HoverState::new(),
        }
    }

    /// Returns true if a drag is currently active (threshold exceeded).
    pub fn is_dragging(&self) -> bool {
        self.active_drag.is_some()
    }

    /// Request a drag for an element that has pointer capture.
    pub fn request_drag(
        &mut self,
        element_id: ElementId,
        pointer_state: PointerState,
        button: Option<PointerButton>,
        config: DragConfig,
        use_default_preview: bool,
    ) -> bool {
        // Can't request drag if one is already pending or active (but if the active is animating then we can interrupt it with a new drag)
        if self.pending_drag.is_some()
            || self
                .active_drag
                .as_ref()
                .is_some_and(|a| a.released_at.is_none())
        {
            return false;
        }

        self.pending_drag = Some(PendingDragState {
            element_id,
            start_state: pointer_state,
            button,
            threshold: config.threshold,
            use_default_preview,
            animation_duration: config.animation_duration,
            easing: config.easing,
            custom_data: config.custom_data,
            track_targets: config.track_targets,
        });

        true
    }

    /// Check if the pending drag has exceeded the threshold.
    pub fn check_threshold(
        &mut self,
        move_event: &PointerUpdate,
        box_tree: &BoxTree,
    ) -> Option<DragEventDispatch> {
        let pending_state = self.pending_drag.as_ref()?;
        let start_pos = pending_state.start_state.logical_point();
        let current_pos = move_event.current.logical_point();
        let offset = current_pos - start_pos;
        let distance = offset.length();

        if distance >= pending_state.threshold {
            let pending = self.pending_drag.take().unwrap();

            let dragging_preview = if pending.use_default_preview {
                // Get the element's local bounds for size
                let local_bounds = box_tree
                    .local_bounds(pending.element_id.0)
                    .unwrap_or_default();

                // Calculate percentage based on custom_pct or initial click position
                // Get world bounds at drag start to know where user clicked
                let start_world_bounds = match box_tree
                    .world_bounds(pending.element_id.0)
                    .map_err(|e| e.value().expect("not stale"))
                {
                    Ok(t) => t,
                    Err(e) => e,
                };

                let click_offset_x = start_pos.x - start_world_bounds.x0;
                let click_offset_y = start_pos.y - start_world_bounds.y0;

                // Convert to percentage
                let pct_x = Pct((click_offset_x / local_bounds.width()) * 100.0);
                let pct_y = Pct((click_offset_y / local_bounds.height()) * 100.0);

                Some(DraggingPreview {
                    element_id: pending.element_id,
                    drag_point_pct: (pct_x, pct_y),
                })
            } else {
                None
            };

            self.active_drag = Some(ActiveDragState {
                element_id: pending.element_id,
                current_state: move_event.current.clone(),
                start_state: pending.start_state.clone(),
                button: pending.button,
                released_at: None,
                release_location: None,
                animation_duration: pending.animation_duration,
                easing: pending.easing,
                animation_timer: None,
                natural_position: None,
                last_applied_translation: None,
                dragging_preview,
                custom_data: pending.custom_data,
                track_targets: pending.track_targets,
            });

            Some(DragEventDispatch::Source(
                pending.element_id,
                DragSourceEvent::Start(DragStartEvent {
                    start_state: pending.start_state,
                    current_state: move_event.current.clone(),
                    button: pending.button,
                    pointer: move_event.pointer,
                    custom_data: self.active_drag.as_ref().unwrap().custom_data.clone(),
                }),
            ))
        } else {
            None
        }
    }

    pub fn on_pointer_move(
        &mut self,
        move_event: &PointerUpdate,
        hover_path: &[ElementId],
    ) -> Vec<DragEventDispatch> {
        let state = match self.active_drag.as_mut() {
            Some(s) => s,
            None => return Vec::new(),
        };

        if state.released_at.is_some() {
            return vec![];
        }

        state.current_state = move_event.current.clone();

        let mut events = Vec::new();

        // Emit Move event to the dragged element
        events.push(DragEventDispatch::Source(
            state.element_id,
            DragSourceEvent::Move(DragMoveEvent {
                other_element: None,
                start_state: state.start_state.clone(),
                current_state: move_event.current.clone(),
                button: state.button,
                pointer: move_event.pointer,
                custom_data: state.custom_data.clone(),
            }),
        ));

        let dragged_id = state.element_id;
        let track_targets = state.track_targets;

        // Only track drop targets if enabled
        if track_targets {
            // Emit Move event to the current drop target (if hovering over one)
            if let Some(current_target) = hover_path.last().copied() {
                if current_target != dragged_id {
                    events.push(DragEventDispatch::Target(
                        current_target,
                        DragTargetEvent::Move(DragMoveEvent {
                            other_element: Some(dragged_id),
                            start_state: state.start_state.clone(),
                            current_state: move_event.current.clone(),
                            button: state.button,
                            pointer: move_event.pointer,
                            custom_data: state.custom_data.clone(),
                        }),
                    ));
                }
            }

            // Update hover state and generate Enter/Leave events
            let hover_events = self.hover_state.update_path(hover_path);
            for hover_event in hover_events {
                match hover_event {
                    understory_event_state::hover::HoverEvent::Enter(drop_target) => {
                        if drop_target != dragged_id {
                            // Send Enter to the dragged element (telling it which target was entered)
                            events.push(DragEventDispatch::Source(
                                dragged_id,
                                DragSourceEvent::Enter(DragEnterEvent {
                                    other_element: drop_target,
                                    start_state: state.start_state.clone(),
                                    current_state: move_event.current.clone(),
                                    button: state.button,
                                    pointer: move_event.pointer,
                                    custom_data: state.custom_data.clone(),
                                }),
                            ));

                            // Send Enter to the drop target (telling it which element entered)
                            events.push(DragEventDispatch::Target(
                                drop_target,
                                DragTargetEvent::Enter(DragEnterEvent {
                                    other_element: dragged_id,
                                    start_state: state.start_state.clone(),
                                    current_state: move_event.current.clone(),
                                    button: state.button,
                                    pointer: move_event.pointer,
                                    custom_data: state.custom_data.clone(),
                                }),
                            ));
                        }
                    }
                    understory_event_state::hover::HoverEvent::Leave(drop_target) => {
                        if drop_target != dragged_id {
                            // Send Leave to the dragged element (telling it which target was left)
                            events.push(DragEventDispatch::Source(
                                dragged_id,
                                DragSourceEvent::Leave(DragLeaveEvent {
                                    other_element: drop_target,
                                    start_state: state.start_state.clone(),
                                    current_state: move_event.current.clone(),
                                    button: state.button,
                                    pointer: move_event.pointer,
                                    custom_data: state.custom_data.clone(),
                                }),
                            ));

                            // Send Leave to the drop target (telling it which element left)
                            events.push(DragEventDispatch::Target(
                                drop_target,
                                DragTargetEvent::Leave(DragLeaveEvent {
                                    other_element: dragged_id,
                                    start_state: state.start_state.clone(),
                                    current_state: move_event.current.clone(),
                                    button: state.button,
                                    pointer: move_event.pointer,
                                    custom_data: state.custom_data.clone(),
                                }),
                            ));
                        }
                    }
                }
            }
        }

        events
    }

    pub fn on_pointer_up(&mut self, button_event: &PointerButtonEvent) -> Vec<DragEventDispatch> {
        // Clear any pending drag
        if self.pending_drag.is_some() {
            self.pending_drag = None;
            return Vec::new();
        }

        let mut state = match self.active_drag.take() {
            Some(s) => s,
            None => return Vec::new(),
        };

        let mut events = Vec::new();
        let track_targets = state.track_targets;

        if track_targets {
            let drop_target = self.hover_state.current_path().last().copied();

            // Clear hover state and generate Leave events
            let hover_events = self.hover_state.clear();
            for hover_event in hover_events {
                if let understory_event_state::hover::HoverEvent::Leave(target) = hover_event {
                    if target != state.element_id {
                        // Send Leave to the dragged element
                        events.push(DragEventDispatch::Source(
                            state.element_id,
                            DragSourceEvent::Leave(DragLeaveEvent {
                                other_element: target,
                                start_state: state.start_state.clone(),
                                current_state: button_event.state.clone(),
                                button: state.button,
                                pointer: button_event.pointer,
                                custom_data: state.custom_data.clone(),
                            }),
                        ));

                        // Send Leave to the drop target
                        events.push(DragEventDispatch::Target(
                            target,
                            DragTargetEvent::Leave(DragLeaveEvent {
                                other_element: state.element_id,
                                start_state: state.start_state.clone(),
                                current_state: button_event.state.clone(),
                                button: state.button,
                                pointer: button_event.pointer,
                                custom_data: state.custom_data.clone(),
                            }),
                        ));
                    }
                }
            }

            if let Some(drop_target_id) = drop_target {
                if drop_target_id != state.element_id {
                    // Send End to the dragged element (with the target in other_element)
                    events.push(DragEventDispatch::Source(
                        state.element_id,
                        DragSourceEvent::End(DragEndEvent {
                            other_element: Some(drop_target_id),
                            start_state: state.start_state.clone(),
                            current_state: button_event.state.clone(),
                            button: state.button,
                            pointer: button_event.pointer,
                            custom_data: state.custom_data.clone(),
                        }),
                    ));

                    // Send Drop to the drop target (with the dragged element in other_element)
                    events.push(DragEventDispatch::Target(
                        drop_target_id,
                        DragTargetEvent::Drop(DragEndEvent {
                            other_element: Some(state.element_id),
                            start_state: state.start_state.clone(),
                            current_state: button_event.state.clone(),
                            button: state.button,
                            pointer: button_event.pointer,
                            custom_data: state.custom_data.clone(),
                        }),
                    ));
                } else {
                    // Dropped on self - just send End to source with no target
                    events.push(DragEventDispatch::Source(
                        state.element_id,
                        DragSourceEvent::End(DragEndEvent {
                            other_element: None,
                            start_state: state.start_state.clone(),
                            current_state: button_event.state.clone(),
                            button: state.button,
                            pointer: button_event.pointer,
                            custom_data: state.custom_data.clone(),
                        }),
                    ));
                }
            } else {
                // No drop target - send End (no target) to the dragged element
                events.push(DragEventDispatch::Source(
                    state.element_id,
                    DragSourceEvent::End(DragEndEvent {
                        other_element: None,
                        start_state: state.start_state.clone(),
                        current_state: button_event.state.clone(),
                        button: state.button,
                        pointer: button_event.pointer,
                        custom_data: state.custom_data.clone(),
                    }),
                ));
            }
        } else {
            // track_targets is false - just send End to source
            events.push(DragEventDispatch::Source(
                state.element_id,
                DragSourceEvent::End(DragEndEvent {
                    other_element: None,
                    start_state: state.start_state.clone(),
                    current_state: button_event.state.clone(),
                    button: state.button,
                    pointer: button_event.pointer,
                    custom_data: state.custom_data.clone(),
                }),
            ));
        }

        // If there's a dragging preview, keep the drag state alive for animation
        if state.dragging_preview.is_some() {
            state.released_at = Some(Instant::now());
            state.release_location = Some(button_event.state.logical_point());
            self.active_drag = Some(state);
            // Trigger the first animation frame
            add_update_message(UpdateMessage::RequestBoxTreeCommit);
        }

        events
    }

    /// Handle pointer cancel to abort the drag.
    pub fn on_pointer_cancel(&mut self, info: PointerInfo) -> Vec<DragEventDispatch> {
        // Clear any pending drag
        if self.pending_drag.is_some() {
            self.pending_drag = None;
            self.hover_state.clear();
            return Vec::new();
        }

        let mut state = match self.active_drag.take() {
            Some(s) => s,
            None => return Vec::new(),
        };

        let mut events = Vec::new();
        let track_targets = state.track_targets;

        // Only track drop targets if enabled
        if track_targets {
            // Clear hover state and generate Leave events
            let hover_events = self.hover_state.clear();
            for hover_event in hover_events {
                if let understory_event_state::hover::HoverEvent::Leave(drop_target) = hover_event {
                    if drop_target != state.element_id {
                        // Send Leave to the dragged element
                        events.push(DragEventDispatch::Source(
                            state.element_id,
                            DragSourceEvent::Leave(DragLeaveEvent {
                                other_element: drop_target,
                                start_state: state.start_state.clone(),
                                current_state: state.current_state.clone(),
                                button: state.button,
                                pointer: info,
                                custom_data: state.custom_data.clone(),
                            }),
                        ));

                        // Send Leave to the drop target
                        events.push(DragEventDispatch::Target(
                            drop_target,
                            DragTargetEvent::Leave(DragLeaveEvent {
                                other_element: state.element_id,
                                start_state: state.start_state.clone(),
                                current_state: state.current_state.clone(),
                                button: state.button,
                                pointer: info,
                                custom_data: state.custom_data.clone(),
                            }),
                        ));
                    }
                }
            }
        }

        // Cancel event to the dragged element
        events.push(DragEventDispatch::Source(
            state.element_id,
            DragSourceEvent::Cancel(DragCancelEvent {
                start_state: state.start_state.clone(),
                current_state: state.current_state.clone(),
                button: state.button,
                pointer: info,
                custom_data: state.custom_data.clone(),
            }),
        ));

        // If there's a dragging preview, keep the drag state alive for animation
        if state.dragging_preview.is_some() {
            state.released_at = Some(Instant::now());
            state.release_location = Some(state.current_state.logical_point());
            self.active_drag = Some(state);
            // Trigger the first animation frame
            add_update_message(UpdateMessage::RequestBoxTreeCommit);
        }

        events
    }
}
