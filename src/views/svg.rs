use std::{cell::RefCell, rc::Rc};

use floem_reactive::Effect;
use imaging::MaskMode;
use peniko::{
    Brush, GradientKind, LinearGradientPosition,
    kurbo::{Affine, Point, Size},
};
use rustc_hash::FxHashMap;
use svg_imaging::{ParseOptions, RenderOptions, SvgDocument};

use crate::{
    prop, prop_extractor,
    style::{Style, TextColor},
    style_class,
    view::ViewId,
    view::{LayoutNodeCx, MeasureFn, View},
};

use super::Decorators;

prop!(pub SvgColor: Option<Brush> {} = None);

prop_extractor! {
    SvgStyle {
        svg_color: SvgColor,
        text_color: TextColor,
    }
}

#[derive(Clone)]
struct SvgLayoutData {
    natural_width: f32,
    natural_height: f32,
}

impl SvgLayoutData {
    fn new() -> Self {
        Self {
            natural_width: 0.0,
            natural_height: 0.0,
        }
    }

    fn set_size(&mut self, width: f32, height: f32) {
        self.natural_width = width;
        self.natural_height = height;
    }

    fn aspect_ratio(&self) -> f32 {
        if self.natural_height == 0.0 {
            1.0
        } else {
            self.natural_width / self.natural_height
        }
    }

    fn create_taffy_layout_fn(layout_data: Rc<RefCell<Self>>) -> Box<MeasureFn> {
        Box::new(
            move |known_dimensions, available_space, _node_id, style, _measure_ctx| {
                use taffy::*;

                let data = layout_data.borrow();
                let natural_width = data.natural_width;
                let natural_height = data.natural_height;
                let natural_aspect_ratio = data.aspect_ratio();
                let explicit_aspect_ratio = style
                    .aspect_ratio
                    .filter(|ratio| ratio.is_finite() && *ratio > 0.0)
                    .unwrap_or(natural_aspect_ratio);

                if let (Some(width), Some(height)) =
                    (known_dimensions.width, known_dimensions.height)
                {
                    return Size { width, height };
                }

                if let Some(width) = known_dimensions.width {
                    let height = known_dimensions
                        .height
                        .unwrap_or_else(|| width / explicit_aspect_ratio);
                    return Size { width, height };
                }

                if let Some(height) = known_dimensions.height {
                    let width = if explicit_aspect_ratio == 0.0 {
                        0.0
                    } else {
                        height * explicit_aspect_ratio
                    };
                    return Size { width, height };
                }

                if natural_width > 0.0 && natural_height > 0.0 {
                    return Size {
                        width: natural_width,
                        height: natural_height,
                    };
                }

                match (available_space.width, available_space.height) {
                    (AvailableSpace::Definite(width), _) => Size {
                        width: if natural_width == 0.0 {
                            width
                        } else {
                            natural_width
                        },
                        height: if natural_width > 0.0 && explicit_aspect_ratio > 0.0 {
                            natural_width / explicit_aspect_ratio
                        } else {
                            0.0
                        },
                    },
                    (_, AvailableSpace::Definite(height)) => Size {
                        width: if natural_height > 0.0 && explicit_aspect_ratio > 0.0 {
                            height * explicit_aspect_ratio
                        } else {
                            0.0
                        },
                        height: if natural_height == 0.0 {
                            height
                        } else {
                            natural_height
                        },
                    },
                    _ => Size {
                        width: natural_width,
                        height: natural_height,
                    },
                }
            },
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct SvgRetainedCacheKey {
    svg: String,
    css: Option<String>,
    brush: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct SvgRetainedCacheKeyBase {
    svg: String,
    css: Option<String>,
}

thread_local! {
    static SVG_RETAINED_CACHE: RefCell<FxHashMap<SvgRetainedCacheKey, imaging::record::Retained>> =
        RefCell::new(FxHashMap::default());
}

fn cached_retained_draw(
    key: &SvgRetainedCacheKey,
    document: &SvgDocument,
    brush: Option<&Brush>,
) -> imaging::record::Retained {
    SVG_RETAINED_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(retained) = cache.get(key) {
            return retained.clone();
        }

        let bounds = document.size().to_rect();
        let retained = imaging::Retained::record(|p| {
            if let Some(brush) = brush {
                p.with_masked_group(
                    MaskMode::Alpha,
                    |mask| {
                        let _ = document.render(mask, &RenderOptions::default());
                    },
                    |painter| {
                        painter.fill(bounds, brush).draw();
                    },
                );
            } else {
                let _ = document.render(p, &RenderOptions::default());
            }
        })
        .with_bounds(bounds)
        .with_cache_policy(imaging::RetainedCachePolicy {
            image_transform: Some(imaging::RetainedTransformPolicy::Linear),
            eviction: imaging::RetainedEvictionPolicy::UntilUnused,
        });
        cache.insert(key.clone(), retained.clone());
        retained
    })
}

pub struct Svg {
    id: ViewId,
    svg_document: Option<SvgDocument>,
    retained_draw: Option<imaging::record::Retained>,
    retained_draw_brush_key: Option<String>,
    retained_cache_key_base: SvgRetainedCacheKeyBase,
    svg_style: SvgStyle,
    svg_string: String,
    svg_css: Option<String>,
    css_prop: Option<Box<dyn SvgCssPropExtractor>>,
    aspect_ratio: f32,
    layout_data: Rc<RefCell<SvgLayoutData>>,
}

style_class!(pub SvgClass);

pub struct SvgStrFn {
    str_fn: Box<dyn Fn() -> String>,
}

impl<T, F> From<F> for SvgStrFn
where
    F: Fn() -> T + 'static,
    T: Into<String>,
{
    fn from(value: F) -> Self {
        SvgStrFn {
            str_fn: Box::new(move || value().into()),
        }
    }
}

impl From<String> for SvgStrFn {
    fn from(value: String) -> Self {
        SvgStrFn {
            str_fn: Box::new(move || value.clone()),
        }
    }
}

impl From<&str> for SvgStrFn {
    fn from(value: &str) -> Self {
        let value = value.to_string();
        SvgStrFn {
            str_fn: Box::new(move || value.clone()),
        }
    }
}

pub trait SvgCssPropExtractor {
    fn read_custom(&mut self, cx: &mut crate::context::StyleCx) -> bool;
    fn css_string(&self) -> String;
}

#[derive(Debug, Clone)]
pub enum SvgOrStyle {
    Svg(String),
    Style(String),
}

impl Svg {
    pub fn update_value<S: Into<String>>(self, svg_str: impl Fn() -> S + 'static) -> Self {
        let id = self.id;
        Effect::new(move |_| {
            let new_svg_str = svg_str();
            id.update_state(SvgOrStyle::Svg(new_svg_str.into()));
        });
        self
    }

    pub fn set_css_extractor(mut self, css: impl SvgCssPropExtractor + 'static) -> Self {
        self.css_prop = Some(Box::new(css));
        self
    }
}

pub fn svg(svg_str_fn: impl Into<SvgStrFn> + 'static) -> Svg {
    let id = ViewId::new();
    let svg_str_fn: SvgStrFn = svg_str_fn.into();
    Effect::new(move |_| {
        let new_svg_str = (svg_str_fn.str_fn)();
        id.update_state(SvgOrStyle::Svg(new_svg_str));
    });
    let layout_data = Rc::new(RefCell::new(SvgLayoutData::new()));
    let mut svg = Svg {
        id,
        svg_document: None,
        retained_draw: None,
        retained_draw_brush_key: None,
        retained_cache_key_base: SvgRetainedCacheKeyBase {
            svg: String::new(),
            css: None,
        },
        svg_style: Default::default(),
        svg_string: Default::default(),
        css_prop: None,
        svg_css: None,
        aspect_ratio: 1.,
        layout_data,
    };
    svg.set_taffy_layout();
    svg.class(SvgClass)
}

impl Svg {
    fn set_taffy_layout(&mut self) {
        let taffy_node = self.id.taffy_node();
        let taffy = self.id.taffy();
        let layout_fn = SvgLayoutData::create_taffy_layout_fn(self.layout_data.clone());
        let _ = taffy.borrow_mut().set_node_context(
            taffy_node,
            Some(LayoutNodeCx::Custom {
                measure: layout_fn,
                finalize: None,
            }),
        );
    }
}

impl View for Svg {
    fn id(&self) -> ViewId {
        self.id
    }

    fn view_style(&self) -> Option<crate::style::Style> {
        if !self.aspect_ratio.is_nan() {
            Some(Style::new().aspect_ratio(self.aspect_ratio))
        } else {
            None
        }
    }

    fn style_pass(&mut self, cx: &mut crate::context::StyleCx<'_>) {
        let style = cx.style();
        let previous_brush = self
            .svg_style
            .color_brush()
            .map(|brush| brush_to_css_string(&brush));
        self.svg_style.read_style(cx, &style);
        let current_brush = self
            .svg_style
            .color_brush()
            .map(|brush| brush_to_css_string(&brush));
        if previous_brush != current_brush {
            self.retained_draw = None;
            self.retained_draw_brush_key = None;
            self.id.request_paint();
        }
        if let Some(document) = &self.svg_document {
            let size = document.size();
            let aspect_ratio = (size.width / size.height) as f32;
            if self.aspect_ratio != aspect_ratio {
                self.aspect_ratio = aspect_ratio;
            }
        }
        if let Some(prop_reader) = &mut self.css_prop
            && prop_reader.read_custom(cx)
        {
            self.id
                .update_state(SvgOrStyle::Style(prop_reader.css_string()));
        }
    }

    fn update(&mut self, _cx: &mut crate::context::UpdateCx, state: Box<dyn std::any::Any>) {
        if let Ok(state) = state.downcast::<SvgOrStyle>() {
            let (text, style) = match *state {
                SvgOrStyle::Svg(text) => (text, self.svg_css.clone()),
                SvgOrStyle::Style(css) => (self.svg_string.clone(), Some(css)),
            };

            if text == self.svg_string && style == self.svg_css {
                return;
            }

            self.svg_string = text.clone();
            self.svg_css = style.clone();
            self.retained_cache_key_base = SvgRetainedCacheKeyBase {
                svg: text.clone(),
                css: style.clone(),
            };
            self.retained_draw = None;
            self.retained_draw_brush_key = None;

            let svg_document = SvgDocument::from_str(
                text.as_str(),
                &ParseOptions {
                    style_sheet: style,
                    ..Default::default()
                },
            )
            .ok();
            {
                let mut layout_data = self.layout_data.borrow_mut();
                if let Some(document) = svg_document.as_ref() {
                    let size = document.size();
                    layout_data.set_size(size.width as f32, size.height as f32);
                } else {
                    layout_data.set_size(0.0, 0.0);
                }
            }
            self.aspect_ratio = svg_document.as_ref().map_or(f32::NAN, |document| {
                let size = document.size();
                let width = size.width as f32;
                let height = size.height as f32;
                if height == 0.0 {
                    f32::NAN
                } else {
                    width / height
                }
            });
            self.svg_document = svg_document;

            self.id.request_layout();
            self.id.request_paint();
        }
    }

    fn paint(&mut self, cx: &mut crate::context::PaintCx) {
        if let Some(document) = self.svg_document.as_ref() {
            let size = document.size();
            if size.width <= 0.0 || size.height <= 0.0 {
                return;
            }

            let layout = self.id.get_layout().unwrap_or_default();
            let rect = Size::new(layout.size.width as f64, layout.size.height as f64).to_rect();
            let transform = Affine::translate((rect.x0, rect.y0))
                * Affine::scale_non_uniform(rect.width() / size.width, rect.height() / size.height);
            let brush = self.svg_style.color_brush();
            let brush_key = brush.as_ref().map(brush_to_css_string);
            if self.retained_draw.is_none() || self.retained_draw_brush_key != brush_key {
                self.retained_draw = Some(cached_retained_draw(
                    &SvgRetainedCacheKey {
                        svg: self.retained_cache_key_base.svg.clone(),
                        css: self.retained_cache_key_base.css.clone(),
                        brush: brush_key.clone(),
                    },
                    document,
                    brush.as_ref(),
                ));
                self.retained_draw_brush_key = brush_key;
            }

            let Some(retained_draw) = self.retained_draw.as_ref() else {
                return;
            };
            cx.painter.draw_retained(
                retained_draw.as_ref(),
                transform,
                imaging::Composite::default(),
            );
        }
    }
}

impl SvgStyle {
    fn color_brush(&self) -> Option<Brush> {
        self.svg_color()
            .or_else(|| self.text_color().map(Brush::Solid))
    }
}

pub fn brush_to_css_string(brush: &Brush) -> String {
    match brush {
        Brush::Solid(color) => {
            let r = (color.components[0] * 255.0).round() as u8;
            let g = (color.components[1] * 255.0).round() as u8;
            let b = (color.components[2] * 255.0).round() as u8;
            let a = color.components[3];

            if a < 1.0 {
                format!("rgba({r}, {g}, {b}, {a})")
            } else {
                format!("#{r:02x}{g:02x}{b:02x}")
            }
        }
        Brush::Gradient(gradient) => {
            match &gradient.kind {
                GradientKind::Linear(LinearGradientPosition { start, end }) => {
                    let angle_degrees = calculate_angle(start, end);

                    let mut css = format!("linear-gradient({angle_degrees}deg, ");

                    for (i, stop) in gradient.stops.iter().enumerate() {
                        let color = &stop.color;
                        let r = (color.components[0] * 255.0).round() as u8;
                        let g = (color.components[1] * 255.0).round() as u8;
                        let b = (color.components[2] * 255.0).round() as u8;
                        let a = color.components[3];

                        let color_str = if a < 1.0 {
                            format!("rgba({r}, {g}, {b}, {a})")
                        } else {
                            format!("#{r:02x}{g:02x}{b:02x}")
                        };

                        css.push_str(&format!("{} {}%", color_str, (stop.offset * 100.0).round()));

                        if i < gradient.stops.len() - 1 {
                            css.push_str(", ");
                        }
                    }

                    css.push(')');
                    css
                }

                _ => "currentColor".to_string(), // Fallback for unsupported gradient types
            }
        }
        Brush::Image(_) => "currentColor".to_string(),
    }
}

fn calculate_angle(start: &Point, end: &Point) -> f64 {
    let angle_rad = (end.y - start.y).atan2(end.x - start.x);

    // CSS angles are measured clockwise from the positive y-axis
    let mut angle_deg = 90.0 - angle_rad.to_degrees();

    // Normalize to 0-360 range
    if angle_deg < 0.0 {
        angle_deg += 360.0;
    }

    angle_deg
}
