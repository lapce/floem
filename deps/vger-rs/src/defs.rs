use euclid::*;

pub struct ScreenSpace;
pub type ScreenSize = Size2D<f32, ScreenSpace>;

pub struct WorldSpace;
pub type WorldPoint = Point2D<f32, WorldSpace>;

pub struct LocalSpace {}
pub type LocalPoint = Point2D<f32, LocalSpace>;
pub type LocalVector = Vector2D<f32, LocalSpace>;
pub type LocalSize = Size2D<f32, LocalSpace>;

pub type LocalToWorld = Transform2D<f32, LocalSpace, WorldSpace>;
pub type WorldToLocal = Transform2D<f32, WorldSpace, LocalSpace>;
pub type LocalTransform = Transform2D<f32, LocalSpace, LocalSpace>;

pub type LocalRect = Rect<f32, LocalSpace>;
