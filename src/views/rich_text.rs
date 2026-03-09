use std::{any::Any, cell::RefCell, rc::Rc};

use floem_reactive::Effect;
use floem_renderer::{
    Renderer,
    text::{Attrs, AttrsList, AttrsOwned},
};
use peniko::{Color, color::palette};
use smallvec::{SmallVec, smallvec};
use taffy::tree::NodeId;

use crate::{IntoView, context::UpdateCx, view::LayoutNodeCx, view::View, view::ViewId};

use crate::text::TextLayoutData;

pub struct RichText {
    id: ViewId,
    /// Layout data containing text layouts and overflow handling logic
    layout_data: Rc<RefCell<TextLayoutData>>,
    text_node: Option<NodeId>,
    layout_node: Option<NodeId>,
}

pub fn rich_text(
    text: String,
    attrs_list: AttrsList,
    text_fn: impl Fn() -> (String, AttrsList) + 'static,
) -> RichText {
    let id = ViewId::new();
    Effect::new(move |_| {
        let (new_text, new_attrs) = text_fn();
        id.update_state((new_text, new_attrs));
    });

    let layout_data = Rc::new(RefCell::new(TextLayoutData::new(Some(id))));

    // Initialize the layout data with the text and attrs
    {
        let mut data = layout_data.borrow_mut();
        data.set_text(&text, attrs_list, None);
        data.set_text_overflow(crate::style::TextOverflow::Wrap {
            overflow_wrap: crate::text::OverflowWrap::Normal,
            word_break: crate::text::WordBreakStrength::Normal,
        });
    }

    let mut rich_text = RichText {
        id,
        layout_data,
        text_node: None,
        layout_node: None,
    };

    rich_text.set_taffy_layout();
    rich_text
}

impl RichText {
    fn set_taffy_layout(&mut self) {
        let taffy_node = self.id.taffy_node();
        let taffy = self.id.taffy();
        let mut taffy = taffy.borrow_mut();
        let text_node = taffy
            .new_leaf(taffy::Style {
                ..taffy::Style::DEFAULT
            })
            .unwrap();

        let layout_fn = TextLayoutData::create_taffy_layout_fn(self.layout_data.clone());
        let finalize_fn = TextLayoutData::create_finalize_fn(self.layout_data.clone());
        self.text_node = Some(text_node);
        self.layout_node = Some(taffy_node);

        taffy
            .set_node_context(
                text_node,
                Some(LayoutNodeCx::Custom {
                    measure: layout_fn,
                    finalize: Some(finalize_fn),
                }),
            )
            .unwrap();
        taffy.set_children(taffy_node, &[text_node]).unwrap();
    }
}

impl View for RichText {
    fn id(&self) -> ViewId {
        self.id
    }

    fn debug_name(&self) -> std::borrow::Cow<'static, str> {
        self.layout_data
            .borrow()
            .get_text_layout()
            .map(|_| {
                let data = self.layout_data.borrow();
                format!(
                    "RichText: {:?}",
                    crate::text::paragraph_ranges(data.text().unwrap_or_default())
                        .map(|r| data.text().unwrap_or_default()[r].to_string())
                        .collect::<Vec<_>>()
                )
            })
            .unwrap_or_else(|| "RichText: <empty>".to_string())
            .into()
    }

    fn update(&mut self, _cx: &mut UpdateCx, state: Box<dyn Any>) {
        if let Ok(state) = state.downcast::<(String, AttrsList)>() {
            let (text, attrs_list) = *state;

            let mut data = self.layout_data.borrow_mut();
            data.set_text(&text, attrs_list, None);
            data.set_text_overflow(crate::style::TextOverflow::Wrap {
                overflow_wrap: crate::text::OverflowWrap::Normal,
                word_break: crate::text::WordBreakStrength::Normal,
            });
            drop(data);

            self.id.request_layout();
        }
    }

    fn paint(&mut self, cx: &mut crate::context::PaintCx) {
        let (Some(parent_node), Some(text_node)) = (self.layout_node, self.text_node) else {
            return;
        };

        let text_loc = self
            .id
            .get_content_rect_relative(text_node, parent_node)
            .unwrap_or_default()
            .origin();

        self.layout_data
            .borrow()
            .with_effective_text_layout(|layout| {
                cx.draw_text_lines(layout.layout_lines(text_loc));
            });
    }
}

#[derive(Clone, Debug)]
pub struct RichSpan<'a> {
    text: &'a str,
    attrs: Attrs<'a>,
}
#[allow(clippy::wrong_self_convention)]
impl<'a> RichSpan<'a> {
    fn to_owned(self) -> RichSpanOwned {
        let len = self.text.len();
        RichSpanOwned {
            text: self.text.to_string(),
            spans: smallvec::smallvec![(0..len, AttrsOwned::new(self.attrs))],
        }
    }
    pub fn color(mut self, color: Color) -> Self {
        self.attrs = self.attrs.color(color);
        self
    }

    pub fn family(mut self, family: &'a [floem_renderer::text::FamilyOwned]) -> RichSpan<'a> {
        self.attrs = self.attrs.family(family);
        self
    }

    pub fn font_width(mut self, stretch: floem_renderer::text::FontWidth) -> RichSpan<'a> {
        self.attrs = self.attrs.font_width(stretch);
        self
    }

    pub fn text_style(mut self, style: floem_renderer::text::FontStyle) -> RichSpan<'a> {
        self.attrs = self.attrs.font_style(style);
        self
    }

    pub fn weight(mut self, weight: floem_renderer::text::FontWeight) -> RichSpan<'a> {
        self.attrs = self.attrs.weight(weight);
        self
    }

    pub fn line_height(
        mut self,
        line_height: floem_renderer::text::LineHeightValue,
    ) -> RichSpan<'a> {
        self.attrs = self.attrs.line_height(line_height);
        self
    }

    pub fn font_size(mut self, font_size: f32) -> RichSpan<'a> {
        self.attrs = self.attrs.font_size(font_size);
        self
    }

    pub fn raw_weight(mut self, weight: u16) -> RichSpan<'a> {
        self.attrs = self.attrs.raw_weight(weight);
        self
    }
}
#[derive(Clone, Debug)]
pub struct RichSpanOwned {
    text: String,
    spans: SmallVec<[(std::ops::Range<usize>, AttrsOwned); 3]>,
}
impl IntoView for RichSpanOwned {
    type V = RichText;
    type Intermediate = RichText;

    fn into_intermediate(self) -> Self::Intermediate {
        let mut attrs_list = AttrsList::new(Attrs::new().color(palette::css::BLACK));
        for span in self.spans.clone() {
            attrs_list.add_span(span.0, span.1.as_attrs());
        }

        let text = self.text.clone();
        let text_clone = self.text.clone();
        let spans = self.spans.clone();

        rich_text(text, attrs_list, move || {
            let mut attrs_list = AttrsList::new(Attrs::new().color(palette::css::BLACK));
            for span in spans.clone() {
                attrs_list.add_span(span.0, span.1.as_attrs());
            }
            (text_clone.clone(), attrs_list)
        })
    }
}
impl<'a> IntoView for RichSpan<'a> {
    type V = RichText;
    type Intermediate = RichText;

    fn into_intermediate(self) -> Self::Intermediate {
        self.to_owned().into_intermediate()
    }
}
impl<'a, S> std::ops::Add<S> for RichSpan<'a>
where
    RichSpan<'a>: From<S>,
{
    type Output = RichSpanOwned;

    fn add(self, rhs: S) -> Self::Output {
        let self_len = self.text.len();
        let rhs: RichSpan = rhs.into();
        let rhs_len = rhs.text.len();
        RichSpanOwned {
            text: self.text.to_string() + rhs.text,
            spans: smallvec![
                (0..self_len, AttrsOwned::new(self.attrs)),
                (self_len..self_len + rhs_len, AttrsOwned::new(rhs.attrs)),
            ],
        }
    }
}
impl<'a> std::ops::Add<&'a str> for RichSpan<'a> {
    type Output = RichSpanOwned;

    fn add(self, rhs: &'a str) -> Self::Output {
        let self_len = self.text.len();
        let rhs_len = rhs.len();
        RichSpanOwned {
            text: self.text.to_string() + rhs,
            spans: smallvec![
                (0..self_len, AttrsOwned::new(self.attrs)),
                (
                    self_len..self_len + rhs_len,
                    AttrsOwned::new(Attrs::new().color(palette::css::BLACK))
                ),
            ],
        }
    }
}
impl std::ops::Add<String> for RichSpan<'_> {
    type Output = RichSpanOwned;

    fn add(self, rhs: String) -> Self::Output {
        let self_len = self.text.len();
        let rhs_len = rhs.len();
        RichSpanOwned {
            text: self.text.to_string() + &rhs,
            spans: smallvec![
                (0..self_len, AttrsOwned::new(self.attrs)),
                (
                    self_len..self_len + rhs_len,
                    AttrsOwned::new(Attrs::new().color(palette::css::BLACK))
                ),
            ],
        }
    }
}
impl<'a, S> std::ops::Add<S> for RichSpanOwned
where
    RichSpan<'a>: From<S>,
{
    type Output = Self;

    fn add(mut self, rhs: S) -> Self::Output {
        let rhs: RichSpan = rhs.into();
        let self_len = self.text.len();
        let new_text = self.text + rhs.text;
        self.spans
            .push((self_len..new_text.len(), AttrsOwned::new(rhs.attrs)));
        Self {
            text: new_text,
            spans: self.spans,
        }
    }
}
impl std::ops::Add<&str> for RichSpanOwned {
    type Output = RichSpanOwned;

    fn add(mut self, rhs: &str) -> Self::Output {
        let self_len = self.text.len();
        let new_text = self.text + rhs;
        self.spans.push((
            self_len..new_text.len(),
            AttrsOwned::new(Attrs::new().color(palette::css::BLACK)),
        ));
        Self {
            text: new_text,
            spans: self.spans,
        }
    }
}
impl std::ops::Add<String> for RichSpanOwned {
    type Output = RichSpanOwned;

    fn add(mut self, rhs: String) -> Self::Output {
        let self_len = self.text.len();
        let new_text = self.text + &rhs;
        self.spans.push((
            self_len..new_text.len(),
            AttrsOwned::new(Attrs::new().color(palette::css::BLACK)),
        ));
        Self {
            text: new_text,
            spans: self.spans,
        }
    }
}
impl std::ops::Add for RichSpanOwned {
    type Output = Self;

    fn add(mut self, rhs: Self) -> Self::Output {
        let self_len = self.text.len();
        self.spans.extend(
            rhs.spans
                .into_iter()
                .map(|span| ((span.0.start + self_len)..(span.0.end + self_len), span.1)),
        );
        Self {
            text: self.text + &rhs.text,
            spans: self.spans,
        }
    }
}

pub trait RichTextExt<'a>
where
    Self: Sized,
    RichSpan<'a>: From<Self>,
{
    fn color(self, color: Color) -> RichSpan<'a> {
        let span: RichSpan = self.into();
        span.color(color)
    }
    fn red(self) -> RichSpan<'a> {
        self.color(palette::css::RED)
    }
    fn blue(self) -> RichSpan<'a> {
        self.color(palette::css::BLUE)
    }

    fn green(self) -> RichSpan<'a> {
        self.color(palette::css::GREEN)
    }

    fn yellow(self) -> RichSpan<'a> {
        self.color(palette::css::YELLOW)
    }

    fn black(self) -> RichSpan<'a> {
        self.color(palette::css::BLACK)
    }

    fn white(self) -> RichSpan<'a> {
        self.color(palette::css::WHITE)
    }

    fn gray(self) -> RichSpan<'a> {
        self.color(palette::css::GRAY)
    }

    fn cyan(self) -> RichSpan<'a> {
        self.color(palette::css::CYAN)
    }

    fn magenta(self) -> RichSpan<'a> {
        self.color(palette::css::MAGENTA)
    }

    fn orange(self) -> RichSpan<'a> {
        self.color(palette::css::ORANGE)
    }

    fn purple(self) -> RichSpan<'a> {
        self.color(palette::css::PURPLE)
    }

    fn pink(self) -> RichSpan<'a> {
        self.color(palette::css::PINK)
    }

    fn family(self, family: &'a [crate::text::FamilyOwned]) -> RichSpan<'a> {
        let span: RichSpan = self.into();
        span.family(family)
    }
    fn stretch(self, stretch: crate::text::FontWidth) -> RichSpan<'a> {
        let span: RichSpan = self.into();
        span.font_width(stretch)
    }
    fn text_style(self, style: crate::text::FontStyle) -> RichSpan<'a> {
        let span: RichSpan = self.into();
        span.text_style(style)
    }
    fn italic(self) -> RichSpan<'a> {
        self.text_style(crate::text::FontStyle::Italic)
    }
    fn oblique(self) -> RichSpan<'a> {
        self.text_style(crate::text::FontStyle::Oblique(None))
    }

    fn weight(self, weight: crate::text::FontWeight) -> RichSpan<'a> {
        let span: RichSpan = self.into();
        span.weight(weight)
    }
    fn thin(self) -> RichSpan<'a> {
        self.weight(crate::text::FontWeight::THIN)
    }
    fn extra_light(self) -> RichSpan<'a> {
        self.weight(crate::text::FontWeight::EXTRA_LIGHT)
    }
    fn light(self) -> RichSpan<'a> {
        self.weight(crate::text::FontWeight::LIGHT)
    }
    fn medium(self) -> RichSpan<'a> {
        self.weight(crate::text::FontWeight::MEDIUM)
    }
    fn semibold(self) -> RichSpan<'a> {
        self.weight(crate::text::FontWeight::SEMI_BOLD)
    }
    fn bold(self) -> RichSpan<'a> {
        self.weight(crate::text::FontWeight::BOLD)
    }
    fn extra_bold(self) -> RichSpan<'a> {
        self.weight(crate::text::FontWeight::EXTRA_BOLD)
    }

    fn raw_weight(self, weight: u16) -> RichSpan<'a> {
        let span: RichSpan = self.into();
        span.raw_weight(weight)
    }
    fn font_size(self, font_size: f32) -> RichSpan<'a> {
        let span: RichSpan = self.into();
        span.font_size(font_size)
    }

    fn line_height(self, line_height: crate::text::LineHeightValue) -> RichSpan<'a> {
        let span: RichSpan = self.into();
        span.line_height(line_height)
    }
}

impl<'a, S> RichTextExt<'a> for S
where
    S: AsRef<str>,
    RichSpan<'a>: From<S>,
{
}
impl<'a, S: AsRef<str> + 'a> From<&'a S> for RichSpan<'a> {
    fn from(value: &'a S) -> Self {
        RichSpan {
            text: value.as_ref(),
            attrs: Attrs::new().color(palette::css::BLACK),
        }
    }
}
impl<'a> RichTextExt<'a> for RichSpan<'a> {}
