use glazier::kurbo::{BezPath, Size};
use leptos_reactive::create_effect;
use vello::{
    peniko::{Brush, Color},
    SceneBuilder,
};
use vello_svg::usvg::{self, NodeExt, Tree};

use crate::{
    app::{AppContext, UpdateMessage},
    context::PaintCx,
    id::Id,
    view::{ChangeFlags, View},
};

pub struct Svg {
    id: Id,
    svg_tree: Option<Tree>,
}

pub fn svg(cx: AppContext, svg_str: impl Fn() -> String + 'static) -> Svg {
    let id = cx.new_id();
    create_effect(cx.scope, move |_| {
        let new_svg_str = svg_str();
        if let Ok(tree) =
            vello_svg::usvg::Tree::from_str(&new_svg_str, &vello_svg::usvg::Options::default())
        {
            AppContext::update_state(id, tree);
        }
    });
    Svg { id, svg_tree: None }
}

impl View for Svg {
    fn id(&self) -> Id {
        self.id
    }

    fn child(&mut self, id: Id) -> Option<&mut dyn View> {
        None
    }

    fn update(
        &mut self,
        cx: &mut crate::context::UpdateCx,
        state: Box<dyn std::any::Any>,
    ) -> crate::view::ChangeFlags {
        if let Ok(state) = state.downcast() {
            self.svg_tree = Some(*state);
            cx.request_layout(self.id());
            ChangeFlags::LAYOUT
        } else {
            ChangeFlags::empty()
        }
    }

    fn layout(&mut self, cx: &mut crate::context::LayoutCx) -> taffy::prelude::Node {
        cx.layout_node(self.id, false, |_| Vec::new())
    }

    fn event(
        &mut self,
        cx: &mut crate::context::EventCx,
        id_path: Option<&[Id]>,
        event: crate::event::Event,
    ) -> bool {
        false
    }

    fn paint(&mut self, cx: &mut crate::context::PaintCx) {
        if let Some(tree) = self.svg_tree.as_mut() {
            if let Some(layout) = cx.get_layout(self.id) {
                if let Some(style) = cx.get_style(self.id) {
                    // let color = Color::rgb8(0xa1, 0xa1, 0xa1);
                    // cx.fill(
                    //     &Size::new(layout.size.width as f64, layout.size.height as f64).to_rect(),
                    //     color,
                    // );
                    let new_size = tree.size.scale_to(
                        vello_svg::usvg::Size::new(
                            layout.size.width as f64,
                            layout.size.height as f64,
                        )
                        .unwrap(),
                    );
                    let scale = new_size.width() / tree.size.width();
                    render_tree(cx, tree, scale);
                }
            }
        }
    }
}

pub fn render_tree(cx: &mut PaintCx, svg: &Tree, scale: f64) {
    for elt in svg.root.descendants() {
        let mut transform = elt.abs_transform();
        transform.scale(scale, scale);
        match &*elt.borrow() {
            usvg::NodeKind::Group(_) => {}
            usvg::NodeKind::Path(path) => {
                let mut local_path = BezPath::new();
                // The semantics of SVG paths don't line up with `BezPath`; we must manually track initial points
                let mut just_closed = false;
                let mut most_recent_initial = (0., 0.);
                for elt in usvg::TransformedPath::new(&path.data, transform) {
                    match elt {
                        usvg::PathSegment::MoveTo { x, y } => {
                            if std::mem::take(&mut just_closed) {
                                local_path.move_to(most_recent_initial);
                            }
                            most_recent_initial = (x, y);
                            local_path.move_to(most_recent_initial)
                        }
                        usvg::PathSegment::LineTo { x, y } => {
                            if std::mem::take(&mut just_closed) {
                                local_path.move_to(most_recent_initial);
                            }
                            local_path.line_to((x, y))
                        }
                        usvg::PathSegment::CurveTo {
                            x1,
                            y1,
                            x2,
                            y2,
                            x,
                            y,
                        } => {
                            if std::mem::take(&mut just_closed) {
                                local_path.move_to(most_recent_initial);
                            }
                            local_path.curve_to((x1, y1), (x2, y2), (x, y))
                        }
                        usvg::PathSegment::ClosePath => {
                            just_closed = true;
                            local_path.close_path()
                        }
                    }
                }

                // FIXME: let path.paint_order determine the fill/stroke order.

                if let Some(fill) = &path.fill {
                    if let Some(brush) = paint_to_brush(&fill.paint, fill.opacity) {
                        // FIXME: Set the fill rule
                        let color = Color::rgb8(0xa1, 0xa1, 0xa1);
                        cx.fill(&local_path, color);
                    }
                }
                if let Some(stroke) = &path.stroke {
                    if let Some(brush) = paint_to_brush(&stroke.paint, stroke.opacity) {
                        // FIXME: handle stroke options such as linecap, linejoin, etc.
                        let color = Color::rgb8(0xa1, 0xa1, 0xa1);
                        cx.stroke(&local_path, color, stroke.width.get());
                    }
                }
            }
            usvg::NodeKind::Image(_) => {}
            usvg::NodeKind::Text(_) => {}
        }
    }
}

fn paint_to_brush(paint: &usvg::Paint, opacity: usvg::Opacity) -> Option<Brush> {
    match paint {
        usvg::Paint::Color(color) => Some(Brush::Solid(Color::rgba8(
            color.red,
            color.green,
            color.blue,
            opacity.to_u8(),
        ))),
        usvg::Paint::LinearGradient(_) => None,
        usvg::Paint::RadialGradient(_) => None,
        usvg::Paint::Pattern(_) => None,
    }
}
