use std::ops::Range;

use crate::text::{fontdb, Family, Stretch, Style, Weight};
use peniko::Color;

/// An owned version of [`Family`]
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
    pub fn new(family: Family) -> Self {
        match family {
            Family::Name(name) => FamilyOwned::Name(name.to_string()),
            Family::Serif => FamilyOwned::Serif,
            Family::SansSerif => FamilyOwned::SansSerif,
            Family::Cursive => FamilyOwned::Cursive,
            Family::Fantasy => FamilyOwned::Fantasy,
            Family::Monospace => FamilyOwned::Monospace,
        }
    }

    pub fn as_family(&self) -> Family {
        match self {
            FamilyOwned::Name(name) => Family::Name(name),
            FamilyOwned::Serif => Family::Serif,
            FamilyOwned::SansSerif => Family::SansSerif,
            FamilyOwned::Cursive => Family::Cursive,
            FamilyOwned::Fantasy => Family::Fantasy,
            FamilyOwned::Monospace => Family::Monospace,
        }
    }

    pub fn parse_list(s: &str) -> impl Iterator<Item = FamilyOwned> + '_ + Clone {
        ParseList {
            source: s.as_bytes(),
            len: s.len(),
            pos: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LineHeightValue {
    Normal(f32),
    Px(f32),
}

/// Text attributes
#[derive(Clone, Debug)]
pub struct AttrsOwned {
    attrs: cosmic_text::AttrsOwned,
    pub font_size: f32,
    line_height: LineHeightValue,
}
impl AttrsOwned {
    pub fn new(attrs: Attrs) -> Self {
        Self {
            attrs: cosmic_text::AttrsOwned::new(attrs.attrs),
            font_size: attrs.font_size,
            line_height: attrs.line_height,
        }
    }

    pub fn as_attrs(&self) -> Attrs {
        Attrs {
            attrs: self.attrs.as_attrs(),
            font_size: self.font_size,
            line_height: self.line_height,
        }
    }
}

/// Text attributes
#[derive(Clone, Copy, Debug)]
pub struct Attrs<'a> {
    attrs: cosmic_text::Attrs<'a>,
    pub font_size: f32,
    line_height: LineHeightValue,
}

impl<'a> Default for Attrs<'a> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> Attrs<'a> {
    /// Create a new set of attributes with sane defaults
    ///
    /// This defaults to a regular Sans-Serif font.
    pub fn new() -> Self {
        Self {
            attrs: cosmic_text::Attrs::new(),
            font_size: 16.0,
            line_height: LineHeightValue::Normal(1.0),
        }
    }

    /// Set [Color]
    pub fn color(mut self, color: Color) -> Self {
        self.attrs = self
            .attrs
            .color(cosmic_text::Color::rgba(color.r, color.g, color.b, color.a));
        self
    }

    /// Set [Family]
    pub fn family(mut self, family: &'a [FamilyOwned]) -> Self {
        if let Some(family) = family.first() {
            self.attrs = self.attrs.family(family.as_family());
        }
        self
    }

    /// Set [Stretch]
    pub fn stretch(mut self, stretch: Stretch) -> Self {
        self.attrs = self.attrs.stretch(stretch);
        self
    }

    /// Set [Style]
    pub fn style(mut self, style: Style) -> Self {
        self.attrs = self.attrs.style(style);
        self
    }

    /// Set [Weight]
    pub fn weight(mut self, weight: Weight) -> Self {
        self.attrs = self.attrs.weight(weight);
        self
    }

    /// Set Weight from u16 value
    pub fn raw_weight(mut self, weight: u16) -> Self {
        self.attrs = self.attrs.weight(Weight(weight));
        self
    }

    fn get_metrics(&self) -> cosmic_text::Metrics {
        let line_height = match self.line_height {
            LineHeightValue::Normal(n) => self.font_size * n,
            LineHeightValue::Px(n) => n,
        };
        cosmic_text::Metrics::new(self.font_size, line_height)
    }

    /// Set font size
    pub fn font_size(mut self, font_size: f32) -> Self {
        self.font_size = font_size;
        self.attrs = self.attrs.metrics(self.get_metrics());
        self
    }

    /// Set line height
    pub fn line_height(mut self, line_height: LineHeightValue) -> Self {
        self.line_height = line_height;
        self.attrs = self.attrs.metrics(self.get_metrics());
        self
    }

    /// Set metadata
    pub fn metadata(mut self, metadata: usize) -> Self {
        self.attrs = self.attrs.metadata(metadata);
        self
    }

    /// Check if font matches
    pub fn matches(&self, face: &fontdb::FaceInfo) -> bool {
        self.attrs.matches(face)
    }

    /// Check if this set of attributes can be shaped with another
    pub fn compatible(&self, other: &Self) -> bool {
        self.attrs.compatible(&other.attrs)
    }
}

#[derive(PartialEq, Clone)]
pub struct AttrsList(pub(crate) cosmic_text::AttrsList);

impl AttrsList {
    /// Create a new attributes list with a set of default [Attrs]
    pub fn new(defaults: Attrs) -> Self {
        Self(cosmic_text::AttrsList::new(defaults.attrs))
    }

    /// Get the default [Attrs]
    pub fn defaults(&self) -> Attrs {
        self.0.defaults().into()
    }

    /// Clear the current attribute spans
    pub fn clear_spans(&mut self) {
        self.0.clear_spans();
    }

    /// Add an attribute span, removes any previous matching parts of spans
    pub fn add_span(&mut self, range: Range<usize>, attrs: Attrs) {
        self.0.add_span(range, attrs.attrs);
    }

    /// Get the attribute span for an index
    ///
    /// This returns a span that contains the index
    pub fn get_span(&self, index: usize) -> Attrs {
        self.0.get_span(index).into()
    }

    /// Split attributes list at an offset
    pub fn split_off(&mut self, index: usize) -> Self {
        let new = self.0.split_off(index);
        Self(new)
    }
}

impl<'a> From<cosmic_text::Attrs<'a>> for Attrs<'a> {
    fn from(attrs: cosmic_text::Attrs<'a>) -> Self {
        Self {
            attrs,
            font_size: 1.0,
            line_height: LineHeightValue::Normal(1.0),
        }
    }
}

#[derive(Clone)]
struct ParseList<'a> {
    source: &'a [u8],
    len: usize,
    pos: usize,
}

impl<'a> Iterator for ParseList<'a> {
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
