use floem_winit::event::TouchPhase;

#[derive(Debug, Clone)]
pub struct TouchpadMagnifyEvent {
    pub delta: f64,
    pub phase: TouchPhase,
}
