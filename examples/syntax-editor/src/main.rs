use std::borrow::Cow;
use std::cell::RefCell;
use std::rc::Rc;
use lazy_static::lazy_static;
use syntect::highlighting::{FontStyle, Highlighter, HighlightState, RangedHighlightIterator, ThemeSet};
use syntect::parsing::{ParseState, ScopeStack, SyntaxReference, SyntaxSet};
use floem::{
    cosmic_text:: {
        FamilyOwned
    },
    keyboard::{Key, ModifiersState, NamedKey},
    view::View,
    views::{
        editor::{
            core::{editor::EditType, selection::Selection},
            text::WrapMethod
        },
        stack, text_editor, Decorators,
    },
    widgets::button,
};
use floem::cosmic_text::{Attrs, AttrsList, Stretch, Style, Weight};
use floem::peniko::Color;
use floem::views::editor::color::EditorColor;
use floem::views::editor::core::buffer::rope_text::RopeText;
use floem::views::editor::core::indent::IndentStyle;
use floem::views::editor::Editor;
use floem::views::editor::layout::TextLayoutLine;
use floem::views::editor::text::{Document, RenderWhitespace, SimpleStylingBuilder, Styling};

lazy_static!{
    pub static ref SYNTAXSET: SyntaxSet = SyntaxSet::load_defaults_newlines();
    pub static ref THEMES: ThemeSet = ThemeSet::load_defaults();
}

struct SyntaxHighlightingStyle<'a> {
    pub syntax: &'a SyntaxReference,
    pub highlighter: Highlighter<'a>,
    pub style: Rc<dyn Styling>,
    pub doc: Option<Rc<dyn Document>>,
    pub states: RefCell<Vec<(ParseState, HighlightState)>>
}

impl<'a> SyntaxHighlightingStyle<'a> {
    pub fn new(style: Rc<dyn Styling>) -> Self {
        let theme = &THEMES.themes["base16-ocean.dark"];
        let rust = SYNTAXSET.find_syntax_by_extension("rs").unwrap();
        let highlighter = Highlighter::new(theme);

        SyntaxHighlightingStyle{
            syntax: rust,
            highlighter,
            style,
            doc: None,
            states: RefCell::new(Vec::new())
        }
    }

    pub fn set_doc(&mut self, doc: Rc<dyn Document>) {
        self.doc = Some(doc);
    }

}

impl<'a> Styling for SyntaxHighlightingStyle<'a> {
    fn id(&self) -> u64 {
        self.style.id()
    }

    fn font_size(&self, line: usize) -> usize {
        self.style.font_size(line)
    }

    fn line_height(&self, line: usize) -> f32 {
        self.style.line_height(line)
    }

    fn font_family(&self, line: usize) -> Cow<[FamilyOwned]> {
        self.style.font_family(line)
    }

    fn weight(&self, line: usize) -> Weight {
        self.style.weight(line)
    }

    fn italic_style(&self, line: usize) -> Style {
        self.style.italic_style(line)
    }

    fn stretch(&self, line: usize) -> Stretch {
        self.style.stretch(line)
    }

    fn indent_style(&self) -> IndentStyle {
        self.style.indent_style()
    }

    fn indent_line(&self, line: usize, line_content: &str) -> usize {
        self.style.indent_line(line, line_content)
    }

    fn tab_width(&self, line: usize) -> usize {
        self.style.tab_width(line)
    }

    fn atomic_soft_tabs(&self, line: usize) -> bool {
        self.style.atomic_soft_tabs(line)
    }

    fn apply_attr_styles(&self, line: usize, default: Attrs, attrs: &mut AttrsList) {
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
                        for (style, _text, range) in RangedHighlightIterator::new(&mut states.1, &ops, &text, &self.highlighter).into_iter() {
                            let mut attr = default.clone();
                            if style.font_style.contains(FontStyle::ITALIC) {
                                attr.style = Style::Italic;
                            }
                            if style.font_style.contains(FontStyle::BOLD) {
                                attr.weight = Weight::BOLD;
                            }
                            attr.color = Color::rgba8(style.foreground.r, style.foreground.g, style.foreground.b, style.foreground.a);

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

    fn wrap(&self) -> WrapMethod {
        self.style.wrap()
    }

    fn render_whitespace(&self) -> RenderWhitespace {
        self.style.render_whitespace()
    }

    fn apply_layout_styles(&self, line: usize, layout_line: &mut TextLayoutLine) {
        self.style.apply_layout_styles(line, layout_line)
    }

    fn color(&self, color: EditorColor) -> Color {
        self.style.color(color)
    }

    fn paint_caret(&self, editor: &Editor, line: usize) -> bool {
        self.style.paint_caret(editor, line)
    }
}

fn app_view() -> impl View {
    let global_style = SimpleStylingBuilder::default()
        .wrap(WrapMethod::None)
        .font_family(vec!(FamilyOwned::Name("Fira Code".to_string()), FamilyOwned::Name("Consolas".to_string()), FamilyOwned::Monospace))
        .build_dark();

    let mut style = SyntaxHighlightingStyle::new(Rc::new(global_style));

    let editor = text_editor(r#"fn fib(n: i32) -> i32 {
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
"#);

    style.set_doc(editor.doc().clone());
    let editor = editor.styling(style);

    let gutter = editor.editor().gutter;
    let doc = editor.doc();

    let view = stack((
        editor,
        stack((
            button(|| "Clear").on_click_stop(move |_| {
                doc.edit_single(
                    Selection::region(0, doc.text().len()),
                    "",
                    EditType::DeleteSelection,
                );
            }),
            button(|| "Gutter").on_click_stop(move |_| {
                let a = !gutter.get_untracked();
                gutter.set(a);
            }),
        )).style(|s| s.width_full().flex_row().items_center().justify_center()),
    )).style(|s| s.size_full().flex_col().items_center().justify_center());

    let id = view.id();
    view.on_key_up(
        Key::Named(NamedKey::F11),
        ModifiersState::empty(),
        move |_| id.inspect(),
    )
}

fn main() {
    floem::launch(app_view)
}
