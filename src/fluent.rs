use std::borrow::Cow;
use std::pin::Pin;
use std::rc::Rc;

use crate::style::{CustomStylable, CustomStyle, Style, StylePropValue};
use crate::view_state::{Stack, StackOffset};
use crate::views::{Decorators, static_label};
use crate::{AnyView, IntoView, View, ViewId, prop, prop_extractor, style_class};
use floem_reactive::create_updater;
use floem_renderer::text::Align;
use fluent_bundle::{FluentBundle, FluentResource};

pub use fluent_bundle::FluentArgs;
pub use fluent_bundle::types::FluentValue;
use smallvec::smallvec;
pub use unic_langid::LanguageIdentifier;

#[derive(Clone)]
pub struct LanguageMap(pub im_rc::HashMap<LanguageIdentifier, Rc<FluentBundle<FluentResource>>>);
impl std::ops::Deref for LanguageMap {
    type Target = im_rc::HashMap<LanguageIdentifier, Rc<FluentBundle<FluentResource>>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl std::ops::DerefMut for LanguageMap {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
impl std::fmt::Debug for LanguageMap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_map()
            .entries(self.0.keys().map(|lang_id| (lang_id, "<FluentBundle>")))
            .finish()
    }
}
impl PartialEq for LanguageMap {
    fn eq(&self, other: &Self) -> bool {
        if self.0.len() != other.0.len() {
            return false;
        }

        self.0.keys().all(|key| {
            other.0.contains_key(key)
                && Rc::ptr_eq(self.0.get(key).unwrap(), other.0.get(key).unwrap())
        })
    }
}
impl StylePropValue for LanguageMap {
    fn debug_view(&self) -> Option<AnyView> {
        use crate::prelude::*;

        let languages: Vec<String> = self.0.keys().map(|lang_id| lang_id.to_string()).collect();

        let count = languages.len();

        let view = stack((
            format!("Languages ({count})").style(|s| {
                s.font_size(12.0)
                    .font_weight(floem_renderer::text::Weight::SEMIBOLD)
            }),
            v_stack_from_iter(languages.into_iter().map(|lang| {
                lang.style(|s| {
                    s.font_size(11.0)
                        .color(Color::WHITE.with_alpha(0.7))
                        .width_full()
                        .items_center()
                        .justify_center()
                        .text_align(Align::Center)
                })
            }))
            .style(|s| s.gap(2.0).width_full()),
        ))
        .style(|s| {
            s.flex_row()
                .gap(8.0)
                .items_center()
                .padding(8.0)
                .border(1.)
                .border_color(palette::css::WHITE.with_alpha(0.3))
                .border_radius(6.0)
                .min_width(120.0)
        });

        Some(view.into_any())
    }
}
impl StylePropValue for LanguageIdentifier {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        Some(crate::views::text(format!("{self:?}")).into_any())
    }

    fn interpolate(&self, _other: &Self, _value: f64) -> Option<Self> {
        None
    }

    fn combine(&self, _other: &Self) -> crate::style::CombineResult<Self> {
        crate::style::CombineResult::Other
    }
}
impl LanguageMap {
    pub fn from_resources<'a, I>(resources: I) -> Result<Self, Box<dyn std::error::Error>>
    where
        I: IntoIterator<Item = (&'a str, &'a str)>,
    {
        let mut map = im_rc::HashMap::new();

        for (lang_id, resource_str) in resources {
            let lang_id = lang_id.parse::<LanguageIdentifier>()?;
            let resource = FluentResource::try_new(resource_str.to_string())
                .map_err(|(_, errs)| format!("Failed to parse Fluent resource: {:?}", errs))?;

            let mut bundle = FluentBundle::new(vec![lang_id.clone()]);
            bundle
                .add_resource(resource)
                .map_err(|errs| format!("Failed to add resource to bundle: {:?}", errs))?;

            map.insert(lang_id, Rc::new(bundle));
        }

        Ok(Self(map))
    }
}

prop!(pub L10nLanguage: Option<LanguageIdentifier> { inherited } = sys_locale::get_locale().and_then(|l| l.parse().ok()));
prop!(pub L10nFallback: Option<String> {} = None);
prop!(pub L10nBundle: LanguageMap { inherited } = LanguageMap(im_rc::HashMap::new()));

prop_extractor! {
    LanguageExtractor {
        language: L10nLanguage,
        bundle: L10nBundle,
    }
}
prop_extractor! {
    FallBackExtractor {
        fallback: L10nFallback,
    }
}

style_class!(pub L10nClass);

pub struct L10n {
    id: ViewId,
    key: String,
    args: FluentArgs<'static>,
    arg_keys: Pin<Box<crate::view_state::Stack<String>>>, // Pinned allocation
    label_id: ViewId,
    language: LanguageExtractor,
    fallback: FallBackExtractor,
    has_format_value: bool,
}

impl L10n {
    pub fn new(key: impl Into<String>) -> Self {
        let key: String = key.into();
        let label = static_label(key.clone());
        let label_id = label.id();
        let id = ViewId::new();
        id.add_child(label.into_any());
        Self {
            id,
            label_id,
            key,
            args: FluentArgs::new(),
            arg_keys: Box::pin(Stack { stack: smallvec![] }),
            language: Default::default(),
            fallback: Default::default(),
            has_format_value: false,
        }
        .class(L10nClass)
    }

    pub fn arg<FV: Into<FluentValue<'static>>>(
        mut self,
        arg_key: impl Into<String>,
        arg_val: impl Fn() -> FV + 'static,
    ) -> Self {
        let id = self.id;
        let arg_key = arg_key.into();

        // Pin projection: get mutable access to pinned data
        let arg_keys = unsafe { self.arg_keys.as_mut().get_unchecked_mut() };
        let offset = arg_keys.next_offset();
        arg_keys.push(arg_key);

        let arg_key_ref = arg_keys.get(offset);
        let arg_key_ptr: *const str = arg_key_ref.as_ref();
        // SAFETY: arg_keys is pinned in a Box, so the pointer remains valid
        // for the lifetime of the L10n struct
        let static_ref: &'static str = unsafe { &*arg_key_ptr };

        let initial_val = create_updater(
            move || arg_val().into(),
            move |arg_val: FluentValue<'static>| {
                id.update_state((offset, arg_val));
            },
        );
        self.args.set(Cow::Borrowed(static_ref), initial_val);
        self
    }
}

pub fn l10n(label_key: impl Into<String>) -> L10n {
    L10n::new(label_key)
}

impl View for L10n {
    fn id(&self) -> ViewId {
        self.id
    }

    fn style_pass(&mut self, cx: &mut crate::context::StyleCx<'_>) {
        if self.language.read(cx) {
            self.has_format_value = false;
        }
        if !self.has_format_value {
            let bundle = self.language.bundle();
            if let Some(language) = self.language.language() {
                if let Some(resource) = bundle.0.get(&language) {
                    if let Some(message) = resource.get_message(&self.key) {
                        if let Some(pattern) = message.value() {
                            let errors = &mut vec![];
                            let value = resource.format_pattern(pattern, Some(&self.args), errors);
                            if errors.is_empty() {
                                self.label_id.update_state(value.to_string());
                                self.has_format_value = true
                            }
                        }
                    }
                }
            }
        }
        self.fallback.read(cx);
        if !self.has_format_value {
            if let Some(fallback) = self.fallback.fallback() {
                self.label_id.update_state(fallback.to_string());
            }
        }
        for child in self.id().children() {
            cx.style_view(child);
        }
    }

    fn update(&mut self, _cx: &mut crate::context::UpdateCx, state: Box<dyn std::any::Any>) {
        if let Ok(inner) = state.downcast::<(StackOffset<String>, FluentValue<'static>)>() {
            self.has_format_value = false;
            let (offset, arg_val) = *inner;
            let arg_key_ref = self.arg_keys.get(offset);
            let arg_key_ptr: *const str = arg_key_ref.as_ref();
            // SAFETY: arg_keys is pinned in a Box, so the pointer remains valid
            // for the lifetime of the L10n struct
            let static_ref: &'static str = unsafe { &*arg_key_ptr };
            self.args.set(Cow::Borrowed(static_ref), arg_val);

            let bundle = self.language.bundle();
            if let Some(language) = self.language.language() {
                if let Some(resource) = bundle.0.get(&language) {
                    if let Some(message) = resource.get_message(&self.key) {
                        if let Some(pattern) = message.value() {
                            let errors = &mut vec![];
                            let value = resource.format_pattern(pattern, Some(&self.args), errors);
                            if errors.is_empty() {
                                self.label_id.update_state(value.to_string());
                                self.has_format_value = true;
                                return;
                            }
                        }
                    }
                }
            }
            if let Some(fallback) = self.fallback.fallback()
                && !self.has_format_value
            {
                self.label_id.update_state(fallback.to_string());
            }
        }
    }
}

/// Represents a custom style for `L10n`.
#[derive(Debug, Clone)]
pub struct L10nCustomStyle(Style);

impl From<L10nCustomStyle> for Style {
    fn from(value: L10nCustomStyle) -> Self {
        value.0
    }
}

impl From<Style> for L10nCustomStyle {
    fn from(value: Style) -> Self {
        Self(value)
    }
}

impl CustomStyle for L10nCustomStyle {
    type StyleClass = L10nClass;
}

impl CustomStylable<L10nCustomStyle> for L10n {
    type DV = Self;
}

impl L10nCustomStyle {
    pub fn new() -> Self {
        Self(Style::new())
    }

    pub fn language(mut self, language: impl Into<LanguageIdentifier>) -> Self {
        let language = language.into();
        self = Self(self.0.set(L10nLanguage, Some(language)));
        self
    }

    pub fn fallback(mut self, fallback: impl Into<String>) -> Self {
        let string = fallback.into();
        self = Self(self.0.set(L10nFallback, Some(string)));
        self
    }

    pub fn bundle(mut self, bundle: impl Into<LanguageMap>) -> Self {
        self = Self(self.0.set(L10nBundle, bundle));
        self
    }
}

impl Default for L10nCustomStyle {
    fn default() -> Self {
        Self::new()
    }
}
