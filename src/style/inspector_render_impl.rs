//! Concrete `InspectorRender` implementation that builds `floem` view
//! widgets. The `PropDebugView` impls for types owned by `floem_style`
//! (Color, Gradient, Brush, Stroke, Rect, Affine, ObjectFit,
//! ObjectPosition, Transition) delegate their widget-building to the
//! methods on this struct; the bodies here are the same view code that
//! previously lived inline on each `PropDebugView` impl.

use std::any::Any;

use floem_reactive::{RwSignal, SignalGet, SignalUpdate as _};
use floem_renderer::Renderer;
use floem_style::{AffineLerp, InspectorRender, Transition};
use peniko::color::palette;
use peniko::kurbo::{self, Affine, Point, Rect, Stroke};
use peniko::{Brush, Color, Gradient, GradientKind, LinearGradientPosition};
use taffy::prelude::{auto, fr};

use crate::prelude::ViewTuple;
use crate::style::values::views;
use crate::style::{FontSizeCx, ObjectFit, ObjectPosition};
use crate::theme::{StyleThemeExt, Theme};
use crate::view::{IntoView, View};
use crate::views::{ContainerExt, Decorators, Empty, Label, Stack, StackExt, TooltipExt, canvas};

/// The concrete renderer used inside the `floem` crate. All methods
/// produce a `Box<dyn View>` wrapped inside a `Box<dyn Any>` so inspector
/// call sites can downcast back to `Box<dyn View>`.
pub struct FloemInspectorRender;

fn erase_view(view: impl View + 'static) -> Box<dyn Any> {
    let view: Box<dyn View> = view.into_any();
    Box::new(view)
}

impl InspectorRender for FloemInspectorRender {
    fn empty(&self) -> Box<dyn Any> {
        erase_view(Empty::new())
    }

    fn text(&self, s: &str) -> Box<dyn Any> {
        erase_view(Label::new(s))
    }

    fn sequence(&self, items: Vec<Box<dyn Any>>) -> Box<dyn Any> {
        let views: Vec<Box<dyn View>> = items
            .into_iter()
            .filter_map(|any| any.downcast::<Box<dyn View>>().ok().map(|b| *b))
            .collect();
        erase_view(Stack::vertical_from_iter(views).style(|s| s.gap(4.0)))
    }

    fn color(&self, c: Color) -> Box<dyn Any> {
        let color = c;
        let swatch = ()
            .style(move |s| {
                s.background(color)
                    .width(22.0)
                    .height(14.0)
                    .border(1.)
                    .border_color(palette::css::WHITE.with_alpha(0.5))
                    .border_radius(5.0)
            })
            .container()
            .style(|s| {
                s.border(1.)
                    .border_color(palette::css::BLACK.with_alpha(0.5))
                    .border_radius(5.0)
            });

        let tooltip_view = move || {
            // Convert to RGBA8 for standard representations
            let c = color.to_rgba8();
            let (r, g, b, a) = (c.r, c.g, c.b, c.a);

            // Hex representation
            let hex = if a == 255 {
                format!("#{:02X}{:02X}{:02X}", r, g, b)
            } else {
                format!("#{:02X}{:02X}{:02X}{:02X}", r, g, b, a)
            };

            // RGBA string
            let rgba_str = format!("rgba({}, {}, {}, {:.3})", r, g, b, a as f32 / 255.0);

            // Alpha percentage
            let alpha_str = format!(
                "{:.1}% ({:.3})",
                (a as f32 / 255.0) * 100.0,
                a as f32 / 255.0
            );

            let components = color.components;
            let color_space_str = format!("{:?}", color.cs);

            let hex = views((
                "Hex:".style(|s| s.font_bold().min_width(80.0).justify_end()),
                Label::derived(move || hex.clone()),
            ));
            let rgba = views((
                "RGBA:".style(|s| s.font_bold().min_width(80.0).justify_end()),
                Label::derived(move || rgba_str.clone()),
            ));
            let components = views((
                "Components:".style(|s| s.font_bold().min_width(80.0).justify_end()),
                (
                    Label::derived(move || format!("[0]: {:.3}", components[0])),
                    Label::derived(move || format!("[1]: {:.3}", components[1])),
                    Label::derived(move || format!("[2]: {:.3}", components[2])),
                    Label::derived(move || format!("[3]: {:.3}", components[3])),
                )
                    .v_stack()
                    .style(|s| s.gap(2.0)),
            ));
            let color_space = views((
                "Color Space:".style(|s| s.font_bold().min_width(80.0).justify_end()),
                Label::derived(move || color_space_str.clone()),
            ));
            let alpha = views((
                "Alpha:".style(|s| s.font_bold().min_width(80.0).justify_end()),
                Label::derived(move || alpha_str.clone()),
            ));
            use crate::view::ViewTupleFlat;
            (hex, rgba, components, color_space, alpha)
                .flatten()
                .style(|s| {
                    s.grid()
                        .grid_template_columns([auto(), fr(1.)])
                        .justify_center()
                        .items_center()
                        .row_gap(20)
                        .col_gap(10)
                        .padding(30)
                })
        };

        erase_view(
            swatch
                .tooltip(tooltip_view)
                .style(|s| s.items_center()),
        )
    }

    fn gradient(&self, g: &Gradient) -> Box<dyn Any> {
        let box_width = 22.;
        let box_height = 14.;
        let mut grad = g.clone();
        let raw = g.clone();
        grad.kind = match grad.kind {
            GradientKind::Linear(LinearGradientPosition { start, end }) => {
                let dx = end.x - start.x;
                let dy = end.y - start.y;

                let scale_x = box_width / dx.abs();
                let scale_y = box_height / dy.abs();
                let scale = scale_x.min(scale_y);

                let new_dx = dx * scale;
                let new_dy = dy * scale;

                let new_start = Point {
                    x: if dx > 0.0 { 0.0 } else { box_width },
                    y: if dy > 0.0 { 0.0 } else { box_height },
                };

                let new_end = Point {
                    x: new_start.x + new_dx,
                    y: new_start.y + new_dy,
                };

                GradientKind::Linear(LinearGradientPosition {
                    start: new_start,
                    end: new_end,
                })
            }
            _ => grad.kind,
        };
        let color = ().style(move |s| {
            s.background(grad.clone())
                .width(box_width)
                .height(box_height)
                .border(1.)
                .border_color(palette::css::WHITE.with_alpha(0.5))
                .border_radius(5.0)
        });
        let color = color.container().style(|s| {
            s.border(1.)
                .border_color(palette::css::BLACK.with_alpha(0.5))
                .border_radius(5.0)
                .margin_left(6.0)
        });
        erase_view(
            Stack::new((Label::new(format!("{raw:?}")), color)).style(|s| s.items_center()),
        )
    }

    fn brush(&self, b: &Brush) -> Box<dyn Any> {
        match b {
            Brush::Solid(color) => self.color(*color),
            Brush::Gradient(grad) => self.gradient(grad),
            Brush::Image(_) => erase_view(Empty::new()),
        }
    }

    fn stroke(&self, s: &Stroke) -> Box<dyn Any> {
        let stroke = s.clone();
        let clone = stroke.clone();

        let color = RwSignal::new(palette::css::RED);

        // Visual preview of the stroke
        let preview = canvas(move |cx, size| {
            cx.stroke(
                &kurbo::Line::new(
                    Point::new(0., size.height / 2.),
                    Point::new(size.width, size.height / 2.),
                ),
                color.get(),
                &clone,
            );
        })
        .style(move |s| s.width(80.0).height(20.0))
        .container()
        .style(move |s| {
            s.with_theme(move |s, t| s.border_color(t.border()))
                .defer::<Theme>(move |t| color.set(t.primary()))
                .padding(4.0)
        });

        let tooltip_view = move || {
            let stroke = stroke.clone();

            let width_row = views((
                "Width:".style(|s| s.font_bold().min_width(100.0).justify_end()),
                Label::derived(move || format!("{:.1}px", stroke.width)),
            ));

            let join_row = views((
                "Join:".style(|s| s.font_bold().min_width(100.0).justify_end()),
                Label::derived(move || format!("{:?}", stroke.join)),
            ));

            let miter_row = views((
                "Miter Limit:".style(|s| s.font_bold().min_width(100.0).justify_end()),
                Label::derived(move || format!("{:.2}", stroke.miter_limit)),
            ));

            let start_cap_row = views((
                "Start Cap:".style(|s| s.font_bold().min_width(100.0).justify_end()),
                Label::derived(move || format!("{:?}", stroke.start_cap)),
            ));

            let end_cap_row = views((
                "End Cap:".style(|s| s.font_bold().min_width(100.0).justify_end()),
                Label::derived(move || format!("{:?}", stroke.end_cap)),
            ));

            let pattern_clone = stroke.dash_pattern.clone();

            let dash_pattern_row = views((
                "Dash Pattern:".style(|s| s.font_bold().min_width(100.0).justify_end()),
                Label::derived(move || {
                    if pattern_clone.is_empty() {
                        "Solid".to_string()
                    } else {
                        format!("{:?}", pattern_clone.as_slice())
                    }
                }),
            ));

            let dash_offset_row = if !stroke.dash_pattern.is_empty() {
                Some(views((
                    "Dash Offset:".style(|s| s.font_bold().min_width(100.0).justify_end()),
                    Label::derived(move || format!("{:.1}", stroke.dash_offset)),
                )))
            } else {
                None
            };

            let mut rows = vec![
                width_row.into_any(),
                join_row.into_any(),
                miter_row.into_any(),
                start_cap_row.into_any(),
                end_cap_row.into_any(),
                dash_pattern_row.into_any(),
            ];

            if let Some(offset_row) = dash_offset_row {
                rows.push(offset_row.into_any());
            }

            Stack::vertical_from_iter(rows).style(|s| {
                s.grid()
                    .grid_template_columns([auto(), fr(1.)])
                    .justify_center()
                    .items_center()
                    .row_gap(12)
                    .col_gap(10)
                    .padding(20)
            })
        };

        erase_view(
            preview
                .tooltip(tooltip_view)
                .style(|s| s.items_center()),
        )
    }

    fn rect(&self, r: &Rect) -> Box<dyn Any> {
        let r = *r;

        let w = r.x1 - r.x0;
        let h = r.y1 - r.y0;

        let coords = [
            format!("x0: {:.2}", r.x0),
            format!("y0: {:.2}", r.y0),
            format!("x1: {:.2}", r.x1),
            format!("y1: {:.2}", r.y1),
        ]
        .v_stack();

        let wh = [format!("w: {:.2}", w), format!("h: {:.2}", h)].h_stack();

        let preview = Empty::new().style(move |s| {
            let max = w.abs().max(h.abs()).max(1.0);
            let scale = 60.0 / max;

            s.width(w.abs() * scale)
                .height(h.abs() * scale)
                .border(1.0)
                .with_theme(|s, t| {
                    s.border_color(t.border())
                        .background(t.primary_muted())
                        .border_radius(t.border_radius())
                })
        });

        erase_view(
            (
                "Rect",
                preview,
                coords.style(|s| s.gap(2)),
                wh.style(|s| s.gap(8)),
            )
                .v_stack(),
        )
    }

    fn affine(&self, a: &Affine) -> Box<dyn Any> {
        let affine = *a;
        let coeffs = affine.as_coeffs();

        // Decompose to show meaningful transform components
        let (scale, rotation) = affine.svd();
        let translation = affine.translation();

        // Create a visual preview showing the transform effect
        let preview = canvas(move |cx, size| {
            let center = Point::new(size.width / 2., size.height / 2.);
            let box_size = 20.0;

            // Draw original position (dashed outline)
            let original_rect =
                kurbo::Rect::from_center_size(center, kurbo::Size::new(box_size, box_size));
            cx.stroke(
                &original_rect,
                palette::css::GRAY.with_alpha(0.5),
                &kurbo::Stroke::new(1.0).with_dashes(0., [3., 3.]),
            );

            // Draw transformed position
            let transform_offset =
                Affine::translate((center.x - box_size / 2., center.y - box_size / 2.));
            let display_transform = transform_offset * affine * transform_offset.inverse();

            let transformed_rect = kurbo::Rect::new(0., 0., box_size, box_size);
            cx.fill(
                &display_transform.transform_rect_bbox(transformed_rect),
                palette::css::BLUE.with_alpha(0.7),
                0.,
            );
            cx.stroke(
                &(display_transform.transform_rect_bbox(transformed_rect)),
                palette::css::BLUE,
                &kurbo::Stroke::new(2.0),
            );

            // Draw origin point
            let origin_marker = kurbo::Circle::new(display_transform * Point::ZERO, 3.0);
            cx.fill(&origin_marker, palette::css::RED, 0.);
        })
        .style(|s| s.width(80.0).height(60.0))
        .container()
        .style(|s| {
            s.padding(4.0)
                .border(1.)
                .border_radius(5.0)
                .with_theme(|s, t| s.border_color(t.border()))
        });

        let tooltip_view = move || {
            // Matrix coefficients in a grid
            let matrix_label = Label::new("Matrix:").style(|s| s.font_bold().margin_bottom(8.0));

            let matrix_grid = (
                views((
                    Label::new(format!("{:.3}", coeffs[0])),
                    Label::new(format!("{:.3}", coeffs[2])),
                    Label::new(format!("{:.3}", coeffs[4])),
                )),
                views((
                    Label::new(format!("{:.3}", coeffs[1])),
                    Label::new(format!("{:.3}", coeffs[3])),
                    Label::new(format!("{:.3}", coeffs[5])),
                )),
                views((Label::new("0"), Label::new("0"), Label::new("1"))),
            )
                .v_stack()
                .style(|s| {
                    s.gap(4.0)
                        .padding(8.0)
                        .border(1.)
                        .border_radius(4.0)
                        .with_theme(|s, t| {
                            s.background(t.def(|t| t.primary().with_alpha(0.5)))
                                .border_color(t.border())
                        })
                });

            // Decomposed components
            let components_label = Label::new("Components:")
                .style(|s| s.font_bold().margin_top(16.0).margin_bottom(8.0));

            let translate_row = views((
                "Translate:".style(|s| s.font_bold().min_width(100.0).justify_end()),
                Label::derived(move || format!("({:.2}, {:.2})", translation.x, translation.y)),
            ));

            let rotate_row = views((
                "Rotate:".style(|s| s.font_bold().min_width(100.0).justify_end()),
                Label::derived(move || format!("{:.1}°", rotation.to_degrees())),
            ));

            let scale_row = views((
                "Scale:".style(|s| s.font_bold().min_width(100.0).justify_end()),
                Label::derived(move || format!("({:.2}, {:.2})", scale.x, scale.y)),
            ));

            // Check for special properties
            let is_identity = affine == Affine::IDENTITY;
            let determinant = coeffs[0] * coeffs[3] - coeffs[1] * coeffs[2];
            let has_reflection = determinant < 0.0;

            let properties = if is_identity {
                Some(
                    Label::new("Identity (no transform)")
                        .style(|s| s.with_theme(|s, t| s.color(t.text_muted()))),
                )
            } else if has_reflection {
                Some(
                    Label::new("⚠ Contains reflection")
                        .style(|s| s.with_theme(|s, t| s.color(t.warning()))),
                )
            } else {
                None
            };

            use crate::view::ViewTupleFlat;
            let components_grid = (translate_row, rotate_row, scale_row).flatten().style(|s| {
                s.grid()
                    .grid_template_columns([auto(), fr(1.)])
                    .justify_center()
                    .items_center()
                    .row_gap(8)
                    .col_gap(10)
            });

            let mut content = vec![
                matrix_label.into_any(),
                matrix_grid.into_any(),
                components_label.into_any(),
                components_grid.into_any(),
            ];

            if let Some(props) = properties {
                content.push(props.into_any());
            }

            Stack::vertical_from_iter(content).style(|s| s.padding(20))
        };

        erase_view(
            preview
                .tooltip(tooltip_view)
                .style(|s| s.items_center()),
        )
    }

    fn object_fit(&self, f: ObjectFit) -> Box<dyn Any> {
        use peniko::kurbo::RoundedRect;

        let object_fit = f;
        let container_color = RwSignal::new(palette::css::GRAY);
        let image_color = RwSignal::new(palette::css::BLUE);

        // Visual preview showing how an image with 2:1 aspect ratio fits in a square container
        let preview = canvas(move |cx, size| {
            let width = size.width;
            let height = size.height;
            let padding = 4.0;
            let container_size = width.min(height) - padding * 2.0;

            // Draw container box (square)
            let container_x = (width - container_size) / 2.0;
            let container_y = (height - container_size) / 2.0;
            let container_rect = RoundedRect::from_rect(
                kurbo::Rect::new(
                    container_x,
                    container_y,
                    container_x + container_size,
                    container_y + container_size,
                ),
                2.0,
            );
            cx.stroke(
                &container_rect,
                container_color.get(),
                &Stroke {
                    width: 1.5,
                    ..Default::default()
                },
            );

            // Simulate an image with 2:1 aspect ratio (wider than tall)
            let image_aspect = 2.0;
            let (img_width, img_height) = match object_fit {
                ObjectFit::Fill => {
                    // Stretch to fill container
                    (container_size, container_size)
                }
                ObjectFit::Contain => {
                    // Fit inside while maintaining aspect ratio
                    // Image is 2:1, container is 1:1, so width is the constraint
                    let w = container_size;
                    let h = w / image_aspect;
                    (w, h)
                }
                ObjectFit::Cover => {
                    // Cover entire container while maintaining aspect ratio
                    // Height is the constraint
                    let h = container_size;
                    let w = h * image_aspect;
                    (w, h)
                }
                ObjectFit::ScaleDown => {
                    // Like contain but don't scale up
                    // Assume natural image size is smaller than container
                    let natural_w = container_size * 0.6;
                    let natural_h = natural_w / image_aspect;
                    (natural_w, natural_h)
                }
                ObjectFit::None => {
                    // Natural size (simulated as 60% of container)
                    let natural_w = container_size * 0.6;
                    let natural_h = natural_w / image_aspect;
                    (natural_w, natural_h)
                }
            };

            // Center the image in the container
            let img_x = container_x + (container_size - img_width) / 2.0;
            let img_y = container_y + (container_size - img_height) / 2.0;

            // Clip to container bounds for Cover mode
            if matches!(object_fit, ObjectFit::Cover) {
                // Draw the image rect (it will extend beyond container)
                let img_rect = RoundedRect::from_rect(
                    kurbo::Rect::new(img_x, img_y, img_x + img_width, img_y + img_height),
                    2.0,
                );
                // Show it as semi-transparent to indicate it's clipped
                let clipped_color = image_color.get().with_alpha(0.7);
                cx.fill(&img_rect, clipped_color, 0.0);
            } else {
                // Draw the image rect normally
                let img_rect = RoundedRect::from_rect(
                    kurbo::Rect::new(img_x, img_y, img_x + img_width, img_y + img_height),
                    2.0,
                );
                cx.fill(&img_rect, image_color.get(), 0.0);
            }
        })
        .style(|s| s.width(70.0).height(70.0))
        .container()
        .style(move |s| {
            s.padding(4.0)
                .border(1.)
                .border_radius(5.0)
                .with_theme(move |s, t| s.border_color(t.border()))
        });

        let label_text = match object_fit {
            ObjectFit::Fill => "Fill",
            ObjectFit::Contain => "Contain",
            ObjectFit::Cover => "Cover",
            ObjectFit::ScaleDown => "ScaleDown",
            ObjectFit::None => "None",
        };

        let tooltip_view = move || {
            let description = match object_fit {
                ObjectFit::Fill => "Stretches content to fill the box.\nMay distort aspect ratio.",
                ObjectFit::Contain => {
                    "Scales content to fit inside the box.\nPreserves aspect ratio (letterboxed)."
                }
                ObjectFit::Cover => {
                    "Scales content to cover the box.\nPreserves aspect ratio (may clip)."
                }
                ObjectFit::ScaleDown => {
                    "Like 'contain' but won't scale up.\nNever larger than natural size."
                }
                ObjectFit::None => {
                    "Content keeps its natural size.\nMay overflow or be smaller than box."
                }
            };

            Stack::vertical((
                Label::new(label_text).style(|s| s.font_bold()),
                Label::new(description).style(|s| s.with_theme(|s, t| s.color(t.text_muted()))),
            ))
            .style(|s| s.gap(8.0).padding(12.0).max_width(220.0))
        };

        erase_view(
            preview
                .tooltip(tooltip_view)
                .style(|s| s.items_center()),
        )
    }

    fn object_position(&self, p: &ObjectPosition) -> Box<dyn Any> {
        use peniko::kurbo::{Circle, RoundedRect};

        let object_position = *p;
        let container_color = RwSignal::new(palette::css::GRAY);
        let image_color = RwSignal::new(palette::css::BLUE);
        let marker_color = RwSignal::new(palette::css::RED);

        let preview = canvas(move |cx, size| {
            let width = size.width;
            let height = size.height;
            let padding = 6.0;
            let container_size = width.min(height) - padding * 2.0;
            let container_x = (width - container_size) / 2.0;
            let container_y = (height - container_size) / 2.0;
            let container_rect = RoundedRect::from_rect(
                kurbo::Rect::new(
                    container_x,
                    container_y,
                    container_x + container_size,
                    container_y + container_size,
                ),
                2.0,
            );
            cx.stroke(
                &container_rect,
                container_color.get(),
                &Stroke {
                    width: 1.5,
                    ..Default::default()
                },
            );

            let image_w = container_size * 0.55;
            let image_h = container_size * 0.35;
            let free_x = container_size - image_w;
            let free_y = container_size - image_h;
            let font_size_cx = FontSizeCx::new(16.0, 16.0);

            let (offset_x, offset_y) = match object_position {
                ObjectPosition::TopLeft => (0.0, 0.0),
                ObjectPosition::Top => (free_x * 0.5, 0.0),
                ObjectPosition::TopRight => (free_x, 0.0),
                ObjectPosition::Left => (0.0, free_y * 0.5),
                ObjectPosition::Center => (free_x * 0.5, free_y * 0.5),
                ObjectPosition::Right => (free_x, free_y * 0.5),
                ObjectPosition::BottomLeft => (0.0, free_y),
                ObjectPosition::Bottom => (free_x * 0.5, free_y),
                ObjectPosition::BottomRight => (free_x, free_y),
                ObjectPosition::Custom(x, y) => (
                    x.resolve(free_x, &font_size_cx),
                    y.resolve(free_y, &font_size_cx),
                ),
            };

            let img_x = container_x + offset_x;
            let img_y = container_y + offset_y;
            let img_rect = RoundedRect::from_rect(
                kurbo::Rect::new(img_x, img_y, img_x + image_w, img_y + image_h),
                2.0,
            );
            cx.fill(&img_rect, image_color.get(), 0.0);

            let marker = Circle::new(
                kurbo::Point::new(img_x + image_w / 2.0, img_y + image_h / 2.0),
                2.5,
            );
            cx.fill(&marker, marker_color.get(), 0.0);
        })
        .style(|s| s.width(70.0).height(70.0))
        .container()
        .style(move |s| {
            s.padding(4.0)
                .border(1.)
                .border_radius(5.0)
                .with_theme(move |s, t| s.border_color(t.border()))
        });

        let (label_text, description) = match object_position {
            ObjectPosition::TopLeft => ("TopLeft", "Anchors content to the top-left corner."),
            ObjectPosition::Top => (
                "Top",
                "Anchors content to the top edge, centered horizontally.",
            ),
            ObjectPosition::TopRight => ("TopRight", "Anchors content to the top-right corner."),
            ObjectPosition::Left => (
                "Left",
                "Anchors content to the left edge, centered vertically.",
            ),
            ObjectPosition::Center => ("Center", "Centers content on both axes."),
            ObjectPosition::Right => (
                "Right",
                "Anchors content to the right edge, centered vertically.",
            ),
            ObjectPosition::BottomLeft => {
                ("BottomLeft", "Anchors content to the bottom-left corner.")
            }
            ObjectPosition::Bottom => (
                "Bottom",
                "Anchors content to the bottom edge, centered horizontally.",
            ),
            ObjectPosition::BottomRight => {
                ("BottomRight", "Anchors content to the bottom-right corner.")
            }
            ObjectPosition::Custom(x, y) => {
                let label = format!("Custom({x:?}, {y:?})");
                let description = "Uses explicit horizontal and vertical offsets. Percentages resolve against remaining free space.";
                return erase_view(
                    preview
                        .tooltip(move || {
                            Stack::vertical((
                                Label::new(label.clone()).style(|s| s.font_bold()),
                                Label::new(description)
                                    .style(|s| s.with_theme(|s, t| s.color(t.text_muted()))),
                            ))
                            .style(|s| s.gap(8.0).padding(12.0).max_width(240.0))
                        })
                        .style(|s| s.items_center()),
                );
            }
        };

        erase_view(
            preview
                .tooltip(move || {
                    Stack::vertical((
                        Label::new(label_text).style(|s| s.font_bold()),
                        Label::new(description)
                            .style(|s| s.with_theme(|s, t| s.color(t.text_muted()))),
                    ))
                    .style(|s| s.gap(8.0).padding(12.0).max_width(240.0))
                })
                .style(|s| s.items_center()),
        )
    }

    fn transition(&self, t: &Transition) -> Box<dyn Any> {
        let transition = t.clone();
        let easing_clone = transition.easing.clone();

        let curve_color = RwSignal::new(palette::css::BLUE);
        let axis_color = RwSignal::new(palette::css::GRAY);

        // Visual preview of the easing curve
        let preview = canvas(move |cx, size| {
            let width = size.width;
            let height = size.height;
            let padding = 4.0;
            let graph_width = width - padding * 2.0;
            let graph_height = height - padding * 2.0;

            // Sample the easing function
            let sample_count = 50;
            let mut path = kurbo::BezPath::new();

            for i in 0..=sample_count {
                let t = i as f64 / sample_count as f64;
                let eased = easing_clone.eval(t);
                let x = padding + t * graph_width;
                let y = padding + (1.0 - eased) * graph_height;

                if i == 0 {
                    path.move_to(Point::new(x, y));
                } else {
                    path.line_to(Point::new(x, y));
                }
            }

            // Draw the curve
            cx.stroke(
                &path,
                curve_color.get(),
                &Stroke {
                    width: 2.0,
                    ..Default::default()
                },
            );

            // Draw axes
            let axis_stroke = Stroke {
                width: 1.0,
                ..Default::default()
            };

            // X axis
            cx.stroke(
                &kurbo::Line::new(
                    Point::new(padding, height - padding),
                    Point::new(width - padding, height - padding),
                ),
                axis_color.get(),
                &axis_stroke,
            );

            // Y axis
            cx.stroke(
                &kurbo::Line::new(
                    Point::new(padding, padding),
                    Point::new(padding, height - padding),
                ),
                axis_color.get(),
                &axis_stroke,
            );
        })
        .style(|s| s.width(80.0).height(60.0))
        .container()
        .style(move |s| {
            s.padding(4.0)
                .border(1.)
                .border_radius(5.0)
                .with_theme(move |s, t| s.border_color(t.border()))
        });

        let tooltip_view = move || {
            let transition = transition.clone();

            let duration_row = views((
                "Duration:".style(|s| s.font_bold().min_width(80.0).justify_end()),
                Label::derived(move || format!("{:.0}ms", transition.duration.as_millis())),
            ));

            let easing_name = format!("{:?}", transition.easing);
            let easing_row = views((
                "Easing:".style(|s| s.font_bold().min_width(80.0).justify_end()),
                Label::derived(move || easing_name.clone()),
            ));

            // Show velocity at key points if available
            let velocity_samples = if transition.easing.velocity(0.0).is_some() {
                let samples = vec![0.0, 0.25, 0.5, 0.75, 1.0]
                    .into_iter()
                    .filter_map(|t| {
                        transition
                            .easing
                            .velocity(t)
                            .map(|v| Label::new(format!("t={:.2}: {:.3}", t, v)))
                    })
                    .collect::<Vec<_>>();

                if !samples.is_empty() {
                    Some(views((
                        "Velocity:".style(|s| s.font_bold().min_width(80.0).justify_end()),
                        Stack::vertical_from_iter(samples).style(|s| s.gap(2.0)),
                    )))
                } else {
                    None
                }
            } else {
                None
            };

            let mut rows = vec![duration_row.into_any(), easing_row.into_any()];

            if let Some(velocity_row) = velocity_samples {
                rows.push(velocity_row.into_any());
            }

            Stack::vertical_from_iter(rows).style(|s| {
                s.grid()
                    .grid_template_columns([auto(), fr(1.)])
                    .justify_center()
                    .items_center()
                    .row_gap(12)
                    .col_gap(10)
                    .padding(20)
            })
        };

        erase_view(
            preview
                .tooltip(tooltip_view)
                .style(|s| s.items_center()),
        )
    }
}

