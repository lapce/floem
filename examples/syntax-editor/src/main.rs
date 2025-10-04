use floem::peniko::Color;
use floem::reactive::{RwSignal, SignalGet, SignalUpdate};
use floem::text::{Attrs, AttrsList, Stretch, Style, Weight};
use floem::views::button;
use floem::views::editor::core::buffer::rope_text::RopeText;
use floem::views::editor::core::cursor::CursorAffinity;
use floem::views::editor::id::EditorId;
use floem::views::editor::layout::TextLayoutLine;
use floem::views::editor::text::{default_dark_color, Document, SimpleStylingBuilder, Styling};
use floem::views::editor::EditorStyle;
use floem::{
    keyboard::{Key, NamedKey},
    text::FamilyOwned,
    views::{
        editor::{
            core::{editor::EditType, selection::Selection},
            text::WrapMethod,
        },
        stack, text_editor, Decorators,
    },
};
use floem::{IntoView, View};
use lazy_static::lazy_static;
use std::borrow::Cow;
use std::cell::RefCell;
use std::rc::Rc;
use syntect::highlighting::{
    FontStyle, HighlightState, Highlighter, RangedHighlightIterator, ThemeSet,
};
use syntect::parsing::{ParseState, ScopeStack, SyntaxReference, SyntaxSet};

lazy_static! {
    pub static ref SYNTAXSET: SyntaxSet = SyntaxSet::load_defaults_newlines();
    pub static ref THEMES: ThemeSet = ThemeSet::load_defaults();
}

struct SyntaxHighlightingStyle<'a> {
    pub syntax: &'a SyntaxReference,
    pub highlighter: Highlighter<'a>,
    pub style: Rc<dyn Styling>,
    pub doc: Option<Rc<dyn Document>>,
    pub states: RefCell<Vec<(ParseState, HighlightState)>>,
}

impl SyntaxHighlightingStyle<'_> {
    pub fn new(style: Rc<dyn Styling>) -> Self {
        let theme = &THEMES.themes["base16-ocean.dark"];
        let rust = SYNTAXSET.find_syntax_by_extension("rs").unwrap();
        let highlighter = Highlighter::new(theme);

        SyntaxHighlightingStyle {
            syntax: rust,
            highlighter,
            style,
            doc: None,
            states: RefCell::new(Vec::new()),
        }
    }

    pub fn set_doc(&mut self, doc: Rc<dyn Document>) {
        self.doc = Some(doc);
    }
}

impl Styling for SyntaxHighlightingStyle<'_> {
    fn id(&self) -> u64 {
        self.style.id()
    }

    fn font_size(&self, edid: EditorId, line: usize) -> usize {
        self.style.font_size(edid, line)
    }

    fn line_height(&self, edid: EditorId, line: usize) -> f32 {
        self.style.line_height(edid, line)
    }

    fn font_family(&self, edid: EditorId, line: usize) -> Cow<'_, [FamilyOwned]> {
        self.style.font_family(edid, line)
    }

    fn weight(&self, edid: EditorId, line: usize) -> Weight {
        self.style.weight(edid, line)
    }

    fn italic_style(&self, edid: EditorId, line: usize) -> Style {
        self.style.italic_style(edid, line)
    }

    fn stretch(&self, edid: EditorId, line: usize) -> Stretch {
        self.style.stretch(edid, line)
    }

    fn indent_line(&self, edid: EditorId, line: usize, line_content: &str) -> usize {
        self.style.indent_line(edid, line, line_content)
    }

    fn tab_width(&self, edid: EditorId, line: usize) -> usize {
        self.style.tab_width(edid, line)
    }

    fn atomic_soft_tabs(&self, edid: EditorId, line: usize) -> bool {
        self.style.atomic_soft_tabs(edid, line)
    }

    fn apply_attr_styles(
        &self,
        _edid: EditorId,
        _style: &EditorStyle,
        line: usize,
        default: Attrs,
        attrs: &mut AttrsList,
    ) {
        attrs.clear_spans();
        if let Some(doc) = &self.doc {
            // states are cached every 16 lines
            let mut states_cache = self.states.borrow_mut();
            let start = (line >> 4).min(states_cache.len());
            states_cache.truncate(start);
            let mut states = states_cache.last().cloned().unwrap_or_else(|| {
                (
                    ParseState::new(self.syntax),
                    HighlightState::new(&self.highlighter, ScopeStack::new()),
                )
            });

            for line_no in start..=line {
                let text = doc.rope_text().line_content(line).to_string();
                if let Ok(ops) = states.0.parse_line(&text, &SYNTAXSET) {
                    if line_no == line {
                        for (style, _text, range) in RangedHighlightIterator::new(
                            &mut states.1,
                            &ops,
                            &text,
                            &self.highlighter,
                        ) {
                            let mut attr = default.clone();
                            if style.font_style.contains(FontStyle::ITALIC) {
                                attr = attr.style(Style::Italic);
                            }
                            if style.font_style.contains(FontStyle::BOLD) {
                                attr = attr.weight(Weight::BOLD);
                            }
                            attr = attr.color(Color::from_rgba8(
                                style.foreground.r,
                                style.foreground.g,
                                style.foreground.b,
                                style.foreground.a,
                            ));

                            attrs.add_span(range, attr);
                        }
                    }
                }

                if line_no & 0xF == 0xF {
                    states_cache.push(states.clone());
                }
            }
        }
    }

    fn apply_layout_styles(
        &self,
        edid: EditorId,
        style: &EditorStyle,
        line: usize,
        layout_line: &mut TextLayoutLine,
    ) {
        self.style
            .apply_layout_styles(edid, style, line, layout_line)
    }

    fn paint_caret(&self, edid: EditorId, line: usize) -> bool {
        self.style.paint_caret(edid, line)
    }
}

fn app_view() -> impl IntoView {
    let global_style = SimpleStylingBuilder::default()
        .wrap(WrapMethod::None)
        .font_family(vec![
            FamilyOwned::Name("Fira Code".to_string()),
            FamilyOwned::Name("Consolas".to_string()),
            FamilyOwned::Monospace,
        ])
        .build();

    let mut style = SyntaxHighlightingStyle::new(Rc::new(global_style));

    let editor = text_editor(
        r#"fn fib(n: i32) -> i32 {
	if n == 0 || n == 1 {
		return n;
	} else {
		return fib(n - 1) + fib(n - 2);
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_fib() {
	    assert_eq!(fib(0), 0);
	    assert_eq!(fib(1), 1);
	    assert_eq!(fib(2), 1);
	    assert_eq!(fib(3), 2);
	    assert_eq!(fib(4), 3);
	    assert_eq!(fib(5), 5);
	}
}
"#,
    );

    let hide_gutter = RwSignal::new(false);

    style.set_doc(editor.doc().clone());
    let editor = editor
        .styling(style)
        .editor_style(default_dark_color)
        .editor_style(move |s| s.hide_gutter(hide_gutter.get()))
        .style(|s| s.size_full());

    let doc = editor.doc();

    let view = stack((
        editor,
        stack((
            button("Clear").action(move || {
                doc.edit_single(
                    Selection::region(0, doc.text().len(), CursorAffinity::Backward),
                    "",
                    EditType::DeleteSelection,
                );
            }),
            button("Gutter").action(move || {
                hide_gutter.update(|hide| *hide = !*hide);
            }),
        ))
        .style(|s| s.width_full().flex_row().items_center().justify_center()),
    ))
    .style(|s| s.size_full().flex_col().items_center().justify_center());

    let id = view.id();
    view.on_key_up(
        Key::Named(NamedKey::F11),
        |m| m.is_empty(),
        move |_| id.inspect(),
    )
}

fn main() {
    floem::launch(app_view)
}
