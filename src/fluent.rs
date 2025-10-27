use std::borrow::Cow;
use std::rc::Rc;

use crate::style::StylePropValue;
use crate::view_state::{Stack, StackOffset};
use crate::views::static_label;
use crate::{IntoView, View, ViewId, prop, prop_extractor};
use floem_reactive::create_updater;
use fluent_bundle::{FluentBundle, FluentResource};

pub use fluent_bundle::FluentArgs;
pub use fluent_bundle::types::FluentValue;
use smallvec::smallvec;
pub use unic_langid::LanguageIdentifier;

fn get_os_language() -> Option<String> {
    // TODO: use external crate for it?
    None
}

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
    fn debug_view(&self) -> Option<Box<dyn View>> {
        None
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

prop!(pub L10nLanguage: Option<LanguageIdentifier> { inherited } = None);
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

pub struct L10n {
    id: ViewId,
    key: String,
    args: FluentArgs<'static>,                  // SAFETY: Drop first
    arg_keys: crate::view_state::Stack<String>, // SAFETY: Drop second
    label_id: ViewId,
    language: LanguageExtractor,
    fallback: FallBackExtractor,
}

impl L10n {
    pub fn new(key: impl Into<String>) -> Self {
        // using static label because we will manually send the state updates through the ViewId.
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
            arg_keys: Stack { stack: smallvec![] },
            language: Default::default(),
            fallback: Default::default(),
        }
    }

    pub fn arg<FV: Into<FluentValue<'static>>>(
        mut self,
        arg_key: impl Into<String>,
        arg_val: impl Fn() -> FV + 'static,
    ) -> Self {
        let id = self.id;
        let arg_key = arg_key.into();
        let offset = self.arg_keys.next_offset();
        self.arg_keys.push(arg_key);

        let arg_key_ref = self.arg_keys.get(offset);
        let arg_key_ptr: *const str = arg_key_ref.as_ref();

        // SAFETY: args is dropped before arg_keys due to field declaration order,
        // so this pointer remains valid for the lifetime of args
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
        for child in self.id().children() {
            cx.style_view(child);
        }
        if self.language.read(cx) {
            let bundle = self.language.bundle();
            if let Some(language) = self.language.language() {
                if let Some(resource) = bundle.0.get(&language) {
                    if let Some(message) = resource.get_message(&self.key) {
                        if let Some(pattern) = message.value() {
                            let errors = &mut vec![];
                            let value = resource.format_pattern(pattern, Some(&self.args), errors);
                            self.label_id.update_state(value.to_string());
                            return;
                        }
                    }
                }
            }
        }
        if self.fallback.read(cx) {
            self.label_id.update_state(self.fallback.fallback());
        }
    }

    fn update(&mut self, _cx: &mut crate::context::UpdateCx, state: Box<dyn std::any::Any>) {
        if let Ok(inner) = state.downcast::<(StackOffset<String>, FluentValue<'static>)>() {
            let (offset, arg_val) = *inner;
            let arg_key_ref = self.arg_keys.get(offset);
            let arg_key_ptr: *const str = arg_key_ref.as_ref();
            // SAFETY: args is dropped before arg_keys due to field declaration order,
            // so this pointer remains valid for the lifetime of args
            let static_ref: &'static str = unsafe { &*arg_key_ptr };
            self.args.set(Cow::Borrowed(static_ref), arg_val);

            let bundle = self.language.bundle();
            if let Some(language) = self.language.language() {
                if let Some(resource) = bundle.0.get(&language) {
                    if let Some(message) = resource.get_message(&self.key) {
                        if let Some(pattern) = message.value() {
                            let errors = &mut vec![];
                            let value = resource.format_pattern(pattern, Some(&self.args), errors);
                            self.label_id.update_state(value.to_string());
                            return;
                        }
                    }
                }
            }
            self.label_id.update_state(self.fallback.fallback());
        }
    }
}
