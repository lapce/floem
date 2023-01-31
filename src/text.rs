use vello::peniko::{Brush, Color};

#[derive(Clone, PartialEq, Debug)]
pub struct ParleyBrush(pub Brush);

impl Default for ParleyBrush {
    fn default() -> ParleyBrush {
        ParleyBrush(Brush::Solid(Color::rgb8(0, 0, 0)))
    }
}

impl parley::style::Brush for ParleyBrush {}
