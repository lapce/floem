use floem_reactive::{RwSignal, SignalGet, SignalUpdate};
use imaging::{
    Brush, Composite, Filter,
    record::{AppliedMask, Clip, Command, Context, Draw, Geometry, Group, Mask, Scene, replay},
};
use peniko::{
    Color,
    color::palette::css,
    kurbo::{Affine, Rect, Shape, Size},
};

use crate::{
    AnyView, IntoView, View,
    style::{BoxShadow, StylePropValue, StyleThemeExt},
    views::{ContainerExt, Decorators, Label, ScrollExt, Stack, canvas, dyn_view, svg},
};

#[derive(Clone)]
enum ResolvedCommand {
    PushContext(Context),
    PopContext,
    PushClip(Clip),
    PopClip,
    PushGroup(Group),
    PopGroup,
    Draw(Draw),
}

fn format_float(value: f64) -> String {
    if value.fract().abs() < 0.01 {
        format!("{}", value.round() as i64)
    } else {
        format!("{value:.2}")
    }
}

fn format_point(x: f64, y: f64) -> String {
    format!("({}, {})", format_float(x), format_float(y))
}

fn format_affine(transform: Affine) -> String {
    let [a, b, c, d, e, f] = transform.as_coeffs();
    format!(
        "[{:.3}, {:.3}, {:.3}, {:.3}, {:.3}, {:.3}]",
        a, b, c, d, e, f
    )
}

fn detail_label(text: impl Into<String>) -> AnyView {
    Label::new(text.into())
        .style(|s| s.font_size(11.0).min_width(0.0).text_wrap())
        .into_any()
}

fn debug_block(text: impl Into<String>) -> AnyView {
    Label::new(text.into())
        .style(|s| {
            s.font_size(11.0)
                .min_width(0.0)
                .text_wrap()
                .padding_vert(2.0)
        })
        .into_any()
}

fn value_view<T: IntoView + Clone>(value: &T) -> AnyView
where
    T::V: View + 'static,
{
    value.clone().into_any()
}

fn field_row(name: impl Into<String>, value: impl Into<String>) -> AnyView {
    Stack::horizontal((
        Label::new(format!("{}:", name.into())).style(|s| {
            s.font_size(11.0)
                .font_bold()
                .min_width(92.0)
                .padding_right(10.0)
                .with_theme(|s, t| s.color(t.text_muted()))
        }),
        detail_label(value),
    ))
    .style(|s| s.items_start().width_full().min_width(0.0).gap(2.0))
    .into_any()
}

fn field_view(name: impl Into<String>, value: AnyView) -> AnyView {
    Stack::horizontal((
        Label::new(format!("{}:", name.into())).style(|s| {
            s.font_size(11.0)
                .font_bold()
                .min_width(92.0)
                .padding_right(10.0)
                .with_theme(|s, t| s.color(t.text_muted()))
        }),
        value.style(|s| s.min_width(0.0).flex_grow(1.0)),
    ))
    .style(|s| s.items_start().width_full().min_width(0.0).gap(2.0))
    .into_any()
}

fn field_value<T: IntoView + Clone>(name: impl Into<String>, value: &T) -> AnyView
where
    T::V: View + 'static,
{
    field_view(name, value_view(value))
}

fn section(title: impl Into<String>, body: AnyView) -> AnyView {
    Stack::vertical((
        Label::new(title.into()).style(|s| {
            s.font_size(10.5)
                .font_bold()
                .with_theme(|s, t| s.color(t.text_muted()))
        }),
        body.style(|s| s.padding_left(12.0)),
    ))
    .style(|s| {
        s.width_full()
            .gap(6.0)
            .padding(8.0)
            .border_radius(8.0)
            .border(1.0)
            .with_theme(|s, t| s.background(t.bg_base()).border_color(t.border()))
    })
    .into_any()
}

fn stack_rows(rows: Vec<AnyView>) -> AnyView {
    Stack::vertical_from_iter(rows)
        .style(|s| s.width_full().gap(4.0).min_width(0.0))
        .into_any()
}

fn metric_chip(label: impl Into<String>, value: impl Into<String>) -> AnyView {
    Stack::horizontal((
        Label::new(label.into()).style(|s| {
            s.font_size(10.0)
                .font_bold()
                .with_theme(|s, t| s.color(t.text_muted()))
        }),
        Label::new(value.into()).style(|s| s.font_size(11.0)),
    ))
    .style(|s| s.items_center().gap(4.0))
    .into_any()
}

fn bounds_view(rect: Rect) -> AnyView {
    Stack::vertical((
        Stack::horizontal((
            metric_chip("x", format_float(rect.x0)),
            metric_chip("y", format_float(rect.y0)),
        )),
        Stack::horizontal((
            metric_chip("w", format_float(rect.width())),
            metric_chip("h", format_float(rect.height())),
        )),
    ))
    .style(|s| s.gap(2.0).min_width(0.0))
    .into_any()
}

fn disclosure_icon(expanded: bool) -> AnyView {
    let icon = if expanded {
        r#"<svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor"><path d="M4.427 6.427l3.396 3.396a.25.25 0 00.354 0l3.396-3.396A.25.25 0 0011.396 6H4.604a.25.25 0 00-.177.427z"/></svg>"#
    } else {
        r#"<svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor"><path d="M6.427 4.427l3.396 3.396a.25.25 0 010 .354l-3.396 3.396A.25.25 0 016 11.396V4.604a.25.25 0 01.427-.177z"/></svg>"#
    };
    svg(icon)
        .style(|s| {
            s.size(14.0, 14.0)
                .with_theme(|s, t| s.color(t.text_muted()))
        })
        .into_any()
}

fn collapsible_section(
    title: impl Into<String>,
    subtitle: impl Into<Option<String>>,
    default_open: bool,
    body: impl Fn() -> AnyView + 'static,
) -> AnyView {
    let title = title.into();
    let subtitle = subtitle.into();
    let expanded = RwSignal::new(default_open);
    let subtitle_view = subtitle
        .map(|subtitle| {
            Label::new(subtitle)
                .style(|s| s.font_size(10.0).with_theme(|s, t| s.color(t.text_muted())))
                .into_any()
        })
        .unwrap_or_else(|| ().into_any());
    Stack::vertical((
        Stack::horizontal((
            dyn_view(move || disclosure_icon(expanded.get())),
            Stack::vertical((
                Label::new(title).style(|s| s.font_size(11.0).font_bold()),
                subtitle_view,
            ))
            .style(|s| s.gap(1.0).min_width(0.0).flex_grow(1.0)),
        ))
        .style(|s| {
            s.items_start()
                .gap(6.0)
                .width_full()
                .padding_horiz(8.0)
                .padding_vert(7.0)
                .border_radius(8.0)
                .border(1.0)
                .with_theme(|s, t| s.background(t.bg_base()).border_color(t.border()))
        })
        .action(move || expanded.update(|value| *value = !*value)),
        dyn_view(move || {
            if expanded.get() {
                body()
                    .style(|s| s.padding_left(10.0).padding_top(4.0))
                    .into_any()
            } else {
                ().into_any()
            }
        }),
    ))
    .style(|s| s.width_full().gap(2.0))
    .into_any()
}

fn raw_debug_section(title: impl Into<String>, text: impl Into<String>) -> AnyView {
    let text = text.into();
    let subtitle = format!("{} characters", text.len());
    collapsible_section(title, Some(subtitle), false, move || {
        debug_block(text.clone())
            .style(|s| {
                s.padding(8.0)
                    .border_radius(8.0)
                    .border(1.0)
                    .with_theme(|s, t| s.background(t.bg_base()).border_color(t.border()))
            })
            .into_any()
    })
}

fn scene_size_hint(size: Size) -> Size {
    let width = size.width.clamp(120.0, 220.0);
    let height = size.height.clamp(72.0, 160.0);
    Size::new(width, height)
}

fn nested_scene_section(
    title: impl Into<String>,
    subtitle: impl Into<String>,
    scene: Scene,
    size: Size,
) -> AnyView {
    collapsible_section(title, Some(subtitle.into()), false, move || {
        scene_debug_view_with_size(scene.clone(), scene_size_hint(size))
            .style(|s| {
                s.padding(8.0)
                    .border_radius(10.0)
                    .border(1.0)
                    .with_theme(|s, t| s.background(t.bg_base()).border_color(t.border()))
            })
            .into_any()
    })
}

fn brush_summary_view(brush: &Brush) -> AnyView {
    match brush {
        Brush::Solid(color) => Stack::horizontal((
            Label::new("solid").style(|s| {
                s.font_size(10.0)
                    .font_bold()
                    .with_theme(|s, t| s.color(t.text_muted()))
            }),
            color.clone().into_any(),
        ))
        .style(|s| s.items_center().gap(8.0).min_width(0.0))
        .into_any(),
        Brush::Gradient(gradient) => Stack::vertical((
            Label::new("gradient").style(|s| {
                s.font_size(10.0)
                    .font_bold()
                    .with_theme(|s, t| s.color(t.text_muted()))
            }),
            gradient.clone().into_any(),
        ))
        .style(|s| s.gap(4.0).min_width(0.0))
        .into_any(),
        Brush::Image(image_brush) => {
            let source_label = match &image_brush.image {
                imaging::Image::Raster(image) => format!("raster {}×{}", image.width, image.height),
                imaging::Image::Scene(scene) => {
                    format!(
                        "scene image #{}  {}×{}",
                        scene.id(),
                        scene.width(),
                        scene.height()
                    )
                }
                imaging::Image::External(image) => {
                    format!(
                        "external image #{}  {}×{}",
                        image.id.0, image.width, image.height
                    )
                }
            };
            let mut rows = vec![
                Stack::horizontal((
                    Label::new("image brush").style(|s| {
                        s.font_size(10.0)
                            .font_bold()
                            .with_theme(|s, t| s.color(t.text_muted()))
                    }),
                    detail_label(source_label),
                ))
                .style(|s| s.items_center().gap(8.0))
                .into_any(),
                Stack::horizontal((
                    metric_chip("quality", format!("{:?}", image_brush.sampler.quality)),
                    metric_chip(
                        "extend",
                        format!(
                            "{:?}/{:?}",
                            image_brush.sampler.x_extend, image_brush.sampler.y_extend
                        ),
                    ),
                    metric_chip("alpha", format!("{:.3}", image_brush.sampler.alpha)),
                ))
                .style(|s| s.items_center().gap(10.0).min_width(0.0).text_wrap())
                .into_any(),
            ];
            match &image_brush.image {
                imaging::Image::Scene(scene_image) => {
                    rows.push(nested_scene_section(
                        "Scene Image",
                        format!("picture #{}", scene_image.picture().id()),
                        scene_image.scene().clone(),
                        Size::new(scene_image.width() as f64, scene_image.height() as f64),
                    ));
                    rows.push(raw_debug_section(
                        "Image Metadata",
                        format!("{:?}", scene_image),
                    ));
                }
                imaging::Image::Raster(image) => {
                    rows.push(raw_debug_section("Image Metadata", format!("{:?}", image)));
                }
                imaging::Image::External(image) => {
                    rows.push(raw_debug_section("Image Metadata", format!("{:?}", image)));
                }
            }
            stack_rows(rows)
        }
    }
}

fn default_composite(composite: Composite) -> bool {
    composite == Composite::default()
}

impl IntoView for Composite {
    type V = AnyView;
    type Intermediate = AnyView;

    fn into_intermediate(self) -> Self::Intermediate {
        stack_rows(vec![
            field_row("blend", format!("{:?}", self.blend)),
            field_row("alpha", format!("{:.3}", self.alpha)),
        ])
    }
}

impl IntoView for Filter {
    type V = AnyView;
    type Intermediate = AnyView;

    fn into_intermediate(self) -> Self::Intermediate {
        let rows = match self {
            Filter::Flood { color } => {
                vec![field_row("kind", "flood"), field_value("color", &color)]
            }
            Filter::Blur {
                std_deviation_x,
                std_deviation_y,
            } => vec![
                field_row("kind", "blur"),
                field_row("sigma x", format!("{std_deviation_x:.2}")),
                field_row("sigma y", format!("{std_deviation_y:.2}")),
            ],
            Filter::DropShadow {
                dx,
                dy,
                std_deviation_x,
                std_deviation_y,
                color,
            } => vec![
                field_row("kind", "drop shadow"),
                field_row("offset", format_point(dx as f64, dy as f64)),
                field_row("sigma x", format!("{std_deviation_x:.2}")),
                field_row("sigma y", format!("{std_deviation_y:.2}")),
                field_value("color", &color),
            ],
            Filter::Offset { dx, dy } => vec![
                field_row("kind", "offset"),
                field_row("delta", format_point(dx as f64, dy as f64)),
            ],
        };
        stack_rows(rows)
    }
}

impl IntoView for Geometry {
    type V = AnyView;
    type Intermediate = AnyView;

    fn into_intermediate(self) -> Self::Intermediate {
        let mut rows = match &self {
            Geometry::Rect(rect) => vec![
                field_row("kind", "rect"),
                field_view("bounds", bounds_view(*rect)),
                field_row("elements", "1"),
            ],
            Geometry::RoundedRect(rect) => vec![
                field_row("kind", "rounded rect"),
                field_view("bounds", bounds_view(rect.bounding_box())),
                field_row("radii", format!("{:?}", rect.radii())),
                field_row("elements", "1"),
            ],
            Geometry::Path(path) => vec![
                field_row("kind", "path"),
                field_view("bounds", bounds_view(path.bounding_box())),
                field_row("elements", path.elements().len().to_string()),
            ],
        };
        rows.push(raw_debug_section("Shape Detail", format!("{self:?}")));
        stack_rows(rows)
    }
}

impl IntoView for AppliedMask {
    type V = AnyView;
    type Intermediate = AnyView;

    fn into_intermediate(self) -> Self::Intermediate {
        let mut rows = vec![field_row("mask", format!("{:?}", self.mask))];
        if self.transform != Affine::IDENTITY {
            rows.push(field_row("transform", format_affine(self.transform)));
        }
        stack_rows(rows)
    }
}

impl IntoView for Mask {
    type V = AnyView;
    type Intermediate = AnyView;

    fn into_intermediate(self) -> Self::Intermediate {
        let mut rows = vec![
            field_row("mode", format!("{:?}", self.mode)),
            field_row("scene commands", self.scene.commands().len().to_string()),
        ];
        rows.push(nested_scene_section(
            "Mask Scene",
            format!("{} commands", self.scene.commands().len()),
            self.scene.clone(),
            Size::new(180.0, 100.0),
        ));
        stack_rows(rows)
    }
}

impl IntoView for Context {
    type V = AnyView;
    type Intermediate = AnyView;

    fn into_intermediate(self) -> Self::Intermediate {
        let mut rows = vec![field_row("label", format!("{:?}", self.label))];
        if let Some(source) = self.source {
            rows.push(field_row(
                "source",
                format!("{:?}:{}:{}", source.file, source.line, source.column),
            ));
        }
        stack_rows(rows)
    }
}

impl IntoView for Clip {
    type V = AnyView;
    type Intermediate = AnyView;

    fn into_intermediate(self) -> Self::Intermediate {
        match self {
            Clip::Fill {
                transform,
                shape,
                fill_rule,
            } => {
                let mut rows = vec![
                    field_row("kind", "fill clip"),
                    field_row("fill rule", format!("{fill_rule:?}")),
                    field_view("shape", shape.into_any()),
                ];
                if transform != Affine::IDENTITY {
                    rows.insert(1, field_value("transform", &transform));
                }
                stack_rows(rows)
            }
            Clip::Stroke {
                transform,
                shape,
                stroke,
            } => {
                let mut rows = vec![
                    field_row("kind", "stroke clip"),
                    field_value("stroke", &stroke),
                    field_view("shape", shape.into_any()),
                ];
                if transform != Affine::IDENTITY {
                    rows.insert(1, field_value("transform", &transform));
                }
                stack_rows(rows)
            }
        }
    }
}

impl IntoView for Group {
    type V = AnyView;
    type Intermediate = AnyView;

    fn into_intermediate(self) -> Self::Intermediate {
        let mut rows = Vec::new();
        rows.push(field_row("clip", self.clip.is_some().to_string()));
        rows.push(field_row("mask", self.mask.is_some().to_string()));
        rows.push(field_row("filters", self.filters.len().to_string()));
        if !default_composite(self.composite) {
            rows.push(section("Composite", self.composite.into_any()));
        } else {
            rows.push(field_row("composite", "default"));
        }
        if let Some(clip) = self.clip {
            rows.push(section("Group Clip", clip.into_any()));
        }
        if let Some(mask) = self.mask {
            rows.push(section("Applied Mask", mask.into_any()));
        }
        if !self.filters.is_empty() {
            rows.push(section(
                "Filters",
                Stack::vertical_from_iter(self.filters.into_iter().map(|filter| {
                    filter.into_any().style(|s| {
                        s.padding(6.0)
                            .width_full()
                            .with_theme(|s, t| s.background(t.bg_base()))
                    })
                }))
                .style(|s| s.width_full().gap(4.0))
                .into_any(),
            ));
        }
        stack_rows(rows)
    }
}

impl IntoView for Draw {
    type V = AnyView;
    type Intermediate = AnyView;

    fn into_intermediate(self) -> Self::Intermediate {
        match self {
            Draw::Fill {
                transform,
                fill_rule,
                brush,
                brush_transform,
                shape,
                composite,
            } => {
                let mut rows = vec![
                    field_row("kind", "fill"),
                    field_row("fill rule", format!("{fill_rule:?}")),
                    field_view("brush", brush_summary_view(&brush)),
                    field_view("shape", shape.into_any()),
                ];
                if transform != Affine::IDENTITY {
                    rows.insert(1, field_value("transform", &transform));
                }
                if let Some(brush_transform) = brush_transform {
                    rows.push(field_value("brush transform", &brush_transform));
                }
                if !default_composite(composite) {
                    rows.push(section("Composite", composite.into_any()));
                }
                stack_rows(rows)
            }
            Draw::Stroke {
                transform,
                stroke,
                brush,
                brush_transform,
                shape,
                composite,
            } => {
                let mut rows = vec![
                    field_row("kind", "stroke"),
                    field_value("stroke", &stroke),
                    field_view("brush", brush_summary_view(&brush)),
                    field_view("shape", shape.into_any()),
                ];
                if transform != Affine::IDENTITY {
                    rows.insert(1, field_value("transform", &transform));
                }
                if let Some(brush_transform) = brush_transform {
                    rows.push(field_value("brush transform", &brush_transform));
                }
                if !default_composite(composite) {
                    rows.push(section("Composite", composite.into_any()));
                }
                stack_rows(rows)
            }
            Draw::GlyphRun(run) => {
                let mut rows = vec![
                    field_row("kind", "glyph run"),
                    field_row("font size", format!("{:.2}", run.font_size)),
                    field_row("glyphs", run.glyphs.len().to_string()),
                    field_row("hint", run.hint.to_string()),
                    field_row("style", format!("{:?}", run.style)),
                    field_view("brush", brush_summary_view(&run.brush)),
                ];
                if run.transform != Affine::IDENTITY {
                    rows.insert(1, field_value("transform", &run.transform));
                }
                if let Some(glyph_transform) = run.glyph_transform {
                    rows.push(field_value("glyph transform", &glyph_transform));
                }
                if !run.normalized_coords.is_empty() {
                    rows.push(field_row(
                        "variations",
                        run.normalized_coords.len().to_string(),
                    ));
                }
                if !default_composite(run.composite) {
                    rows.push(section("Composite", run.composite.into_any()));
                }
                stack_rows(rows)
            }
            Draw::BlurredRoundedRect(draw) => {
                let mut rows = vec![
                    field_row("kind", "blurred rounded rect"),
                    field_value("rect", &draw.rect),
                    field_row("radius", format_float(draw.radius)),
                    field_row("std dev", format_float(draw.std_dev)),
                    field_value("color", &draw.color),
                ];
                if draw.transform != Affine::IDENTITY {
                    rows.insert(1, field_value("transform", &draw.transform));
                }
                if !default_composite(draw.composite) {
                    rows.push(section("Composite", draw.composite.into_any()));
                }
                stack_rows(rows)
            }
            Draw::ScenePicture { transform, picture } => {
                let mut rows = vec![
                    field_row("kind", "scene picture"),
                    field_row("picture id", picture.id().to_string()),
                    field_view("bounds", bounds_view(picture.bounds())),
                    field_row(
                        "scene commands",
                        picture.scene().commands().len().to_string(),
                    ),
                ];
                if transform != Affine::IDENTITY {
                    rows.insert(1, field_value("transform", &transform));
                }
                rows.push(nested_scene_section(
                    "Picture Scene",
                    format!("{} commands", picture.scene().commands().len()),
                    picture.scene().clone(),
                    picture.bounds().size(),
                ));
                stack_rows(rows)
            }
        }
    }
}

impl IntoView for Command {
    type V = AnyView;
    type Intermediate = AnyView;

    fn into_intermediate(self) -> Self::Intermediate {
        let rows = match self {
            Command::PushContext(id) => vec![
                field_row("command", "push context"),
                field_row("context", format!("{id:?}")),
            ],
            Command::PopContext => vec![field_row("command", "pop context")],
            Command::PushClip(id) => vec![
                field_row("command", "push clip"),
                field_row("clip", format!("{id:?}")),
            ],
            Command::PopClip => vec![field_row("command", "pop clip")],
            Command::PushGroup(id) => vec![
                field_row("command", "push group"),
                field_row("group", format!("{id:?}")),
            ],
            Command::PopGroup => vec![field_row("command", "pop group")],
            Command::Draw(id) => vec![
                field_row("command", "draw"),
                field_row("draw", format!("{id:?}")),
            ],
        };
        stack_rows(rows)
    }
}

impl IntoView for ResolvedCommand {
    type V = AnyView;
    type Intermediate = AnyView;

    fn into_intermediate(self) -> Self::Intermediate {
        match self {
            Self::PushContext(context) => command_card("Push Context", context.into_any()),
            Self::PopContext => command_card("Pop Context", field_row("command", "pop context")),
            Self::PushClip(clip) => command_card("Push Clip", clip.into_any()),
            Self::PopClip => command_card("Pop Clip", field_row("command", "pop clip")),
            Self::PushGroup(group) => command_card("Push Group", group.into_any()),
            Self::PopGroup => command_card("Pop Group", field_row("command", "pop group")),
            Self::Draw(draw) => command_card("Draw", draw.into_any()),
        }
    }
}

fn command_card(title: impl Into<String>, body: AnyView) -> AnyView {
    Stack::vertical((
        Label::new(title.into()).style(|s| s.font_bold().font_size(12.5)),
        body.style(|s| s.padding_left(2.0)),
    ))
    .style(|s| {
        s.width_full()
            .gap(8.0)
            .padding(10.0)
            .min_width(0.0)
            .border_radius(10.0)
            .border(1.0)
            .with_theme(|s, t| s.background(t.bg_elevated()).border_color(t.border()))
    })
    .into_any()
}

fn resolve_command(scene: &Scene, command: &Command) -> ResolvedCommand {
    match command {
        Command::PushContext(id) => ResolvedCommand::PushContext(scene.context(*id).clone()),
        Command::PopContext => ResolvedCommand::PopContext,
        Command::PushClip(id) => ResolvedCommand::PushClip(scene.clip(*id).clone()),
        Command::PopClip => ResolvedCommand::PopClip,
        Command::PushGroup(id) => ResolvedCommand::PushGroup(scene.group(*id).clone()),
        Command::PopGroup => ResolvedCommand::PopGroup,
        Command::Draw(id) => ResolvedCommand::Draw(scene.draw_op(*id).clone()),
    }
}

fn command_rows(scene: &Scene) -> Vec<(usize, ResolvedCommand)> {
    let mut rows = Vec::new();
    let mut depth = 0usize;
    for command in scene.commands() {
        match command {
            Command::PopContext | Command::PopClip | Command::PopGroup => {
                depth = depth.saturating_sub(1);
                rows.push((depth, resolve_command(scene, command)));
            }
            Command::PushContext(_) | Command::PushClip(_) | Command::PushGroup(_) => {
                rows.push((depth, resolve_command(scene, command)));
                depth += 1;
            }
            Command::Draw(_) => {
                rows.push((depth, resolve_command(scene, command)));
            }
        }
    }
    rows
}

pub(crate) fn scene_debug_view_with_size(
    scene: Scene,
    content_size: peniko::kurbo::Size,
) -> AnyView {
    let preview_scene = scene.clone();
    let rows = command_rows(&scene);
    let preview_width = content_size.width.max(1.0);
    let preview_height = content_size.height.max(1.0);

    let preview = canvas(move |cx, _size| {
        replay(&preview_scene, cx.painter.as_imaging_painter().sink_mut());
    })
    .style(move |s| s.width(preview_width).height(preview_height));

    let preview = preview.container().style(|s| {
        s.padding(16.0)
            .background(css::WHITE)
            .border(1.0)
            .border_color(Color::from_rgba8(0, 0, 0, 24))
            .border_radius(10.0)
            .apply_box_shadows(vec![
                BoxShadow::new()
                    .color(css::BLACK.multiply_alpha(0.3))
                    .top_offset(-13.)
                    .bottom_offset(0.4)
                    .right_offset(-4.)
                    .left_offset(-4.)
                    .blur_radius(2.)
                    .spread(1.),
                BoxShadow::new()
                    .color(css::BLACK.multiply_alpha(0.3))
                    .top_offset(-15.)
                    .bottom_offset(4.)
                    .right_offset(-6.)
                    .left_offset(-6.)
                    .blur_radius(5.)
                    .spread(6.),
            ])
    });

    let tree = if rows.is_empty() {
        Label::new("No retained commands")
            .style(|s| s.padding(8.0))
            .into_any()
    } else {
        Stack::vertical_from_iter(rows.into_iter().enumerate().map(|(idx, (depth, command))| {
            command.into_any().style(move |s| {
                s.width_full()
                    .padding_left(8.0 + depth as f64 * 16.0)
                    .with_theme(move |s, t| {
                        s.apply_if(idx.is_multiple_of(2), |s| s.background(t.bg_base()))
                            .apply_if(!idx.is_multiple_of(2), |s| s.background(t.bg_elevated()))
                    })
            })
        }))
        .style(|s| s.width_full().gap(4.0))
        .into_any()
    };

    let command_tree = tree
        .scroll()
        .style(|s| s.width_full().height_full().min_height(0.0))
        .into_any();

    let commands = Stack::vertical((
        Label::new("Commands").style(|s| s.font_bold().padding_horiz(4.0)),
        command_tree,
    ))
    .style(|s| s.width_full().height_full().min_height(0.0).gap(4.0));

    Stack::vertical((
        Label::new("Preview").style(|s| s.font_bold().padding_horiz(4.0)),
        preview,
        commands,
    ))
    .style(|s| s.width_full().min_size(0.0, 0.0).gap(8.0))
    .into_any()
}

impl StylePropValue for Scene {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        Some(scene_debug_view_with_size(
            self.clone(),
            peniko::kurbo::Size::new(220.0, 120.0),
        ))
    }
}
