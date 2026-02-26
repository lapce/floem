use std::ops::Range;

use crate::text::TextBrush;
use crate::text::{FontWidth, FontStyle, Weight};
use fontique::GenericFamily;
use parley::style::{FontFamily, FontStack, StyleProperty};
use peniko::Color;

/// An owned version of font family.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum FamilyOwned {
    Name(String),
    Serif,
    SansSerif,
    Cursive,
    Fantasy,
    Monospace,
}

impl FamilyOwned {
    pub fn parse_list(s: &str) -> impl Iterator<Item = FamilyOwned> + '_ + Clone {
        ParseList {
            source: s.as_bytes(),
            len: s.len(),
            pos: 0,
        }
    }

    fn to_font_family(&self) -> FontFamily<'_> {
        match self {
            FamilyOwned::Name(name) => FontFamily::Named(std::borrow::Cow::Borrowed(name.as_str())),
            FamilyOwned::Serif => FontFamily::Generic(GenericFamily::Serif),
            FamilyOwned::SansSerif => FontFamily::Generic(GenericFamily::SansSerif),
            FamilyOwned::Cursive => FontFamily::Generic(GenericFamily::Cursive),
            FamilyOwned::Fantasy => FontFamily::Generic(GenericFamily::Fantasy),
            FamilyOwned::Monospace => FontFamily::Generic(GenericFamily::Monospace),
        }
    }

    pub fn to_font_stack(families: &[FamilyOwned]) -> FontStack<'_> {
        if families.len() == 1 {
            match &families[0] {
                FamilyOwned::Name(name) => {
                    FontStack::Source(std::borrow::Cow::Borrowed(name.as_str()))
                }
                other => FontStack::Single(other.to_font_family()),
            }
        } else if let Some(first) = families.first() {
            match first {
                FamilyOwned::Name(name) => {
                    FontStack::Source(std::borrow::Cow::Borrowed(name.as_str()))
                }
                other => FontStack::Single(other.to_font_family()),
            }
        } else {
            FontStack::Single(FontFamily::Generic(GenericFamily::SansSerif))
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LineHeightValue {
    Normal(f32),
    Px(f32),
}

/// Text attributes.
#[derive(Clone, Debug)]
pub struct Attrs<'a> {
    pub font_size: f32,
    line_height: LineHeightValue,
    color: Option<Color>,
    family: Option<&'a [FamilyOwned]>,
    weight: Option<Weight>,
    style: Option<FontStyle>,
    font_width: Option<FontWidth>,
    metadata: Option<usize>,
}

impl Default for Attrs<'_> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> Attrs<'a> {
    pub fn new() -> Self {
        Self {
            font_size: 16.0,
            line_height: LineHeightValue::Normal(1.0),
            color: None,
            family: None,
            weight: None,
            style: None,
            font_width: None,
            metadata: None,
        }
    }

    pub fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }

    pub fn family(mut self, family: &'a [FamilyOwned]) -> Self {
        self.family = Some(family);
        self
    }

    pub fn font_width(mut self, stretch: FontWidth) -> Self {
        self.font_width = Some(stretch);
        self
    }

    pub fn font_style(mut self, font_style: FontStyle) -> Self {
        self.style = Some(font_style);
        self
    }

    pub fn weight(mut self, weight: Weight) -> Self {
        self.weight = Some(weight);
        self
    }

    pub fn raw_weight(mut self, weight: u16) -> Self {
        self.weight = Some(Weight(weight));
        self
    }

    pub fn font_size(mut self, font_size: f32) -> Self {
        self.font_size = font_size;
        self
    }

    pub fn line_height(mut self, line_height: LineHeightValue) -> Self {
        self.line_height = line_height;
        self
    }

    pub fn metadata(mut self, metadata: usize) -> Self {
        self.metadata = Some(metadata);
        self
    }

    pub fn get_color(&self) -> Option<Color> {
        self.color
    }

    pub fn get_line_height(&self) -> LineHeightValue {
        self.line_height
    }

    pub fn get_family(&self) -> Option<&'a [FamilyOwned]> {
        self.family
    }

    pub fn get_weight(&self) -> Option<Weight> {
        self.weight
    }

    pub fn get_font_style(&self) -> Option<FontStyle> {
        self.style
    }

    pub fn get_stretch(&self) -> Option<FontWidth> {
        self.font_width
    }

    pub fn get_metadata(&self) -> Option<usize> {
        self.metadata
    }

    /// Compute the effective line height in pixels
    pub fn effective_line_height(&self) -> f32 {
        match self.line_height {
            LineHeightValue::Normal(n) => self.font_size * n,
            LineHeightValue::Px(n) => n,
        }
    }

    /// Push default style properties onto a Parley RangedBuilder
    pub fn apply_defaults(&self, builder: &mut parley::RangedBuilder<'_, TextBrush>) {
        builder.push_default(StyleProperty::FontSize(self.font_size));
        let lh = self.effective_line_height();
        builder.push_default(StyleProperty::LineHeight(
            parley::style::LineHeight::Absolute(lh),
        ));
        if let Some(color) = self.color {
            builder.push_default(StyleProperty::Brush(TextBrush(color)));
        }
        if let Some(family) = self.family {
            let stack = FamilyOwned::to_font_stack(family);
            builder.push_default(StyleProperty::FontStack(stack));
        }
        if let Some(weight) = self.weight {
            builder.push_default(StyleProperty::FontWeight(weight.into()));
        }
        if let Some(style) = self.style {
            builder.push_default(StyleProperty::FontStyle(style.into()));
        }
        if let Some(width) = self.font_width {
            builder.push_default(StyleProperty::FontWidth(width.into()));
        }
    }

    /// Push style properties for a specific range onto a Parley RangedBuilder.
    /// Only pushes properties that differ from the given defaults to reduce redundant work.
    pub fn apply_range(
        &self,
        builder: &mut parley::RangedBuilder<'_, TextBrush>,
        range: Range<usize>,
        defaults: &Attrs<'_>,
    ) {
        if self.font_size != defaults.font_size {
            builder.push(StyleProperty::FontSize(self.font_size), range.clone());
        }
        if self.effective_line_height() != defaults.effective_line_height() {
            let lh = self.effective_line_height();
            builder.push(
                StyleProperty::LineHeight(parley::style::LineHeight::Absolute(lh)),
                range.clone(),
            );
        }
        if let Some(color) = self.color {
            builder.push(StyleProperty::Brush(TextBrush(color)), range.clone());
        }
        if let Some(family) = self.family {
            let stack = FamilyOwned::to_font_stack(family);
            builder.push(StyleProperty::FontStack(stack), range.clone());
        }
        if let Some(weight) = self.weight {
            builder.push(StyleProperty::FontWeight(weight.into()), range.clone());
        }
        if let Some(style) = self.style {
            builder.push(StyleProperty::FontStyle(style.into()), range.clone());
        }
        if let Some(width) = self.font_width {
            builder.push(StyleProperty::FontWidth(width.into()), range);
        }
    }
}

/// Owned text attributes.
#[derive(Clone, Debug)]
pub struct AttrsOwned {
    pub font_size: f32,
    line_height: LineHeightValue,
    color: Option<Color>,
    family: Option<Vec<FamilyOwned>>,
    weight: Option<Weight>,
    style: Option<FontStyle>,
    font_width: Option<FontWidth>,
    metadata: Option<usize>,
}

impl AttrsOwned {
    pub fn new(attrs: Attrs) -> Self {
        Self {
            font_size: attrs.font_size,
            line_height: attrs.line_height,
            color: attrs.color,
            family: attrs.family.map(|f| f.to_vec()),
            weight: attrs.weight,
            style: attrs.style,
            font_width: attrs.font_width,
            metadata: attrs.metadata,
        }
    }

    pub fn as_attrs(&self) -> Attrs<'_> {
        Attrs {
            font_size: self.font_size,
            line_height: self.line_height,
            color: self.color,
            family: self.family.as_deref(),
            weight: self.weight,
            style: self.style,
            font_width: self.font_width,
            metadata: self.metadata,
        }
    }
}

/// Attribute spans list.
#[derive(Clone, Debug)]
pub struct AttrsList {
    defaults: AttrsOwned,
    spans: Vec<(Range<usize>, AttrsOwned)>,
}

impl PartialEq for AttrsList {
    fn eq(&self, _other: &Self) -> bool {
        // AttrsList comparison is expensive; use identity comparison or always false
        false
    }
}

impl AttrsList {
    pub fn new(defaults: Attrs) -> Self {
        Self {
            defaults: AttrsOwned::new(defaults),
            spans: Vec::new(),
        }
    }

    pub fn defaults(&self) -> Attrs<'_> {
        self.defaults.as_attrs()
    }

    pub fn clear_spans(&mut self) {
        self.spans.clear();
    }

    pub fn add_span(&mut self, range: Range<usize>, attrs: Attrs) {
        // Remove any previous spans that overlap with this range
        self.spans
            .retain(|(r, _)| r.end <= range.start || r.start >= range.end);
        self.spans.push((range, AttrsOwned::new(attrs)));
    }

    pub fn get_span(&self, index: usize) -> Attrs<'_> {
        for (range, attrs) in &self.spans {
            if range.contains(&index) {
                return attrs.as_attrs();
            }
        }
        self.defaults.as_attrs()
    }

    pub fn split_off(&mut self, index: usize) -> Self {
        let mut new_spans = Vec::new();
        let mut remaining = Vec::new();

        for (range, attrs) in self.spans.drain(..) {
            if range.start >= index {
                new_spans.push((range.start - index..range.end - index, attrs));
            } else if range.end > index {
                remaining.push((range.start..index, attrs.clone()));
                new_spans.push((0..range.end - index, attrs));
            } else {
                remaining.push((range, attrs));
            }
        }

        self.spans = remaining;

        AttrsList {
            defaults: self.defaults.clone(),
            spans: new_spans,
        }
    }

    /// Apply all defaults and spans to a Parley RangedBuilder.
    pub fn apply_to_builder(&self, builder: &mut parley::RangedBuilder<'_, TextBrush>) {
        let defaults = self.defaults.as_attrs();
        defaults.apply_defaults(builder);
        for (range, attrs) in &self.spans {
            attrs
                .as_attrs()
                .apply_range(builder, range.clone(), &defaults);
        }
    }

    /// Get the inner spans for iteration.
    pub fn spans(&self) -> &[(Range<usize>, AttrsOwned)] {
        &self.spans
    }
}

#[derive(Clone)]
struct ParseList<'a> {
    source: &'a [u8],
    len: usize,
    pos: usize,
}

impl Iterator for ParseList<'_> {
    type Item = FamilyOwned;

    fn next(&mut self) -> Option<Self::Item> {
        let mut quote = None;
        let mut pos = self.pos;
        while pos < self.len && {
            let ch = self.source[pos];
            ch.is_ascii_whitespace() || ch == b','
        } {
            pos += 1;
        }
        self.pos = pos;
        if pos >= self.len {
            return None;
        }
        let first = self.source[pos];
        let mut start = pos;
        match first {
            b'"' | b'\'' => {
                quote = Some(first);
                pos += 1;
                start += 1;
            }
            _ => {}
        }
        if let Some(quote) = quote {
            while pos < self.len {
                if self.source[pos] == quote {
                    self.pos = pos + 1;
                    return Some(FamilyOwned::Name(
                        core::str::from_utf8(self.source.get(start..pos)?)
                            .ok()?
                            .trim()
                            .to_string(),
                    ));
                }
                pos += 1;
            }
            self.pos = pos;
            return Some(FamilyOwned::Name(
                core::str::from_utf8(self.source.get(start..pos)?)
                    .ok()?
                    .trim()
                    .to_string(),
            ));
        }
        let mut end = start;
        while pos < self.len {
            if self.source[pos] == b',' {
                pos += 1;
                break;
            }
            pos += 1;
            end += 1;
        }
        self.pos = pos;
        let name = core::str::from_utf8(self.source.get(start..end)?)
            .ok()?
            .trim();
        Some(match name.to_lowercase().as_str() {
            "serif" => FamilyOwned::Serif,
            "sans-serif" => FamilyOwned::SansSerif,
            "monospace" => FamilyOwned::Monospace,
            "cursive" => FamilyOwned::Cursive,
            "fantasy" => FamilyOwned::Fantasy,
            _ => FamilyOwned::Name(name.to_string()),
        })
    }
}
