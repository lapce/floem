#[derive(Copy, Clone)]
#[allow(dead_code)]
pub enum PrimType {
    /// Filled circle.
    Circle,

    /// Stroked arc.
    Arc,

    /// Rounded corner rectangle.
    Rect,

    /// Stroked rounded rectangle.
    RectStroke,

    /// Single-segment quadratic bezier curve.
    Bezier,

    /// line segment
    Segment,

    /// Multi-segment bezier curve.
    Curve,

    /// Connection wire. See https://www.shadertoy.com/view/NdsXRl
    Wire,

    /// Text rendering.
    Glyph,

    /// Colored glyph e.g. emoji.
    ColorGlyph,

    /// Path fills.
    PathFill,

    /// Svg with override color
    OverrideColorSvg,
}

#[derive(Copy, Clone, Default)]
#[repr(C)]
pub struct Prim {
    /// Min and max coordinates of the quad we're rendering.
    pub quad_bounds: [f32; 4],

    /// Index of transform applied to drawing region.
    pub xform: u32,

    /// Type of primitive.
    pub prim_type: u32,

    /// Stroke width.
    pub width: f32,

    /// Radius of circles. Corner radius for rounded rectangles.
    pub radius: f32,

    /// Control vertices.
    pub cvs: [f32; 6],

    /// Start of the control vertices, if they're in a separate buffer.
    pub start: u32,

    /// Number of control vertices (vgerCurve and vgerPathFill)
    pub count: u32,

    /// Index of paint applied to drawing region.
    pub paint: u32,

    /// Glyph region index.
    pub glyph: u32,

    /// Min and max coordinates in texture space.
    pub tex_bounds: [f32; 4],

    /// Index of scissor.
    pub scissor: u32,

    pad: u32,
}

mod tests {

    #[test]
    fn test_size() {
        assert_eq!(std::mem::size_of::<super::Prim>(), 96);
    }
}
