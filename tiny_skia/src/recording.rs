use std::sync::Arc;

use floem_renderer::tiny_skia::{Path, Pixmap};
use peniko::{
    BlendMode, Brush,
    kurbo::{Affine, Rect, Stroke},
};

use crate::ClipPath;

pub(crate) struct Recording {
    root: RecordedLayer,
    stack: Vec<RecordedLayer>,
}

impl Recording {
    pub(crate) fn new() -> Self {
        Self {
            root: RecordedLayer::root(),
            stack: Vec::new(),
        }
    }

    pub(crate) fn clear(&mut self) {
        self.root = RecordedLayer::root();
        self.stack.clear();
    }

    pub(crate) fn push_clip(&mut self, clip: ClipPath) {
        self.current_layer_mut()
            .commands
            .push(RecordedCommand::PushClip(clip));
    }

    pub(crate) fn pop_clip(&mut self) {
        self.current_layer_mut()
            .commands
            .push(RecordedCommand::PopClip);
    }

    pub(crate) fn fill_rect(
        &mut self,
        rect: Rect,
        brush: Brush,
        transform: Affine,
        blur_radius: f64,
    ) {
        self.current_layer_mut()
            .commands
            .push(RecordedCommand::FillRect {
                rect,
                brush,
                transform,
                blur_radius,
            });
    }

    pub(crate) fn fill_path(
        &mut self,
        path: Path,
        bounds: Rect,
        brush: Brush,
        transform: Affine,
        blur_radius: f64,
    ) {
        self.current_layer_mut()
            .commands
            .push(RecordedCommand::FillPath {
                path,
                bounds,
                brush,
                transform,
                blur_radius,
            });
    }

    pub(crate) fn stroke_path(
        &mut self,
        path: Path,
        bounds: Rect,
        brush: Brush,
        stroke: Stroke,
        transform: Affine,
    ) {
        self.current_layer_mut()
            .commands
            .push(RecordedCommand::StrokePath {
                path,
                bounds,
                brush,
                stroke,
                transform,
            });
    }

    pub(crate) fn draw_pixmap_direct(
        &mut self,
        pixmap: Arc<Pixmap>,
        x: f32,
        y: f32,
        transform: Affine,
    ) {
        self.current_layer_mut()
            .commands
            .push(RecordedCommand::DrawPixmapDirect {
                pixmap,
                x,
                y,
                transform,
            });
    }

    pub(crate) fn draw_pixmap_rect(&mut self, pixmap: Arc<Pixmap>, rect: Rect, transform: Affine) {
        self.current_layer_mut()
            .commands
            .push(RecordedCommand::DrawPixmapRect {
                pixmap,
                rect,
                transform,
            });
    }

    pub(crate) fn push_layer(&mut self, blend_mode: BlendMode, alpha: f32, clip: ClipPath) {
        self.stack
            .push(RecordedLayer::new(Some(clip), blend_mode, alpha));
    }

    pub(crate) fn pop_layer(&mut self) {
        let Some(layer) = self.stack.pop() else {
            return;
        };
        self.current_layer_mut()
            .commands
            .push(RecordedCommand::Layer(Box::new(layer)));
    }

    pub(crate) fn root(&self) -> &RecordedLayer {
        &self.root
    }

    fn current_layer_mut(&mut self) -> &mut RecordedLayer {
        self.stack.last_mut().unwrap_or(&mut self.root)
    }
}

pub(crate) struct RecordedLayer {
    pub(crate) clip: Option<ClipPath>,
    pub(crate) blend_mode: BlendMode,
    pub(crate) alpha: f32,
    pub(crate) commands: Vec<RecordedCommand>,
}

impl RecordedLayer {
    fn root() -> Self {
        Self {
            clip: None,
            blend_mode: peniko::Mix::Normal.into(),
            alpha: 1.0,
            commands: Vec::new(),
        }
    }

    fn new(clip: Option<ClipPath>, blend_mode: BlendMode, alpha: f32) -> Self {
        Self {
            clip,
            blend_mode,
            alpha,
            commands: Vec::new(),
        }
    }
}

pub(crate) enum RecordedCommand {
    PushClip(ClipPath),
    PopClip,
    FillRect {
        rect: Rect,
        brush: Brush,
        transform: Affine,
        blur_radius: f64,
    },
    FillPath {
        path: Path,
        bounds: Rect,
        brush: Brush,
        transform: Affine,
        blur_radius: f64,
    },
    StrokePath {
        path: Path,
        bounds: Rect,
        brush: Brush,
        stroke: Stroke,
        transform: Affine,
    },
    DrawPixmapDirect {
        pixmap: Arc<Pixmap>,
        x: f32,
        y: f32,
        transform: Affine,
    },
    DrawPixmapRect {
        pixmap: Arc<Pixmap>,
        rect: Rect,
        transform: Affine,
    },
    Layer(Box<RecordedLayer>),
}
