use std::ops::Range;

use crate::text::TextBrush;
use crate::text::{FontStyle, FontWeight, FontWidth};
use fontique::GenericFamily;
use parley::style::{FontFamily, FontStack, StyleProperty};
use peniko::Color;

/// An owned font family identifier.
///
/// This is an owned equivalent of Parley's [`FontFamily`] that can be stored
/// and cloned independently of any layout context. It supports both named fonts
/// and the standard CSS generic families.
///
/// # Example
///
/// ```
/// use floem_renderer::text::FamilyOwned;
///
/// let families: Vec<FamilyOwned> = FamilyOwned::parse_list("'Fira Code', monospace").collect();
/// assert_eq!(families, vec![
///     FamilyOwned::Name("Fira Code".to_string()),
///     FamilyOwned::Monospace,
/// ]);
/// ```
///
/// [`FontFamily`]: parley::style::FontFamily
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum FamilyOwned {
    /// A named font family (e.g. `"Helvetica"`, `"Fira Code"`).
    Name(String),
    /// The generic serif family.
    Serif,
    /// The generic sans-serif family.
    SansSerif,
    /// The generic cursive family.
    Cursive,
    /// The generic fantasy family.
    Fantasy,
    /// The generic monospace family.
    Monospace,
}

impl FamilyOwned {
    /// Parses a CSS-style comma-separated font family list into an iterator of [`FamilyOwned`] values.
    ///
    /// Quoted names (single or double quotes) are treated as named families.
    /// Unquoted generic keywords (`serif`, `sans-serif`, `monospace`, `cursive`, `fantasy`)
    /// are mapped to their corresponding variants. All other unquoted names become
    /// [`FamilyOwned::Name`].
    ///
    /// # Example
    ///
    /// ```
    /// use floem_renderer::text::FamilyOwned;
    ///
    /// let families: Vec<_> = FamilyOwned::parse_list("Arial, sans-serif").collect();
    /// assert_eq!(families, vec![
    ///     FamilyOwned::Name("Arial".to_string()),
    ///     FamilyOwned::SansSerif,
    /// ]);
    /// ```
    pub fn parse_list(s: &str) -> impl Iterator<Item = FamilyOwned> + '_ + Clone {
        ParseList {
            source: s.as_bytes(),
            len: s.len(),
            pos: 0,
        }
    }

    /// Converts this owned family to a borrowed Parley [`FontFamily`] reference.
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

    /// Converts a slice of owned families into a Parley [`FontStack`].
    ///
    /// For a single named family, this produces a [`FontStack::Source`] so that
    /// Parley can parse comma-separated fallbacks within the name string.
    /// For a single generic family, it produces [`FontStack::Single`].
    /// An empty slice defaults to sans-serif.
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

/// Specifies how line height is computed for text layout.
///
/// # Example
///
/// ```
/// use floem_renderer::text::{Attrs, LineHeightValue};
///
/// // 1.5x the font size (e.g. 24px for a 16px font).
/// let attrs = Attrs::new().line_height(LineHeightValue::Normal(1.5));
///
/// // Fixed 20-pixel line height regardless of font size.
/// let attrs = Attrs::new().line_height(LineHeightValue::Px(20.0));
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LineHeightValue {
    /// A multiplier of the font size (e.g. `1.0` means line height equals font size).
    Normal(f32),
    /// An absolute line height in pixels.
    Px(f32),
}

/// Text styling attributes used to configure font properties, color, and layout.
///
/// `Attrs` uses a builder pattern where each setter consumes and returns `self`,
/// making it easy to chain calls. Unset fields (`None`) inherit from the layout
/// defaults when applied to a Parley builder.
///
/// # Defaults
///
/// | Property    | Default                           |
/// |-------------|-----------------------------------|
/// | font_size   | `16.0`                            |
/// | line_height | `LineHeightValue::Normal(1.0)`    |
/// | color       | `None` (inherits from context)    |
/// | family      | `None` (system default)           |
/// | weight      | `None` (normal)                   |
/// | style       | `None` (normal)                   |
/// | font_width  | `None` (normal)                   |
/// | metadata    | `None`                            |
///
/// # Example
///
/// ```
/// use floem_renderer::text::{Attrs, FamilyOwned, FontWeight, LineHeightValue};
/// use peniko::Color;
///
/// let families = [FamilyOwned::Name("Inter".to_string()), FamilyOwned::SansSerif];
/// let attrs = Attrs::new()
///     .family(&families)
///     .font_size(14.0)
///     .weight(FontWeight::BOLD)
///     .color(Color::WHITE)
///     .line_height(LineHeightValue::Normal(1.4));
/// ```
#[derive(Clone, Debug)]
pub struct Attrs<'a> {
    /// Font size in pixels.
    pub font_size: f32,
    /// Line height mode — either a multiplier of `font_size` or an absolute pixel value.
    line_height: LineHeightValue,
    /// Text color, or `None` to inherit from the rendering context.
    color: Option<Color>,
    /// Ordered list of font families to try, or `None` to use the system default.
    family: Option<&'a [FamilyOwned]>,
    /// Font weight (e.g. normal, bold), or `None` for the default weight.
    weight: Option<FontWeight>,
    /// Font style (normal, italic, oblique), or `None` for normal.
    style: Option<FontStyle>,
    /// Font width / stretch (e.g. condensed, expanded), or `None` for normal.
    font_width: Option<FontWidth>,
    /// Application-defined metadata carried through layout without interpretation.
    metadata: Option<usize>,
}

impl Default for Attrs<'_> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> Attrs<'a> {
    /// Creates a new `Attrs` with default values (16px font, 1.0 line height multiplier).
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

    /// Sets the text color.
    pub fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }

    /// Sets the font family list. Families are tried in order as fallbacks.
    pub fn family(mut self, family: &'a [FamilyOwned]) -> Self {
        self.family = Some(family);
        self
    }

    /// Sets the font width (stretch), e.g. condensed or expanded.
    pub fn font_width(mut self, stretch: FontWidth) -> Self {
        self.font_width = Some(stretch);
        self
    }

    /// Sets the font style (normal, italic, or oblique).
    pub fn font_style(mut self, font_style: FontStyle) -> Self {
        self.style = Some(font_style);
        self
    }

    /// Sets the font weight (e.g. [`FontWeight::BOLD`]).
    pub fn weight(mut self, weight: FontWeight) -> Self {
        self.weight = Some(weight);
        self
    }

    /// Sets the font weight from a raw numeric value (typically 100–900).
    pub fn raw_weight(mut self, weight: u16) -> Self {
        self.weight = Some(FontWeight::new(weight as f32));
        self
    }

    /// Sets the font size in pixels.
    pub fn font_size(mut self, font_size: f32) -> Self {
        self.font_size = font_size;
        self
    }

    /// Sets the line height. See [`LineHeightValue`] for the available modes.
    pub fn line_height(mut self, line_height: LineHeightValue) -> Self {
        self.line_height = line_height;
        self
    }

    /// Sets an opaque metadata value that is carried through layout.
    ///
    /// This can be used to associate application-specific data (e.g. a span
    /// identifier) with a range of text.
    pub fn metadata(mut self, metadata: usize) -> Self {
        self.metadata = Some(metadata);
        self
    }

    /// Returns the text color, or `None` if unset.
    pub fn get_color(&self) -> Option<Color> {
        self.color
    }

    /// Returns the line height setting.
    pub fn get_line_height(&self) -> LineHeightValue {
        self.line_height
    }

    /// Returns the font family list, or `None` if unset.
    pub fn get_family(&self) -> Option<&'a [FamilyOwned]> {
        self.family
    }

    /// Returns the font weight, or `None` if unset.
    pub fn get_weight(&self) -> Option<FontWeight> {
        self.weight
    }

    /// Returns the font style, or `None` if unset.
    pub fn get_font_style(&self) -> Option<FontStyle> {
        self.style
    }

    /// Returns the font width (stretch), or `None` if unset.
    pub fn get_stretch(&self) -> Option<FontWidth> {
        self.font_width
    }

    /// Returns the metadata value, or `None` if unset.
    pub fn get_metadata(&self) -> Option<usize> {
        self.metadata
    }

    /// Computes the effective line height in pixels.
    ///
    /// For [`LineHeightValue::Normal`], this multiplies the font size by the factor.
    /// For [`LineHeightValue::Px`], the pixel value is returned directly.
    pub fn effective_line_height(&self) -> f32 {
        match self.line_height {
            LineHeightValue::Normal(n) => self.font_size * n,
            LineHeightValue::Px(n) => n,
        }
    }

    /// Pushes all set properties as defaults onto a Parley [`RangedBuilder`].
    ///
    /// Font size and line height are always pushed. Optional properties (color,
    /// family, weight, style, width) are only pushed when set.
    ///
    /// [`RangedBuilder`]: parley::RangedBuilder
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
            builder.push_default(StyleProperty::FontWeight(weight));
        }
        if let Some(style) = self.style {
            builder.push_default(StyleProperty::FontStyle(style));
        }
        if let Some(width) = self.font_width {
            builder.push_default(StyleProperty::FontWidth(width));
        }
    }

    /// Pushes style properties for a specific byte range onto a Parley [`RangedBuilder`].
    ///
    /// Only properties that differ from `defaults` are pushed, avoiding redundant
    /// work when a span shares most attributes with the base style.
    ///
    /// [`RangedBuilder`]: parley::RangedBuilder
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
            builder.push(StyleProperty::FontWeight(weight), range.clone());
        }
        if let Some(style) = self.style {
            builder.push(StyleProperty::FontStyle(style), range.clone());
        }
        if let Some(width) = self.font_width {
            builder.push(StyleProperty::FontWidth(width), range);
        }
    }
}

/// An owned version of [`Attrs`] that does not borrow the font family slice.
///
/// This is used internally by [`AttrsList`] to store attribute spans, since spans
/// need to own their data independently of the caller's lifetime.
///
/// # Example
///
/// ```
/// use floem_renderer::text::{Attrs, AttrsOwned, FamilyOwned, FontWeight};
///
/// let families = [FamilyOwned::Monospace];
/// let attrs = Attrs::new().family(&families).weight(FontWeight::BOLD);
/// let owned = AttrsOwned::new(attrs);
///
/// // Convert back to a borrowed Attrs for use with builders.
/// let borrowed = owned.as_attrs();
/// ```
#[derive(Clone, Debug)]
pub struct AttrsOwned {
    /// Font size in pixels.
    pub font_size: f32,
    /// Line height mode — either a multiplier of `font_size` or an absolute pixel value.
    line_height: LineHeightValue,
    /// Text color, or `None` to inherit from the rendering context.
    color: Option<Color>,
    /// Owned list of font families to try, or `None` to use the system default.
    family: Option<Vec<FamilyOwned>>,
    /// Font weight (e.g. normal, bold), or `None` for the default weight.
    weight: Option<FontWeight>,
    /// Font style (normal, italic, oblique), or `None` for normal.
    style: Option<FontStyle>,
    /// Font width / stretch (e.g. condensed, expanded), or `None` for normal.
    font_width: Option<FontWidth>,
    /// Application-defined metadata carried through layout without interpretation.
    metadata: Option<usize>,
}

impl AttrsOwned {
    /// Creates an owned copy of the given [`Attrs`], cloning the font family slice if present.
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

    /// Returns a borrowed [`Attrs`] referencing this owned data.
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

/// A list of text attributes with default styling and per-range overrides.
///
/// `AttrsList` pairs a set of default [`Attrs`] with zero or more byte-range spans
/// that override specific properties. When applied to a Parley builder via
/// [`apply_to_builder`](Self::apply_to_builder), the defaults are pushed first,
/// then each span is layered on top for its range.
///
/// # Example
///
/// ```
/// use floem_renderer::text::{Attrs, AttrsList, FontWeight};
/// use peniko::Color;
///
/// let mut attrs_list = AttrsList::new(Attrs::new().font_size(14.0));
///
/// // Make bytes 0..5 bold and red.
/// attrs_list.add_span(
///     0..5,
///     Attrs::new()
///         .font_size(14.0)
///         .weight(FontWeight::BOLD)
///         .color(Color::WHITE),
/// );
/// ```
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
    /// Creates a new attribute list with the given default attributes and no spans.
    pub fn new(defaults: Attrs) -> Self {
        Self {
            defaults: AttrsOwned::new(defaults),
            spans: Vec::new(),
        }
    }

    /// Returns the default attributes.
    pub fn defaults(&self) -> Attrs<'_> {
        self.defaults.as_attrs()
    }

    /// Removes all attribute spans, keeping only the defaults.
    pub fn clear_spans(&mut self) {
        self.spans.clear();
    }

    /// Adds an attribute span for the given byte range.
    ///
    /// Any existing spans that overlap with `range` are removed before the new
    /// span is inserted.
    pub fn add_span(&mut self, range: Range<usize>, attrs: Attrs) {
        // Remove any previous spans that overlap with this range
        self.spans
            .retain(|(r, _)| r.end <= range.start || r.start >= range.end);
        self.spans.push((range, AttrsOwned::new(attrs)));
    }

    /// Returns the attributes at the given byte index.
    ///
    /// If a span covers `index`, its attributes are returned. Otherwise the
    /// defaults are returned.
    pub fn get_span(&self, index: usize) -> Attrs<'_> {
        for (range, attrs) in &self.spans {
            if range.contains(&index) {
                return attrs.as_attrs();
            }
        }
        self.defaults.as_attrs()
    }

    /// Splits this attribute list at the given byte index.
    ///
    /// Returns a new `AttrsList` covering `[index..)` with span ranges shifted
    /// to start from zero. Spans that cross the split point are duplicated into
    /// both halves with their ranges adjusted accordingly. `self` is left
    /// containing only the `[..index)` portion.
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

    /// Applies all defaults and spans to a Parley [`RangedBuilder`].
    ///
    /// This first pushes the default attributes, then layers each span on top
    /// for its byte range. Span properties that match the defaults are skipped
    /// to avoid redundant work.
    ///
    /// [`RangedBuilder`]: parley::RangedBuilder
    pub fn apply_to_builder(&self, builder: &mut parley::RangedBuilder<'_, TextBrush>) {
        let defaults = self.defaults.as_attrs();
        defaults.apply_defaults(builder);
        for (range, attrs) in &self.spans {
            attrs
                .as_attrs()
                .apply_range(builder, range.clone(), &defaults);
        }
    }

    /// Returns the inner spans as a slice of `(byte_range, attributes)` pairs.
    pub fn spans(&self) -> &[(Range<usize>, AttrsOwned)] {
        &self.spans
    }
}

/// A streaming parser for CSS-style comma-separated font family lists.
///
/// Created by [`FamilyOwned::parse_list`]. Walks the input bytes left-to-right,
/// yielding one [`FamilyOwned`] per entry. The parser handles:
///
/// - **Quoted names** (single `'` or double `"` quotes) — the content between
///   the quotes is trimmed and returned as [`FamilyOwned::Name`].
/// - **Unquoted generic keywords** (`serif`, `sans-serif`, `monospace`,
///   `cursive`, `fantasy`) — matched case-insensitively and mapped to the
///   corresponding enum variant.
/// - **Unquoted custom names** — everything up to the next comma, trimmed.
///
/// Leading/trailing whitespace and commas between entries are skipped.
#[derive(Clone)]
struct ParseList<'a> {
    /// The raw input bytes (must be valid UTF-8).
    source: &'a [u8],
    /// Cached `source.len()`.
    len: usize,
    /// Current read position in `source`.
    pos: usize,
}

impl Iterator for ParseList<'_> {
    type Item = FamilyOwned;

    /// Advances the parser and returns the next [`FamilyOwned`], or `None`
    /// when the input is exhausted.
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


#[cfg(test)]
mod tests {
    use super::*;

    // ========================== FamilyOwned ==========================

    #[test]
    fn parse_list_named_and_generic() {
        let families: Vec<_> = FamilyOwned::parse_list("Arial, sans-serif").collect();
        assert_eq!(
            families,
            vec![
                FamilyOwned::Name("Arial".to_string()),
                FamilyOwned::SansSerif,
            ]
        );
    }

    #[test]
    fn parse_list_quoted_names() {
        let families: Vec<_> =
            FamilyOwned::parse_list("'Fira Code', \"Noto Sans\", monospace").collect();
        assert_eq!(
            families,
            vec![
                FamilyOwned::Name("Fira Code".to_string()),
                FamilyOwned::Name("Noto Sans".to_string()),
                FamilyOwned::Monospace,
            ]
        );
    }

    #[test]
    fn parse_list_all_generics() {
        let families: Vec<_> =
            FamilyOwned::parse_list("serif, sans-serif, monospace, cursive, fantasy").collect();
        assert_eq!(
            families,
            vec![
                FamilyOwned::Serif,
                FamilyOwned::SansSerif,
                FamilyOwned::Monospace,
                FamilyOwned::Cursive,
                FamilyOwned::Fantasy,
            ]
        );
    }

    #[test]
    fn parse_list_case_insensitive() {
        let families: Vec<_> = FamilyOwned::parse_list("SERIF, Sans-Serif").collect();
        assert_eq!(families, vec![FamilyOwned::Serif, FamilyOwned::SansSerif]);
    }

    #[test]
    fn parse_list_empty() {
        let families: Vec<_> = FamilyOwned::parse_list("").collect();
        assert!(families.is_empty());
    }

    #[test]
    fn parse_list_whitespace_only() {
        let families: Vec<_> = FamilyOwned::parse_list("  , , ").collect();
        assert!(families.is_empty());
    }

    #[test]
    fn to_font_stack_single_named() {
        let families = vec![FamilyOwned::Name("Inter".to_string())];
        let stack = FamilyOwned::to_font_stack(&families);
        assert!(matches!(stack, FontStack::Source(_)));
    }

    #[test]
    fn to_font_stack_single_generic() {
        let families = vec![FamilyOwned::Monospace];
        let stack = FamilyOwned::to_font_stack(&families);
        assert!(matches!(stack, FontStack::Single(_)));
    }

    #[test]
    fn to_font_stack_empty_defaults_to_sans_serif() {
        let families: Vec<FamilyOwned> = vec![];
        let stack = FamilyOwned::to_font_stack(&families);
        assert!(matches!(
            stack,
            FontStack::Single(FontFamily::Generic(GenericFamily::SansSerif))
        ));
    }

    // ========================== Attrs ==========================

    #[test]
    fn attrs_defaults() {
        let a = Attrs::new();
        assert_eq!(a.font_size, 16.0);
        assert_eq!(a.get_line_height(), LineHeightValue::Normal(1.0));
        assert_eq!(a.get_color(), None);
        assert_eq!(a.get_family(), None);
        assert_eq!(a.get_weight(), None);
        assert_eq!(a.get_font_style(), None);
        assert_eq!(a.get_stretch(), None);
        assert_eq!(a.get_metadata(), None);
    }

    #[test]
    fn attrs_builder_chain() {
        let families = [FamilyOwned::Monospace];
        let a = Attrs::new()
            .font_size(20.0)
            .color(Color::WHITE)
            .family(&families)
            .weight(FontWeight::BOLD)
            .font_style(FontStyle::Italic)
            .font_width(FontWidth::CONDENSED)
            .line_height(LineHeightValue::Px(24.0))
            .metadata(42);

        assert_eq!(a.font_size, 20.0);
        assert_eq!(a.get_color(), Some(Color::WHITE));
        assert_eq!(a.get_family(), Some(families.as_slice()));
        assert_eq!(a.get_weight(), Some(FontWeight::BOLD));
        assert_eq!(a.get_font_style(), Some(FontStyle::Italic));
        assert_eq!(a.get_stretch(), Some(FontWidth::CONDENSED));
        assert_eq!(a.get_line_height(), LineHeightValue::Px(24.0));
        assert_eq!(a.get_metadata(), Some(42));
    }

    #[test]
    fn attrs_raw_weight() {
        let a = Attrs::new().raw_weight(700);
        assert_eq!(a.get_weight(), Some(FontWeight::new(700.0)));
    }

    #[test]
    fn effective_line_height_normal_multiplier() {
        let a = Attrs::new()
            .font_size(20.0)
            .line_height(LineHeightValue::Normal(1.5));
        assert!((a.effective_line_height() - 30.0).abs() < f32::EPSILON);
    }

    #[test]
    fn effective_line_height_px_absolute() {
        let a = Attrs::new()
            .font_size(20.0)
            .line_height(LineHeightValue::Px(24.0));
        assert!((a.effective_line_height() - 24.0).abs() < f32::EPSILON);
    }

    #[test]
    fn effective_line_height_default() {
        // Default: Normal(1.0), font_size 16.0 → 16.0.
        let a = Attrs::new();
        assert!((a.effective_line_height() - 16.0).abs() < f32::EPSILON);
    }

    // ========================== AttrsOwned ==========================

    #[test]
    fn attrs_owned_roundtrip() {
        let families = [
            FamilyOwned::Name("Inter".to_string()),
            FamilyOwned::SansSerif,
        ];
        let a = Attrs::new()
            .font_size(18.0)
            .family(&families)
            .weight(FontWeight::BOLD)
            .color(Color::WHITE)
            .metadata(7);

        let owned = AttrsOwned::new(a);
        let back = owned.as_attrs();

        assert_eq!(back.font_size, 18.0);
        assert_eq!(back.get_weight(), Some(FontWeight::BOLD));
        assert_eq!(back.get_color(), Some(Color::WHITE));
        assert_eq!(back.get_metadata(), Some(7));
        // Family should be preserved.
        let fam = back.get_family().unwrap();
        assert_eq!(fam.len(), 2);
        assert_eq!(fam[0], FamilyOwned::Name("Inter".to_string()));
        assert_eq!(fam[1], FamilyOwned::SansSerif);
    }

    #[test]
    fn attrs_owned_no_family() {
        let a = Attrs::new();
        let owned = AttrsOwned::new(a);
        assert_eq!(owned.as_attrs().get_family(), None);
    }

    // ========================== AttrsList ==========================

    #[test]
    fn attrs_list_new_has_no_spans() {
        let list = AttrsList::new(Attrs::new().font_size(14.0));
        assert_eq!(list.defaults().font_size, 14.0);
        assert!(list.spans().is_empty());
    }

    #[test]
    fn attrs_list_add_span_and_get() {
        let mut list = AttrsList::new(Attrs::new().font_size(14.0));
        list.add_span(5..10, Attrs::new().font_size(20.0).weight(FontWeight::BOLD));

        // Inside span.
        let at7 = list.get_span(7);
        assert_eq!(at7.font_size, 20.0);
        assert_eq!(at7.get_weight(), Some(FontWeight::BOLD));

        // Outside span — returns defaults.
        let at0 = list.get_span(0);
        assert_eq!(at0.font_size, 14.0);
        assert_eq!(at0.get_weight(), None);

        // At span boundary (end is exclusive).
        let at10 = list.get_span(10);
        assert_eq!(at10.font_size, 14.0);
    }

    #[test]
    fn attrs_list_overlapping_span_replaces() {
        let mut list = AttrsList::new(Attrs::new());
        list.add_span(0..10, Attrs::new().font_size(20.0));
        list.add_span(5..15, Attrs::new().font_size(30.0));

        // Original span 0..10 should be removed since it overlaps 5..15.
        assert_eq!(list.spans().len(), 1);
        assert_eq!(list.spans()[0].0, 5..15);
    }

    #[test]
    fn attrs_list_clear_spans() {
        let mut list = AttrsList::new(Attrs::new());
        list.add_span(0..5, Attrs::new().font_size(20.0));
        list.add_span(5..10, Attrs::new().font_size(30.0));
        assert_eq!(list.spans().len(), 2);

        list.clear_spans();
        assert!(list.spans().is_empty());
        // Defaults preserved.
        assert_eq!(list.defaults().font_size, 16.0);
    }

    #[test]
    fn attrs_list_split_off_basic() {
        let mut list = AttrsList::new(Attrs::new().font_size(14.0));
        list.add_span(2..4, Attrs::new().font_size(20.0));
        list.add_span(6..8, Attrs::new().font_size(30.0));

        let right = list.split_off(5);

        // Left: only 2..4 remains (entirely before split point).
        assert_eq!(list.spans().len(), 1);
        assert_eq!(list.spans()[0].0, 2..4);

        // Right: 6..8 shifted to 1..3.
        assert_eq!(right.spans().len(), 1);
        assert_eq!(right.spans()[0].0, 1..3);
    }

    #[test]
    fn attrs_list_split_off_crossing_span() {
        let mut list = AttrsList::new(Attrs::new());
        list.add_span(3..7, Attrs::new().font_size(20.0));

        let right = list.split_off(5);

        // Left: 3..5 (truncated at split).
        assert_eq!(list.spans().len(), 1);
        assert_eq!(list.spans()[0].0, 3..5);

        // Right: 0..2 (crossing span starts at 0 in new list).
        assert_eq!(right.spans().len(), 1);
        assert_eq!(right.spans()[0].0, 0..2);
        assert_eq!(right.spans()[0].1.font_size, 20.0);
    }

    #[test]
    fn attrs_list_split_off_empty() {
        let mut list = AttrsList::new(Attrs::new().font_size(14.0));
        let right = list.split_off(0);
        assert!(list.spans().is_empty());
        assert!(right.spans().is_empty());
        assert_eq!(right.defaults().font_size, 14.0);
    }
}
