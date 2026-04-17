//! Core style property value trait and implementations.

use floem_reactive::{RwSignal, SignalGet, SignalUpdate as _};
use floem_renderer::text::{FontWeight, LineHeightValue};
use peniko::kurbo::{self, Affine, Stroke};
use peniko::{Brush, Color, Gradient};
use smallvec::SmallVec;
use std::collections::HashSet;
use std::fmt::Debug;
use std::rc::Rc;
use taffy::GridTemplateComponent;

#[cfg(not(target_arch = "wasm32"))]
use std::time::Duration;
#[cfg(target_arch = "wasm32")]
use web_time::Duration;

use taffy::style::{
    AlignContent, AlignItems, BoxSizing, Display, FlexDirection, FlexWrap, Overflow, Position,
};
use taffy::{
    geometry::{MinMax, Size},
    prelude::{GridPlacement, Line},
    style::{LengthPercentage, MaxTrackSizingFunction, MinTrackSizingFunction},
};

use crate::AnyView;
use crate::prelude::ViewTuple;
use crate::style::CursorStyle;
use crate::theme::StyleThemeExt;
use crate::unit::{Length, LengthAuto, Pct, Pt};
use crate::view::{IntoView, View};
use crate::views::{
    ButtonClass, ContainerExt, Decorators, Empty, Label, Stack, TabSelectorClass, dyn_view, svg,
    tab,
};

use super::{
    FontSize, InspectorRender, PropDebugView, ResponsiveSelectors, StructuralSelectors, Style,
    StyleDebugGroupInfo, StyleKey, StyleKeyInfo, StylePropRef, Transition, TransitionDebugViewExt,
};
use std::any::Any;

use crate::no_debug_view;

no_debug_view!(
    i32,
    bool,
    f32,
    u16,
    usize,
    f64,
    Overflow,
    Display,
    Position,
    FlexDirection,
    FlexWrap,
    AlignItems,
    BoxSizing,
    AlignContent,
    GridTemplateComponent<String>,
    MinTrackSizingFunction,
    MaxTrackSizingFunction,
    taffy::GridAutoFlow,
    GridPlacement,
    String,
    crate::text::Alignment,
    LineHeightValue,
    Size<LengthPercentage>,
    super::Angle,
    super::AnchorAbout,
);

pub struct ContextValue<T> {
    pub(crate) eval: Rc<dyn Fn(&Style) -> T>,
}

impl<T> Clone for ContextValue<T> {
    fn clone(&self) -> Self {
        Self {
            eval: self.eval.clone(),
        }
    }
}

impl<T> std::fmt::Debug for ContextValue<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ContextValue(..)")
    }
}

impl<T> PartialEq for ContextValue<T> {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.eval, &other.eval)
    }
}

impl<T> Eq for ContextValue<T> {}

impl<T> ContextValue<T> {
    pub(crate) fn new(eval: impl Fn(&Style) -> T + 'static) -> Self {
        Self {
            eval: Rc::new(eval),
        }
    }

    pub fn resolve(&self, style: &Style) -> T {
        floem_reactive::Runtime::with_effect(style.effect_context.clone(), || {
            // todo use context
            (self.eval)(style)
        })
    }

    pub fn map<U>(self, f: impl Fn(T) -> U + 'static) -> ContextValue<U>
    where
        T: 'static,
    {
        let eval = self.eval;
        ContextValue::new(move |style| f(eval(style)))
    }
}

pub use floem_style::prop_value::StylePropValue;
#[allow(unused_imports)]
pub use floem_style::prop_value::{hash_f32, hash_f64, hash_value};

// Primitive, taffy, collection, peniko, text, and unit `StylePropValue` impls
// live in `floem_style::value_impls` (moved there to satisfy the orphan rule).

impl<T, M> PropDebugView for MinMax<T, M> {}
impl<T> PropDebugView for Line<T> {}

pub use floem_style::{ObjectFit, ObjectPosition};

impl PropDebugView for ObjectFit {
    fn debug_view(&self, r: &dyn InspectorRender) -> Option<Box<dyn Any>> {
        Some(r.object_fit(*self))
    }
}

impl PropDebugView for ObjectPosition {
    fn debug_view(&self, r: &dyn InspectorRender) -> Option<Box<dyn Any>> {
        Some(r.object_position(self))
    }
}

impl<A: smallvec::Array> PropDebugView for SmallVec<A>
where
    <A as smallvec::Array>::Item: StylePropValue + PropDebugView,
{
    fn debug_view(&self, r: &dyn InspectorRender) -> Option<Box<dyn Any>> {
        if self.is_empty() {
            return Some(r.text("smallvec\n[]"));
        }

        let count = self.len();
        let is_spilled = self.spilled();

        // Create a preview that shows count and whether it has spilled to heap
        let preview = Label::derived(move || {
            if is_spilled {
                format!("smallvec\n[{}] (heap)", count)
            } else {
                format!("smallvec\n[{}] (inline)", count)
            }
        })
        .style(|s| {
            s.padding(2.0)
                .padding_horiz(6.0)
                .items_center()
                .justify_center()
                .text_align(parley::Alignment::Center)
                .border(1.)
                .border_radius(5.0)
                .margin_left(6.0)
                .with_theme(|s, t| s.color(t.text()).border_color(t.border()))
                .with::<FontSize>(|s, fs| s.font_size(fs.def(|fs| fs * 0.85)))
        });

        // Render each item via the renderer; downcast back to concrete
        // view type so the tooltip closure can own them.
        let item_views: Vec<Box<dyn View>> = self
            .iter()
            .enumerate()
            .map(|(i, item)| {
                let index_label = Label::new(format!("[{}]", i))
                    .style(|s| s.with_theme(|s, t| s.color(t.text_muted())));

                let item_view: Box<dyn View> = item
                    .debug_view(r)
                    .and_then(|any| any.downcast::<Box<dyn View>>().ok().map(|b| *b))
                    .unwrap_or_else(|| {
                        Label::new(format!("{:?}", item))
                            .style(|s| s.flex_grow(1.0))
                            .into_any()
                    });

                Stack::new((index_label, item_view))
                    .style(|s| s.items_center().gap(8.0).padding(4.0))
                    .into_any()
            })
            .collect();

        let tooltip = Stack::vertical_from_iter(item_views).style(|s| s.gap(4.0));

        let view: Box<dyn View> = Stack::new((preview, tooltip))
            .style(|s| s.gap(8.0))
            .into_any();
        Some(Box::new(view))
    }
}
impl PropDebugView for FontWeight {
    fn debug_view(&self, _r: &dyn InspectorRender) -> Option<Box<dyn Any>> {
        let clone = *self;
        let view: Box<dyn View> = format!("{clone:?}")
            .style(move |s| s.font_weight(clone))
            .into_any();
        Some(Box::new(view))
    }
}
impl PropDebugView for crate::text::FontStyle {
    fn debug_view(&self, _r: &dyn InspectorRender) -> Option<Box<dyn Any>> {
        let clone = *self;
        let view: Box<dyn View> = format!("{clone:?}")
            .style(move |s| s.font_style(clone))
            .into_any();
        Some(Box::new(view))
    }
}
impl<T: PropDebugView> PropDebugView for Option<T> {
    fn debug_view(&self, r: &dyn InspectorRender) -> Option<Box<dyn Any>> {
        self.as_ref().and_then(|v| v.debug_view(r))
    }
}
impl<T: StylePropValue + PropDebugView + 'static> PropDebugView for Vec<T> {
    fn debug_view(&self, r: &dyn InspectorRender) -> Option<Box<dyn Any>> {
        if self.is_empty() {
            let view: Box<dyn View> = Label::new("[]")
                .style(|s| s.with_theme(|s, t| s.color(t.text_muted())))
                .into_any();
            return Some(Box::new(view));
        }

        let item_views: Vec<Box<dyn View>> = self
            .iter()
            .enumerate()
            .map(|(i, item)| {
                let index_label = Label::new(format!("[{}]", i))
                    .style(|s| s.with_theme(|s, t| s.color(t.text_muted())));

                let item_view: Box<dyn View> = item
                    .debug_view(r)
                    .and_then(|any| any.downcast::<Box<dyn View>>().ok().map(|b| *b))
                    .unwrap_or_else(|| {
                        Label::new(format!("{:?}", item))
                            .style(|s| s.flex_grow(1.0))
                            .into_any()
                    });

                Stack::new((index_label, item_view))
                    .style(|s| s.items_center().gap(8.0).padding(4.0))
                    .into_any()
            })
            .collect();

        let view: Box<dyn View> = Stack::vertical_from_iter(item_views)
            .style(|s| s.gap(4.0))
            .into_any();
        Some(Box::new(view))
    }
}
impl PropDebugView for Pt {
    fn debug_view(&self, r: &dyn InspectorRender) -> Option<Box<dyn Any>> {
        Some(r.text(&format!("{} pt", self.0)))
    }
}
#[allow(deprecated)]
impl PropDebugView for super::unit::Px {
    fn debug_view(&self, r: &dyn InspectorRender) -> Option<Box<dyn Any>> {
        Pt(self.0).debug_view(r)
    }
}
impl PropDebugView for Pct {
    fn debug_view(&self, r: &dyn InspectorRender) -> Option<Box<dyn Any>> {
        Some(r.text(&format!("{}%", self.0)))
    }
}
impl PropDebugView for LengthAuto {
    fn debug_view(&self, r: &dyn InspectorRender) -> Option<Box<dyn Any>> {
        let label = match self {
            Self::Pt(v) => format!("{v} pt"),
            Self::Pct(v) => format!("{v}%"),
            Self::Em(v) => format!("{v} em"),
            Self::Lh(v) => format!("{v} lh"),
            Self::Auto => "auto".to_string(),
        };
        Some(r.text(&label))
    }
}
#[allow(deprecated)]
impl PropDebugView for super::unit::PxPctAuto {
    fn debug_view(&self, r: &dyn InspectorRender) -> Option<Box<dyn Any>> {
        LengthAuto::from(*self).debug_view(r)
    }
}
impl PropDebugView for Length {
    fn debug_view(&self, r: &dyn InspectorRender) -> Option<Box<dyn Any>> {
        let label = match self {
            Self::Pt(v) => format!("{v} pt"),
            Self::Pct(v) => format!("{v}%"),
            Self::Em(v) => format!("{v} em"),
            Self::Lh(v) => format!("{v} lh"),
        };
        Some(r.text(&label))
    }
}
#[allow(deprecated)]
impl PropDebugView for super::unit::PxPct {
    fn debug_view(&self, r: &dyn InspectorRender) -> Option<Box<dyn Any>> {
        Length::from(*self).debug_view(r)
    }
}

pub(crate) fn views(views: impl ViewTuple) -> Vec<AnyView> {
    views.into_views()
}

impl PropDebugView for Color {
    fn debug_view(&self, r: &dyn InspectorRender) -> Option<Box<dyn Any>> {
        Some(r.color(*self))
    }
}

impl PropDebugView for Gradient {
    fn debug_view(&self, r: &dyn InspectorRender) -> Option<Box<dyn Any>> {
        Some(r.gradient(self))
    }
}

pub use floem_style::StrokeWrap;

impl PropDebugView for Stroke {
    fn debug_view(&self, r: &dyn InspectorRender) -> Option<Box<dyn Any>> {
        Some(r.stroke(self))
    }
}
impl PropDebugView for Brush {
    fn debug_view(&self, r: &dyn InspectorRender) -> Option<Box<dyn Any>> {
        match self {
            Brush::Solid(_) | Brush::Gradient(_) => Some(r.brush(self)),
            Brush::Image(_) => None,
        }
    }
}
impl PropDebugView for Duration {
    fn debug_view(&self, _r: &dyn InspectorRender) -> Option<Box<dyn Any>> {
        None
    }
}

impl PropDebugView for kurbo::Rect {
    fn debug_view(&self, r: &dyn InspectorRender) -> Option<Box<dyn Any>> {
        Some(r.rect(self))
    }
}

impl PropDebugView for Affine {
    fn debug_view(&self, r: &dyn InspectorRender) -> Option<Box<dyn Any>> {
        Some(r.affine(self))
    }
}

/// Internal storage for style property values in the style map.
///
/// Unlike `StyleValue<T>` which is used in the public API, `StyleMapValue<T>`
/// is the internal representation stored in the style hashmap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StyleMapValue<T> {
    /// Value inserted by animation interpolation
    Animated(T),
    /// Value set directly
    Val(T),
    /// Value resolved from inherited context when the property is read.
    Context(ContextValue<T>),
    /// Use the default value for the style, typically from the underlying `ComputedStyle`
    Unset,
}

/// The value for a [`Style`] property in the public API.
///
/// This represents the result of reading a style property, with additional
/// states like `Base` that indicate inheritance from parent styles.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum StyleValue<T> {
    /// Value resolved from inherited context when the property is read.
    Context(ContextValue<T>),
    /// Value inserted by animation interpolation
    Animated(T),
    /// Value set directly
    Val(T),
    /// Use the default value for the style, typically from the underlying `ComputedStyle`.
    Unset,
    /// Use whatever the base style is. For an overriding style like hover, this uses the base
    /// style. For the base style, this is equivalent to `Unset`.
    #[default]
    Base,
}

impl<T: 'static> StyleValue<T> {
    pub fn map<U>(self, f: impl Fn(T) -> U + 'static) -> StyleValue<U> {
        match self {
            Self::Context(x) => StyleValue::Context(x.map(f)),
            Self::Val(x) => StyleValue::Val(f(x)),
            Self::Animated(x) => StyleValue::Animated(f(x)),
            Self::Unset => StyleValue::Unset,
            Self::Base => StyleValue::Base,
        }
    }

    pub fn unwrap_or(self, default: T) -> T {
        match self {
            Self::Context(_) => default,
            Self::Val(x) => x,
            Self::Animated(x) => x,
            Self::Unset => default,
            Self::Base => default,
        }
    }

    pub fn unwrap_or_else(self, f: impl FnOnce() -> T) -> T {
        match self {
            Self::Context(_) => f(),
            Self::Val(x) => x,
            Self::Animated(x) => x,
            Self::Unset => f(),
            Self::Base => f(),
        }
    }

    pub fn as_mut(&mut self) -> Option<&mut T> {
        match self {
            Self::Context(_) => None,
            Self::Val(x) => Some(x),
            Self::Animated(x) => Some(x),
            Self::Unset => None,
            Self::Base => None,
        }
    }
}

impl<T> From<T> for StyleValue<T> {
    fn from(x: T) -> Self {
        Self::Val(x)
    }
}

impl<T> From<ContextValue<T>> for StyleValue<T> {
    fn from(x: ContextValue<T>) -> Self {
        Self::Context(x)
    }
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
            let mut value_view = (prop.info().debug_view)(&*value)
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

impl Style {
    pub fn debug_view(&self, direct_style: Option<&Style>) -> Box<dyn View> {
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
