//! Style-debug view impls for floem `Style`.
//!
//! `ContextValue<T>`, `StyleMapValue<T>`, and `StyleValue<T>` now live in the
//! `floem_style` crate; they are re-exported from `crate::style` for
//! compatibility.

use floem_reactive::{RwSignal, SignalGet, SignalUpdate as _};
use std::collections::HashSet;
use std::rc::Rc;
use taffy::style::FlexDirection;

use crate::AnyView;
use crate::prelude::ViewTuple;
use crate::style::CursorStyle;
use crate::theme::StyleThemeExt;
use crate::view::{IntoView, View};
use crate::views::{
    ButtonClass, ContainerExt, Decorators, Empty, Label, Stack, TabSelectorClass, dyn_view, svg,
    tab,
};

use super::{
    FontSize, ResponsiveSelectors, StructuralSelectors, Style, StyleDebugGroupInfo, StyleKey,
    StyleKeyInfo, StylePropRef, Transition, TransitionDebugViewExt,
};

pub use floem_style::prop_value::StylePropValue;
#[allow(unused_imports)]
pub use floem_style::prop_value::{hash_f32, hash_f64, hash_value};

// Primitive, taffy, collection, peniko, text, and unit `StylePropValue` impls
// live in `floem_style::value_impls` (moved there to satisfy the orphan rule).
// `PropDebugView` impls for types owned by external crates
// (Option, Vec, SmallVec, MinMax, Line, FontWeight, FontStyle, primitives via
// `no_debug_view!`) likewise live in `floem_style::value_impls`.

pub(crate) fn views(views: impl ViewTuple) -> Vec<AnyView> {
    views.into_views()
}

fn short_style_name(name: &str) -> String {
    name.strip_prefix("floem::style::")
        .unwrap_or(name)
        .to_string()
}

struct StyleDebugRow {
    render: Rc<dyn Fn(bool) -> AnyView>,
    is_empty: bool,
}

fn effective_inherited_debug_groups(
    style: &Style,
    parent_groups: &HashSet<StyleKey>,
) -> HashSet<StyleKey> {
    let mut groups = parent_groups.clone();
    for key in style.map.keys() {
        if let StyleKeyInfo::DebugGroup(info) = key.info {
            if style.debug_group_enabled(*key) {
                if info.inherited {
                    groups.insert(*key);
                }
            } else {
                groups.remove(key);
            }
        }
    }
    groups
}

fn style_debug_active_groups(
    style: &Style,
    inherited_groups: &HashSet<StyleKey>,
) -> Vec<&'static StyleDebugGroupInfo> {
    let mut groups = style
        .map
        .keys()
        .filter_map(|key| match key.info {
            StyleKeyInfo::DebugGroup(info) if style.debug_group_enabled(*key) => Some(info),
            _ => None,
        })
        .collect::<Vec<_>>();

    for key in inherited_groups {
        if let StyleKeyInfo::DebugGroup(info) = key.info
            && !style.map.contains_key(key)
        {
            groups.push(info);
        }
    }

    groups.sort_unstable_by_key(|info| short_style_name((info.name)()));
    groups.dedup_by_key(|info| (info.name)());
    groups
}

fn style_debug_is_empty(style: &Style, inherited_groups: &HashSet<StyleKey>) -> bool {
    let mut hidden_props = HashSet::new();

    for info in style_debug_active_groups(style, inherited_groups) {
        let members = (info.member_props)();
        let present = members
            .iter()
            .copied()
            .filter(|key| style.map.contains_key(key) && !hidden_props.contains(key))
            .collect::<Vec<_>>();
        if !present.is_empty() {
            hidden_props.extend(present);
        }
    }

    if style
        .map
        .iter()
        .any(|(key, _)| matches!(key.info, StyleKeyInfo::Prop(..)) && !hidden_props.contains(key))
    {
        return false;
    }

    if style.map.iter().any(|(key, value)| match key.info {
        StyleKeyInfo::Selector(..) | StyleKeyInfo::Class(..) => {
            value.downcast_ref::<Style>().is_some_and(|nested| {
                !style_debug_is_empty(
                    nested,
                    &effective_inherited_debug_groups(nested, inherited_groups),
                )
            })
        }
        _ => false,
    }) {
        return false;
    }

    for value in style.map.values() {
        if let Some(rules) = value.downcast_ref::<StructuralSelectors>()
            && rules.0.iter().any(|(_, nested)| {
                !style_debug_is_empty(
                    nested,
                    &effective_inherited_debug_groups(nested, inherited_groups),
                )
            })
        {
            return false;
        }
        if let Some(rules) = value.downcast_ref::<ResponsiveSelectors>()
            && rules.0.iter().any(|(_, nested)| {
                !style_debug_is_empty(
                    nested,
                    &effective_inherited_debug_groups(nested, inherited_groups),
                )
            })
        {
            return false;
        }
    }

    true
}

fn debug_name_cell(name: String, is_direct: bool, indent: usize) -> AnyView {
    let indent = (indent as f64) * 16.0;
    let name = if is_direct {
        Label::new(name).into_any()
    } else {
        Stack::new((
            "Inherited".style(|s| {
                s.margin_right(5.0)
                    .border(1.)
                    .border_radius(5.0)
                    .with_theme(|s, t| s.color(t.text_muted()).border_color(t.border()))
                    .padding_horiz(4.0)
                    .with::<FontSize>(|s, fs| s.font_size(fs.def(|fs| fs * 0.8)))
            }),
            Label::new(name),
        ))
        .style(|s| s.items_center().gap(6.0))
        .into_any()
    };

    name.container()
        .style(move |s| {
            s.padding_left(indent)
                .min_width(170.)
                .padding_right(5.0)
                .flex_direction(FlexDirection::RowReverse)
        })
        .into_any()
}

fn style_debug_prop_row(
    style: &Style,
    prop: StylePropRef,
    value: &Rc<dyn std::any::Any>,
    is_direct: bool,
    indent: usize,
) -> StyleDebugRow {
    let style = style.clone();
    let value = value.clone();
    let name = short_style_name(&format!("{:?}", prop.key));
    StyleDebugRow {
        render: Rc::new(move |_| {
            let mut value_view = (prop.info().debug_view)(
                &*value,
                &crate::style::FloemInspectorRender,
            )
            .and_then(|any| any.downcast::<Box<dyn View>>().ok().map(|b| *b))
            .unwrap_or_else(|| Label::new((prop.info().debug_any)(&*value)).into_any());

            if let Some(transition) = style
                .map
                .get(&prop.info().transition_key)
                .and_then(|v| v.downcast_ref::<Transition>())
            {
                value_view = Stack::vertical((
                    value_view,
                    Stack::new((
                        "Transition".style(|s| {
                            s.margin_top(4.0)
                                .margin_right(5.0)
                                .border(1.)
                                .border_radius(5.0)
                                .padding_horiz(4.0)
                                .with_theme(|s, t| s.color(t.text_muted()).border_color(t.border()))
                                .with::<FontSize>(|s, fs| s.font_size(fs.def(|fs| fs * 0.8)))
                        }),
                        transition.debug_view(),
                    ))
                    .style(|s| s.items_center().gap(6.0)),
                ))
                .into_any();
            }

            Stack::new((debug_name_cell(name.clone(), is_direct, indent), value_view))
                .style(|s| s.items_center().width_full().padding_vert(4.0).gap(8.0))
                .into_any()
        }),
        is_empty: false,
    }
}

fn style_debug_group_row<V>(
    name: String,
    value_view: V,
    is_direct: bool,
    indent: usize,
) -> StyleDebugRow
where
    V: Fn() -> AnyView + 'static,
{
    StyleDebugRow {
        render: Rc::new(move |_| {
            Stack::new((
                debug_name_cell(name.clone(), is_direct, indent),
                value_view(),
            ))
            .style(|s| s.items_center().width_full().padding_vert(4.0).gap(8.0))
            .into_any()
        }),
        is_empty: false,
    }
}

fn style_debug_section(title: String, child: StyleDebugRow, indent: usize) -> StyleDebugRow {
    let expanded = RwSignal::new(false);
    let title_text = title.clone();
    let child_is_empty = child.is_empty;
    let chevron = move || {
        if expanded.get() {
            svg(
                r#"<svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor"><path d="M4.427 6.427l3.396 3.396a.25.25 0 00.354 0l3.396-3.396A.25.25 0 0011.396 6H4.604a.25.25 0 00-.177.427z"/></svg>"#,
            )
        } else {
            svg(
                r#"<svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor"><path d="M6.427 4.427l3.396 3.396a.25.25 0 010 .354l-3.396 3.396A.25.25 0 016 11.396V4.604a.25.25 0 01.427-.177z"/></svg>"#,
            )
        }
        .style(|s| s.size_full().with_theme(|s, t| s.color(t.text())))
    };

    StyleDebugRow {
        render: Rc::new(move |row_is_base| {
            let child_render = child.render.clone();
            Stack::vertical((
                Stack::new((
                    dyn_view(chevron)
                        .class(ButtonClass)
                        .style(|s| s.size(16.0, 16.0).padding(0.)),
                    Label::new(title_text.clone()).style(|s| {
                        s.font_bold()
                            .cursor(CursorStyle::Pointer)
                            .with_theme(|s, t| s.color(t.primary()))
                    }),
                    Label::new("empty")
                        .style(|s| {
                            s.padding_horiz(6.0)
                                .border(1.)
                                .border_radius(999.0)
                                .with_theme(|s, t| s.color(t.text_muted()).border_color(t.border()))
                                .with::<FontSize>(|s, fs| s.font_size(fs.def(|fs| fs * 0.75)))
                        })
                        .style(move |s| s.apply_if(!child_is_empty, |s| s.hide())),
                ))
                .style(move |s| {
                    s.items_center()
                        .gap(6.0)
                        .padding_left((indent as f64) * 16.0)
                        .cursor(super::CursorStyle::Pointer)
                })
                .on_event_stop(crate::event::listener::Click, move |_cx, _event| {
                    expanded.update(|value| *value = !*value)
                }),
                dyn_view(move || {
                    if expanded.get() {
                        child_render(!row_is_base)
                            .style(|s| s.padding_left(12.0))
                            .into_any()
                    } else {
                        Empty::new().into_any()
                    }
                })
                .into_any(),
            ))
            .style(|s| s.gap(6.0).width_full().padding_vert(4.0))
            .into_any()
        }),
        is_empty: false,
    }
}

fn style_debug_sections(
    title: &str,
    children: Vec<StyleDebugRow>,
    indent: usize,
) -> Option<StyleDebugRow> {
    if children.is_empty() {
        return None;
    }

    Some(style_debug_section(
        title.to_string(),
        StyleDebugRow {
            render: Rc::new(move |start_with_base| style_debug_rows(&children, start_with_base)),
            is_empty: false,
        },
        indent,
    ))
}

fn style_debug_style_section(
    title: String,
    style: &Style,
    inherited_groups: &HashSet<StyleKey>,
    indent: usize,
) -> StyleDebugRow {
    let nested_inherited = effective_inherited_debug_groups(style, inherited_groups);
    style_debug_section(
        title,
        style_debug_body(style, None, &nested_inherited, indent + 1),
        indent,
    )
}

fn style_debug_prop_rows(
    style: &Style,
    direct_keys: Option<&HashSet<StyleKey>>,
    inherited_groups: &HashSet<StyleKey>,
    indent: usize,
) -> Vec<StyleDebugRow> {
    let mut rows: Vec<StyleDebugRow> = Vec::new();
    let mut hidden_props = HashSet::new();

    for info in style_debug_active_groups(style, inherited_groups) {
        let members = (info.member_props)();
        let present = members
            .iter()
            .copied()
            .filter(|key| style.map.contains_key(key) && !hidden_props.contains(key))
            .collect::<Vec<_>>();
        if present.is_empty() {
            continue;
        }
        hidden_props.extend(present);
        if (info.debug_view)(style as &dyn std::any::Any).is_some() {
            let info = info.clone();
            let style = style.clone();
            rows.push(style_debug_group_row(
                short_style_name((info.name)()),
                move || {
                    (info.debug_view)(&style as &dyn std::any::Any)
                        .and_then(|any| any.downcast::<Box<dyn View>>().ok().map(|b| *b))
                        .unwrap_or_else(|| Label::new("empty").into_any())
                        .into_any()
                },
                true,
                indent,
            ));
        }
    }

    let mut props = style
        .map
        .iter()
        .filter_map(|(key, value)| match key.info {
            StyleKeyInfo::Prop(..) if !hidden_props.contains(key) => {
                Some((StylePropRef { key: *key }, value))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    props.sort_unstable_by_key(|(prop, _)| short_style_name(&format!("{:?}", prop.key)));

    for (prop, value) in props {
        let is_direct = direct_keys
            .as_ref()
            .is_none_or(|keys| keys.contains(&prop.key));
        rows.push(style_debug_prop_row(style, prop, value, is_direct, indent));
    }

    rows
}

fn style_debug_selector_rows(
    style: &Style,
    inherited_groups: &HashSet<StyleKey>,
    indent: usize,
) -> Vec<StyleDebugRow> {
    let mut selector_rows: Vec<StyleDebugRow> = Vec::new();
    let mut selectors = style
        .map
        .iter()
        .filter_map(|(key, value)| match key.info {
            StyleKeyInfo::Selector(selector) => Some((selector.debug_string(), value)),
            _ => None,
        })
        .collect::<Vec<_>>();
    selectors.sort_unstable_by(|a, b| a.0.cmp(&b.0));
    for (name, value) in selectors {
        if let Some(nested_style) = value.downcast_ref::<Style>() {
            selector_rows.push(style_debug_style_section(
                name,
                nested_style,
                inherited_groups,
                indent,
            ));
        }
    }

    for value in style.map.values() {
        if let Some(rules) = value.downcast_ref::<StructuralSelectors>() {
            for (selector, nested_style) in &rules.0 {
                selector_rows.push(style_debug_style_section(
                    format!("Structural: {selector:?}"),
                    nested_style,
                    inherited_groups,
                    indent,
                ));
            }
        }
        if let Some(rules) = value.downcast_ref::<ResponsiveSelectors>() {
            for (selector, nested_style) in &rules.0 {
                selector_rows.push(style_debug_style_section(
                    format!("Responsive: {selector:?}"),
                    nested_style,
                    inherited_groups,
                    indent,
                ));
            }
        }
    }

    selector_rows
}

fn style_debug_class_rows(
    style: &Style,
    inherited_groups: &HashSet<StyleKey>,
    indent: usize,
) -> Vec<StyleDebugRow> {
    let mut class_rows: Vec<StyleDebugRow> = Vec::new();
    let mut classes = style
        .map
        .iter()
        .filter_map(|(key, value)| match key.info {
            StyleKeyInfo::Class(info) => Some((short_style_name((info.name)()), value)),
            _ => None,
        })
        .collect::<Vec<_>>();
    classes.sort_unstable_by(|a, b| a.0.cmp(&b.0));
    for (name, value) in classes {
        if let Some(nested_style) = value.downcast_ref::<Style>() {
            class_rows.push(style_debug_style_section(
                name,
                nested_style,
                inherited_groups,
                indent,
            ));
        }
    }
    class_rows
}

fn style_debug_body(
    style: &Style,
    direct_keys: Option<&HashSet<StyleKey>>,
    inherited_groups: &HashSet<StyleKey>,
    indent: usize,
) -> StyleDebugRow {
    let style = style.clone();
    let inherited_groups = inherited_groups.clone();
    let is_empty = style_debug_is_empty(&style, &inherited_groups);
    let direct_keys = direct_keys.cloned();
    StyleDebugRow {
        render: Rc::new(move |start_with_base| {
            let mut rows =
                style_debug_prop_rows(&style, direct_keys.as_ref(), &inherited_groups, indent);
            if let Some(selectors_section) = style_debug_sections(
                "Selectors",
                style_debug_selector_rows(&style, &inherited_groups, indent),
                indent,
            ) {
                rows.push(selectors_section);
            }
            if let Some(classes_section) = style_debug_sections(
                "Classes",
                style_debug_class_rows(&style, &inherited_groups, indent),
                indent,
            ) {
                rows.push(classes_section);
            }

            if rows.is_empty() {
                return Label::new("empty")
                    .style(|s| s.with_theme(|s, t| s.color(t.text_muted())))
                    .into_any();
            }

            style_debug_rows(&rows, start_with_base)
        }),
        is_empty,
    }
}

fn style_debug_rows(rows: &[StyleDebugRow], start_with_base: bool) -> AnyView {
    Stack::vertical_from_iter(rows.iter().enumerate().map(|(idx, row)| {
        let is_base = if start_with_base {
            idx.is_multiple_of(2)
        } else {
            !idx.is_multiple_of(2)
        };
        (row.render)(is_base).style(move |s| {
            s.width_full().padding_horiz(4.0).with_theme(move |s, t| {
                s.apply_if(is_base, |s| s.background(t.bg_base()))
                    .apply_if(!is_base, |s| s.background(t.bg_elevated()))
            })
        })
    }))
    .style(|s| s.gap(4.0).width_full())
    .into_any()
}

impl super::StyleDebugViewExt for Style {
    fn debug_view(&self, direct_style: Option<&Style>) -> Box<dyn View> {
        let direct_keys =
            direct_style.map(|style| style.map.keys().copied().collect::<HashSet<_>>());
        let style = self.clone();
        let inherited_groups = effective_inherited_debug_groups(&style, &HashSet::new());
        let selected_tab = RwSignal::new(0);
        let tab_item = move |name, index| {
            Label::new(name)
                .class(TabSelectorClass)
                .action(move || selected_tab.set(index))
                .style(move |s| s.set_selected(selected_tab.get() == index))
        };
        let tabs = (
            tab_item("View Style", 0),
            tab_item("Selectors", 1),
            tab_item("Classes", 2),
        )
            .h_stack()
            .style(|s| s.with_theme(|s, t| s.background(t.bg_base())));
        let direct_keys_for_body = direct_keys.clone();
        let style_for_body = style.clone();
        let style_for_selectors = style.clone();
        let style_for_classes = style.clone();
        Stack::vertical((
            tabs,
            tab(
                move || Some(selected_tab.get()),
                move || [0, 1, 2],
                |it| *it,
                move |it| match it {
                    0 => {
                        let rows = style_debug_prop_rows(
                            &style_for_body,
                            direct_keys_for_body.as_ref(),
                            &inherited_groups,
                            0,
                        );
                        if rows.is_empty() {
                            Label::new("empty")
                                .style(|s| s.with_theme(|s, t| s.color(t.text_muted())))
                                .into_any()
                        } else {
                            style_debug_rows(&rows, true)
                        }
                    }
                    1 => {
                        let rows =
                            style_debug_selector_rows(&style_for_selectors, &inherited_groups, 0);
                        if rows.is_empty() {
                            Label::new("empty")
                                .style(|s| s.with_theme(|s, t| s.color(t.text_muted())))
                                .into_any()
                        } else {
                            style_debug_rows(&rows, true)
                        }
                    }
                    2 => {
                        let rows = style_debug_class_rows(&style_for_classes, &inherited_groups, 0);
                        if rows.is_empty() {
                            Label::new("empty")
                                .style(|s| s.with_theme(|s, t| s.color(t.text_muted())))
                                .into_any()
                        } else {
                            style_debug_rows(&rows, true)
                        }
                    }
                    _ => Label::new("empty").into_any(),
                },
            ),
        ))
        .style(|s| s.width_full().gap(6.0))
        .into_any()
    }
}
