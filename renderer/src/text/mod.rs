mod attrs;
mod layout;

pub use attrs::{Attrs, AttrsList, AttrsOwned, FamilyOwned, LineHeightValue};
pub use layout::{HitPoint, HitPosition, TextLayout, FONT_CONTEXT};

// --- Font Properties ---

/// Font weight (wraps u16 for cosmic-text compat, converts to fontique::FontWeight)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Weight(pub u16);
impl Weight {
    pub const THIN: Self = Weight(100);
    pub const EXTRA_LIGHT: Self = Weight(200);
    pub const LIGHT: Self = Weight(300);
    pub const NORMAL: Self = Weight(400);
    pub const MEDIUM: Self = Weight(500);
    pub const SEMIBOLD: Self = Weight(600);
    pub const BOLD: Self = Weight(700);
    pub const EXTRA_BOLD: Self = Weight(800);
    pub const BLACK: Self = Weight(900);
}

impl From<Weight> for fontique::FontWeight {
    fn from(w: Weight) -> Self {
        fontique::FontWeight::new(w.0 as f32)
    }
}

impl From<fontique::FontWeight> for Weight {
    fn from(w: fontique::FontWeight) -> Self {
        Weight(w.value() as u16)
    }
}

/// Font style
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum Style {
    #[default]
    Normal,
    Italic,
    Oblique,
}

impl From<Style> for fontique::FontStyle {
    fn from(s: Style) -> Self {
        match s {
            Style::Normal => fontique::FontStyle::Normal,
            Style::Italic => fontique::FontStyle::Italic,
            Style::Oblique => fontique::FontStyle::Oblique(None),
        }
    }
}

/// Font stretch/width
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum Stretch {
    UltraCondensed,
    ExtraCondensed,
    Condensed,
    SemiCondensed,
    #[default]
    Normal,
    SemiExpanded,
    Expanded,
    ExtraExpanded,
    UltraExpanded,
}

impl From<Stretch> for fontique::FontWidth {
    fn from(s: Stretch) -> Self {
        match s {
            Stretch::UltraCondensed => fontique::FontWidth::ULTRA_CONDENSED,
            Stretch::ExtraCondensed => fontique::FontWidth::EXTRA_CONDENSED,
            Stretch::Condensed => fontique::FontWidth::CONDENSED,
            Stretch::SemiCondensed => fontique::FontWidth::SEMI_CONDENSED,
            Stretch::Normal => fontique::FontWidth::NORMAL,
            Stretch::SemiExpanded => fontique::FontWidth::SEMI_EXPANDED,
            Stretch::Expanded => fontique::FontWidth::EXPANDED,
            Stretch::ExtraExpanded => fontique::FontWidth::EXTRA_EXPANDED,
            Stretch::UltraExpanded => fontique::FontWidth::ULTRA_EXPANDED,
        }
    }
}

// --- Text Layout Properties ---

/// Text alignment
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Align {
    Left,
    Right,
    Center,
    Justified,
    End,
}

impl From<Align> for parley::layout::Alignment {
    fn from(a: Align) -> Self {
        match a {
            Align::Left => parley::layout::Alignment::Left,
            Align::Right => parley::layout::Alignment::Right,
            Align::Center => parley::layout::Alignment::Center,
            Align::Justified => parley::layout::Alignment::Justify,
            Align::End => parley::layout::Alignment::End,
        }
    }
}

/// Text wrap mode
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum Wrap {
    None,
    Glyph,
    #[default]
    Word,
    WordOrGlyph,
}

/// Line ending type
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum LineEnding {
    #[default]
    Lf,
    CrLf,
    Cr,
    None,
}

// --- Cursor/Hit Testing ---

/// Cursor affinity
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum Affinity {
    #[default]
    Before,
    After,
}

impl From<Affinity> for parley::layout::Affinity {
    fn from(a: Affinity) -> Self {
        match a {
            Affinity::Before => parley::layout::Affinity::Upstream,
            Affinity::After => parley::layout::Affinity::Downstream,
        }
    }
}

impl From<parley::layout::Affinity> for Affinity {
    fn from(a: parley::layout::Affinity) -> Self {
        match a {
            parley::layout::Affinity::Upstream => Affinity::Before,
            parley::layout::Affinity::Downstream => Affinity::After,
        }
    }
}

/// Text cursor position
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Cursor {
    pub line: usize,
    pub index: usize,
    pub affinity: Affinity,
}

impl Cursor {
    pub fn new(line: usize, index: usize) -> Self {
        Self {
            line,
            index,
            affinity: Affinity::Before,
        }
    }

    pub fn new_with_affinity(line: usize, index: usize, affinity: Affinity) -> Self {
        Self {
            line,
            index,
            affinity,
        }
    }
}

// Implement Ord-compatible Hash for Affinity used in Cursor's Ord
impl PartialOrd for Affinity {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Affinity {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (*self as u8).cmp(&(*other as u8))
    }
}

// --- Brush type for Parley ---

/// A brush type that wraps peniko::Color and implements Default (required by parley::Brush)
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextBrush(pub peniko::Color);

impl Default for TextBrush {
    fn default() -> Self {
        TextBrush(peniko::Color::from_rgba8(0, 0, 0, 255))
    }
}

impl From<peniko::Color> for TextBrush {
    fn from(c: peniko::Color) -> Self {
        TextBrush(c)
    }
}

impl From<TextBrush> for peniko::Color {
    fn from(b: TextBrush) -> Self {
        b.0
    }
}
