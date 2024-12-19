use winit::event::TouchPhase;

#[derive(Debug, Clone)]
pub struct PinchGestureEvent {
    pub delta: f64,
    pub phase: TouchPhase,
}
