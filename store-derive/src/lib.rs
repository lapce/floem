//! Derive macros for floem_store.
//!
//! This crate provides the `#[derive(Lenses)]` macro that automatically
//! generates lens types, accessor methods, and wrapper types for struct fields.
//!
//! # Example
//!
//! ```rust,ignore
//! use floem_store::{Store, Lenses};
//! use std::collections::HashMap;
//!
//! #[derive(Lenses, Default)]
//! struct State {
//!     count: i32,
//!     #[nested]  // Mark fields that also have #[derive(Lenses)]
//!     user: User,
//!     #[nested]  // Also works with Vec<T> where T has #[derive(Lenses)]
//!     items: Vec<Item>,
//!     #[nested]  // Also works with HashMap<K, V> where V has #[derive(Lenses)]
//!     users_by_id: HashMap<u32, User>,
//! }
//!
//! #[derive(Lenses, Default)]
//! struct User {
//!     name: String,
//!     age: i32,
//! }
//!
//! #[derive(Lenses, Default, Clone)]
//! struct Item {
//!     text: String,
//!     done: bool,
//! }
//!
//! // Use the generated wrapper type - NO IMPORTS NEEDED even for nested access!
//! let store = StateStore::new(State::default());
//! let count = store.count();
//! let name = store.user().name();  // Works without imports!
//! let first_item_text = store.items().index(0).text();  // Vec<T> nested access!
//! let user_1_name = store.users_by_id().key(1).name();  // HashMap<K, V> nested access!
//!
//! count.set(42);
//! name.set("Alice".into());
//! ```
//!
//! The `#[nested]` attribute tells the macro that a field's type also has
//! `#[derive(Lenses)]`, so it returns the wrapper type instead of raw `Binding`.
//! This works at multiple levels of nesting and with `Vec<T>` and `HashMap<K, V>` fields.

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{parse_macro_input, Data, DeriveInput, Fields, GenericArgument, PathArguments, Type};

/// Information about a nested type
enum NestedKind {
    /// Direct nested type (e.g., `user: User`)
    Direct(syn::Ident),
    /// Vec of nested type (e.g., `items: Vec<Item>`)
    /// Contains the inner type ident, optional key field name, and optional key type for identity-based access
    Vec {
        inner_ident: syn::Ident,
        key_field: Option<syn::Ident>,
        key_type: Option<syn::Type>,
    },
    /// HashMap of nested type (e.g., `users: HashMap<u32, User>`)
    /// Contains the value type's ident (the key type is handled separately)
    HashMap(syn::Ident),
    /// IndexMap of nested type (e.g., `todos: IndexMap<u64, Todo>`)
    /// Contains the value type's ident and optional key field name for push() convenience
    IndexMap {
        val_ident: syn::Ident,
        key_field: Option<syn::Ident>,
    },
    /// Not nested
    None,
}

/// Parse #[nested] or #[nested(key = field_name)] or #[nested(key = field_name: KeyType)] attribute
fn parse_nested_attr(attr: &syn::Attribute) -> Option<Option<KeyAttr>> {
    if !attr.path().is_ident("nested") {
        return None;
    }

    // Try to parse as #[nested(key = field_name)] or #[nested(key = field_name: KeyType)]
    match &attr.meta {
        syn::Meta::Path(_) => {
            // Just #[nested] without arguments
            Some(None)
        }
        syn::Meta::List(list) => {
            // #[nested(key = field_name)] or #[nested(key = field_name: KeyType)]
            let tokens = list.tokens.clone();
            let parsed: Result<KeyAttr, _> = syn::parse2(tokens);
            match parsed {
                Ok(key_attr) => Some(Some(key_attr)),
                Err(_) => Some(None), // Fallback to no key
            }
        }
        _ => Some(None),
    }
}

/// Helper struct for parsing `key = field_name` or `key = field_name: KeyType`
struct KeyAttr {
    field: syn::Ident,
    key_type: Option<syn::Type>,
}

impl syn::parse::Parse for KeyAttr {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let key_ident: syn::Ident = input.parse()?;
        if key_ident != "key" {
            return Err(syn::Error::new(key_ident.span(), "expected `key`"));
        }
        let _eq: syn::Token![=] = input.parse()?;
        let field: syn::Ident = input.parse()?;

        // Optionally parse `: KeyType`
        let key_type = if input.peek(syn::Token![:]) {
            let _colon: syn::Token![:] = input.parse()?;
            Some(input.parse()?)
        } else {
            None
        };

        Ok(KeyAttr { field, key_type })
    }
}

/// Extract the inner type from Vec<T> if the type is a Vec
fn extract_vec_inner_type(ty: &Type) -> Option<&Type> {
    if let Type::Path(type_path) = ty {
        let segment = type_path.path.segments.last()?;
        if segment.ident == "Vec" {
            if let PathArguments::AngleBracketed(args) = &segment.arguments {
                if let Some(GenericArgument::Type(inner_ty)) = args.args.first() {
                    return Some(inner_ty);
                }
            }
        }
    }
    None
}

/// Get the type name as an identifier (for simple types)
fn type_to_ident(ty: &Type) -> Option<syn::Ident> {
    if let Type::Path(type_path) = ty {
        let segment = type_path.path.segments.last()?;
        Some(segment.ident.clone())
    } else {
        None
    }
}

/// Extract the key and value types from HashMap<K, V> if the type is a HashMap
fn extract_hashmap_types(ty: &Type) -> Option<(&Type, &Type)> {
    if let Type::Path(type_path) = ty {
        let segment = type_path.path.segments.last()?;
        if segment.ident == "HashMap" {
            if let PathArguments::AngleBracketed(args) = &segment.arguments {
                let mut iter = args.args.iter();
                if let (Some(GenericArgument::Type(key_ty)), Some(GenericArgument::Type(val_ty))) =
                    (iter.next(), iter.next())
                {
                    return Some((key_ty, val_ty));
                }
            }
        }
    }
    None
}

/// Extract the key and value types from IndexMap<K, V>
fn extract_indexmap_types(ty: &Type) -> Option<(&Type, &Type)> {
    if let Type::Path(type_path) = ty {
        let segment = type_path.path.segments.last()?;
        if segment.ident == "IndexMap" {
            if let PathArguments::AngleBracketed(args) = &segment.arguments {
                let mut iter = args.args.iter();
                if let (Some(GenericArgument::Type(key_ty)), Some(GenericArgument::Type(val_ty))) =
                    (iter.next(), iter.next())
                {
                    return Some((key_ty, val_ty));
                }
            }
        }
    }
    None
}

/// Derive macro that generates lens types and wrapper types for struct fields.
///
/// For a struct `State` with fields `count` and `user`, this generates:
/// - A module `state_lenses` containing lens types `CountLens` and `UserLens`
/// - A wrapper type `StateStore` with direct method access
/// - A wrapper type `StateBinding` for binding wrappers
///
/// # Example
///
/// ```rust,ignore
/// use floem_store::{Store, Lenses};
///
/// #[derive(Lenses, Default)]
/// struct AppState {
///     count: i32,
///     name: String,
/// }
///
/// // Use the wrapper type - NO IMPORTS NEEDED!
/// let store = AppStateStore::new(AppState::default());
/// let count = store.count();
/// let name = store.name();
///
/// count.set(42);
/// name.set("Hello".into());
/// ```
#[proc_macro_derive(Lenses, attributes(nested))]
pub fn derive_lenses(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let struct_name = &input.ident;

    // Create module name: UserProfile -> user_profile_lenses
    let module_name = format_ident!("{}_lenses", to_snake_case(&struct_name.to_string()));

    // Get the fields
    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => &fields.named,
            _ => {
                return syn::Error::new_spanned(
                    &input,
                    "Lenses can only be derived for structs with named fields",
                )
                .to_compile_error()
                .into();
            }
        },
        _ => {
            return syn::Error::new_spanned(&input, "Lenses can only be derived for structs")
                .to_compile_error()
                .into();
        }
    };

    // Collect field info, including whether they have #[nested] attribute
    let field_info: Vec<_> = fields
        .iter()
        .filter_map(|field| {
            let field_name = field.ident.as_ref()?;
            let field_type = &field.ty;
            // Append "Lens" to avoid collision when field name matches type name
            // e.g., field `user: User` creates lens `UserLens`, not `User`
            let lens_struct_name =
                format_ident!("{}Lens", to_pascal_case(&field_name.to_string()));

            // Parse #[nested] or #[nested(key = field_name)] attribute
            let nested_attr = field
                .attrs
                .iter()
                .find_map(parse_nested_attr);

            // Determine the nested kind
            let nested_kind = if let Some(key_attr) = nested_attr {
                // Check if it's Vec<T>
                if let Some(inner_ty) = extract_vec_inner_type(field_type) {
                    if let Some(inner_ident) = type_to_ident(inner_ty) {
                        NestedKind::Vec {
                            inner_ident,
                            key_field: key_attr.as_ref().map(|k| k.field.clone()),
                            key_type: key_attr.and_then(|k| k.key_type),
                        }
                    } else {
                        NestedKind::None
                    }
                // Check if it's IndexMap<K, V>
                } else if let Some((_key_ty, val_ty)) = extract_indexmap_types(field_type) {
                    if let Some(val_ident) = type_to_ident(val_ty) {
                        NestedKind::IndexMap {
                            val_ident,
                            key_field: key_attr.map(|k| k.field),
                        }
                    } else {
                        NestedKind::None
                    }
                // Check if it's HashMap<K, V>
                } else if let Some((_key_ty, val_ty)) = extract_hashmap_types(field_type) {
                    if let Some(val_ident) = type_to_ident(val_ty) {
                        NestedKind::HashMap(val_ident)
                    } else {
                        NestedKind::None
                    }
                } else if let Some(ident) = type_to_ident(field_type) {
                    NestedKind::Direct(ident)
                } else {
                    NestedKind::None
                }
            } else {
                NestedKind::None
            };

            Some((
                field_name.clone(),
                field_type.clone(),
                lens_struct_name,
                nested_kind,
            ))
        })
        .collect();

    // Generate lens structs and field type aliases
    let lens_impls: Vec<_> = field_info
        .iter()
        .map(|(field_name, field_type, lens_struct_name, _nested_kind)| {
            let field_name_str = field_name.to_string();
            let lens_doc = format!("Lens for the `{}` field of [`{}`].", field_name, struct_name);
            // Generate a type alias for this field's type, so it can be referenced
            // by other structs (e.g., for by_key access without explicit type)
            let field_type_alias = format_ident!(
                "{}{}FieldType",
                struct_name,
                to_pascal_case(&field_name.to_string())
            );
            let type_alias_doc = format!(
                "Type alias for the `{}` field of [`{}`].\n\n\
                This is used internally for identity-based Vec access.",
                field_name, struct_name
            );
            quote! {
                #[doc = #type_alias_doc]
                pub type #field_type_alias = #field_type;

                #[doc = #lens_doc]
                #[derive(Copy, Clone, Debug, Default)]
                pub struct #lens_struct_name;

                impl floem_store::Lens<#struct_name, #field_type> for #lens_struct_name {
                    const PATH_HASH: u64 = floem_store::lens::const_hash(#field_name_str);

                    fn get<'a>(&self, source: &'a #struct_name) -> &'a #field_type {
                        &source.#field_name
                    }

                    fn get_mut<'a>(&self, source: &'a mut #struct_name) -> &'a mut #field_type {
                        &mut source.#field_name
                    }
                }
            }
        })
        .collect();

    // Generate wrapper store struct name: State -> StateStore
    let store_wrapper_name = format_ident!("{}Store", struct_name);

    // Generate wrapper binding struct name: State -> StateBinding
    let binding_wrapper_name = format_ident!("{}Binding", struct_name);

    // Collect Vec wrapper types we need to generate
    let vec_wrapper_types: Vec<_> = field_info
        .iter()
        .filter_map(|(field_name, field_type, _lens_struct_name, nested_kind)| {
            if let NestedKind::Vec { inner_ident, key_field, key_type } = nested_kind {
                let wrapper_name =
                    format_ident!("{}VecBinding", to_pascal_case(&field_name.to_string()));
                let inner_binding_wrapper = format_ident!("{}Binding", inner_ident);

                // Extract the inner type from Vec<T>
                let inner_type = extract_vec_inner_type(field_type)?;

                Some((wrapper_name, inner_binding_wrapper, inner_type.clone(), key_field.clone(), key_type.clone()))
            } else {
                None
            }
        })
        .collect();

    // Generate Vec wrapper structs
    let vec_wrapper_impls: Vec<_> = vec_wrapper_types
        .iter()
        .map(|(wrapper_name, inner_binding_wrapper, inner_type, key_field, explicit_key_type)| {
            // Generate by_key method and lens when key_field is provided.
            // The key type is inferred from the inner type's field type alias if not explicitly provided.
            //
            // Usage:
            // - `#[nested]` - no keyed reconciliation, no by_id method
            // - `#[nested(key = id)]` - keyed reconciliation AND by_id method (type inferred from inner type)
            // - `#[nested(key = id: u64)]` - keyed reconciliation AND by_id method (explicit type)
            let by_key_impl = if let Some(key_field) = key_field {
                // Determine the key type: use explicit type if provided, otherwise use the generated type alias
                let key_type: syn::Type = if let Some(explicit) = explicit_key_type {
                    explicit.clone()
                } else {
                    // Use the generated type alias: {inner_type_lenses}::{InnerType}{KeyFieldPascalCase}FieldType
                    let inner_type_ident = type_to_ident(inner_type);
                    if let Some(inner_ident) = inner_type_ident {
                        // The type alias is in the lens module: {inner_type_snake}_lenses::{InnerType}{Field}FieldType
                        let inner_module_name = format!("{}_lenses", to_snake_case(&inner_ident.to_string()));
                        let type_alias_name = format!(
                            "{}{}FieldType",
                            inner_ident,
                            to_pascal_case(&key_field.to_string())
                        );
                        let full_path = format!("{}::{}", inner_module_name, type_alias_name);
                        syn::parse_str(&full_path).unwrap_or_else(|_| {
                            // Fallback: if parsing fails, skip by_key generation
                            return syn::parse_str("()").unwrap();
                        })
                    } else {
                        // Can't determine inner type, skip by_key
                        return quote! {};
                    }
                };
                let by_key_method = format_ident!("by_{}", key_field);
                let key_lens_name = format_ident!("{}By{}Lens", wrapper_name, to_pascal_case(&key_field.to_string()));
                let key_field_str = key_field.to_string();

                quote! {
                    /// Lens for accessing a Vec element by its key field (identity-based access).
                    ///
                    /// This lens finds the item with the matching key value, regardless of its position.
                    /// The PathId is based on the key value, not the position, enabling stable bindings
                    /// across reorders.
                    ///
                    /// Uses lazy caching: stores a position hint that's checked first (O(1)),
                    /// falling back to O(N) search only if the item has moved.
                    #[derive(Clone, Copy)]
                    pub struct #key_lens_name {
                        key: #key_type,
                        /// Cached position hint - checked first for O(1) access.
                        /// If the item at this position doesn't have the right key, we fall back to O(N).
                        cached_pos: usize,
                    }

                    impl floem_store::Lens<Vec<#inner_type>, #inner_type> for #key_lens_name {
                        const PATH_HASH: u64 = floem_store::lens::const_hash(concat!("[by_", #key_field_str, "]"));

                        fn get<'a>(&self, source: &'a Vec<#inner_type>) -> &'a #inner_type {
                            // Try cached position first (O(1))
                            if let Some(item) = source.get(self.cached_pos) {
                                if item.#key_field == self.key {
                                    return item;
                                }
                            }
                            // Fall back to O(N) search if item moved
                            source.iter()
                                .find(|item| item.#key_field == self.key)
                                .expect(concat!("item with ", #key_field_str, " not found in Vec"))
                        }

                        fn get_mut<'a>(&self, source: &'a mut Vec<#inner_type>) -> &'a mut #inner_type {
                            // Try cached position first (O(1))
                            // Need to check without borrowing mutably first
                            let use_cached = source.get(self.cached_pos)
                                .map(|item| item.#key_field == self.key)
                                .unwrap_or(false);

                            if use_cached {
                                return &mut source[self.cached_pos];
                            }

                            // Fall back to O(N) search if item moved
                            source.iter_mut()
                                .find(|item| item.#key_field == self.key)
                                .expect(concat!("item with ", #key_field_str, " not found in Vec"))
                        }

                        /// Each key gets a unique path hash by mixing the key into the base hash.
                        /// This enables identity-based fine-grained reactivity.
                        fn path_hash(&self) -> u64 {
                            use std::hash::{Hash, Hasher};
                            let mut hasher = std::collections::hash_map::DefaultHasher::new();
                            self.key.hash(&mut hasher);
                            let key_hash = hasher.finish();

                            let mut hash = floem_store::lens::const_hash(concat!("[by_", #key_field_str, "]"));
                            hash ^= key_hash;
                            hash = hash.wrapping_mul(0x100000001b3u64); // FNV_PRIME
                            hash
                        }
                    }

                    impl<__Root: 'static, __L: floem_store::Lens<__Root, Vec<#inner_type>>> #wrapper_name<__Root, __L> {
                        /// Get a wrapped binding for the element with the given key (identity-based access).
                        ///
                        /// Unlike `index()` which accesses by position, this method accesses by the item's
                        /// identity (key field). The binding's PathId is based on the key value, so it
                        /// remains stable even if the item's position changes.
                        ///
                        /// Uses lazy caching for O(1) best-case access: the current position is cached
                        /// when the binding is created. If the item hasn't moved, access is O(1).
                        /// If the item has moved, it falls back to O(N) search.
                        ///
                        /// # Panics
                        ///
                        /// Panics if no item with the given key exists in the Vec.
                        ///
                        /// # Example
                        ///
                        /// ```rust,ignore
                        #[doc = concat!(" let item = vec_binding.", stringify!(#by_key_method), "(5);")]
                        /// // This binding stays on the item with key=5 even if the Vec is reordered
                        /// ```
                        pub fn #by_key_method(&self, key: #key_type) -> #inner_binding_wrapper<
                            __Root,
                            floem_store::ComposedLens<__L, #key_lens_name, Vec<#inner_type>>
                        >
                        where
                            #key_type: std::hash::Hash + Eq + Copy + 'static,
                        {
                            // Find current position and cache it as a hint
                            let cached_pos = self.inner.with_untracked(|v| {
                                v.iter()
                                    .position(|item| item.#key_field == key)
                                    .expect(concat!("item with ", #key_field_str, " not found in Vec"))
                            });

                            #inner_binding_wrapper::from_binding(
                                self.inner.binding_with_lens(#key_lens_name { key, cached_pos })
                            )
                        }

                        /// Check if an item with the given key exists in the Vec.
                        pub fn contains_key(&self, key: &#key_type) -> bool
                        where
                            #key_type: PartialEq,
                        {
                            self.inner.with(|v| v.iter().any(|item| &item.#key_field == key))
                        }

                        /// Remove an item by its key. Returns the removed item if found.
                        pub fn remove_by_key(&self, key: &#key_type) -> Option<#inner_type>
                        where
                            #key_type: PartialEq,
                            #inner_type: Clone,
                        {
                            self.inner.try_update(|v| {
                                if let Some(idx) = v.iter().position(|item| &item.#key_field == key) {
                                    Some(v.remove(idx))
                                } else {
                                    None
                                }
                            })
                        }

                        /// Get bindings for all items that match a filter predicate.
                        ///
                        /// This is useful for `dyn_stack` where you want to return bindings directly:
                        ///
                        /// ```rust,ignore
                        /// dyn_stack(
                        ///     move || todos.filtered_bindings(|todo| !todo.done),
                        ///     |binding| binding.id().get_untracked(),
                        ///     move |binding| todo_item_view(binding),
                        /// )
                        /// ```
                        ///
                        /// The filter closure receives `&T` (plain reference) so it doesn't create
                        /// reactive subscriptions. Each returned binding uses identity-based access
                        /// via `by_key`, so bindings remain stable across reorders.
                        ///
                        /// Uses cached positions for O(1) access on each binding.
                        pub fn filtered_bindings<__F>(
                            &self,
                            filter: __F,
                        ) -> impl Iterator<Item = #inner_binding_wrapper<
                            __Root,
                            floem_store::ComposedLens<__L, #key_lens_name, Vec<#inner_type>>
                        >> + 'static
                        where
                            __F: Fn(&#inner_type) -> bool,
                            #key_type: std::hash::Hash + Eq + Copy + 'static,
                        {
                            // Collect (key, position) pairs during iteration - O(N) total
                            // Each binding gets its position cached for O(1) subsequent access
                            let inner = self.inner.clone();
                            self.inner.with(|v| {
                                v.iter()
                                    .enumerate()
                                    .filter(|(_, item)| filter(item))
                                    .map(|(cached_pos, item)| {
                                        let key = item.#key_field;
                                        #inner_binding_wrapper::from_binding(
                                            inner.binding_with_lens(#key_lens_name { key, cached_pos })
                                        )
                                    })
                                    .collect::<Vec<_>>()
                            }).into_iter()
                        }

                        /// Get bindings for all items in the Vec.
                        ///
                        /// Returns an iterator of bindings, one for each item, using identity-based access.
                        /// This is equivalent to `filtered_bindings(|_| true)`.
                        /// Uses cached positions for O(1) access on each binding.
                        pub fn all_bindings(&self) -> impl Iterator<Item = #inner_binding_wrapper<
                            __Root,
                            floem_store::ComposedLens<__L, #key_lens_name, Vec<#inner_type>>
                        >> + 'static
                        where
                            #key_type: std::hash::Hash + Eq + Copy + 'static,
                        {
                            self.filtered_bindings(|_| true)
                        }
                    }
                }
            } else {
                quote! {}
            };

            quote! {
                /// Wrapper around `Binding<Root, Vec<T>, L>` that returns wrapped element types.
                pub struct #wrapper_name<__Root: 'static, __L: floem_store::Lens<__Root, Vec<#inner_type>>> {
                    inner: floem_store::Binding<__Root, Vec<#inner_type>, __L>,
                }

                impl<__Root: 'static, __L: floem_store::Lens<__Root, Vec<#inner_type>>> #wrapper_name<__Root, __L> {
                    /// Create a wrapper from a raw Binding.
                    pub fn from_binding(binding: floem_store::Binding<__Root, Vec<#inner_type>, __L>) -> Self {
                        Self { inner: binding }
                    }

                    /// Get the underlying Binding.
                    pub fn inner(&self) -> &floem_store::Binding<__Root, Vec<#inner_type>, __L> {
                        &self.inner
                    }

                    /// Consume wrapper and return the underlying Binding.
                    pub fn into_inner(self) -> floem_store::Binding<__Root, Vec<#inner_type>, __L> {
                        self.inner
                    }

                    /// Get a wrapped binding for the element at the given index (position-based access).
                    pub fn index(&self, index: usize) -> #inner_binding_wrapper<
                        __Root,
                        floem_store::ComposedLens<__L, floem_store::lens::IndexLens, Vec<#inner_type>>
                    > {
                        #inner_binding_wrapper::from_binding(self.inner.index(index))
                    }

                    /// Get the length of the Vec.
                    pub fn len(&self) -> usize {
                        self.inner.len()
                    }

                    /// Check if the Vec is empty.
                    pub fn is_empty(&self) -> bool {
                        self.inner.is_empty()
                    }

                    /// Push an element to the Vec.
                    pub fn push(&self, value: #inner_type) {
                        self.inner.push(value);
                    }

                    /// Pop an element from the Vec.
                    pub fn pop(&self) -> Option<#inner_type>
                    where
                        #inner_type: Clone,
                    {
                        self.inner.pop()
                    }

                    /// Clear the Vec.
                    pub fn clear(&self) {
                        self.inner.clear();
                    }

                    /// Update the Vec with a closure.
                    pub fn update(&self, f: impl FnOnce(&mut Vec<#inner_type>)) {
                        self.inner.update(f);
                    }

                    /// Read the Vec by reference.
                    pub fn with<R>(&self, f: impl FnOnce(&Vec<#inner_type>) -> R) -> R {
                        self.inner.with(f)
                    }
                }

                impl<__Root: 'static, __L: floem_store::Lens<__Root, Vec<#inner_type>>> Clone for #wrapper_name<__Root, __L> {
                    fn clone(&self) -> Self {
                        Self {
                            inner: self.inner.clone(),
                        }
                    }
                }

                #by_key_impl
            }
        })
        .collect();

    // Collect HashMap wrapper types we need to generate
    let hashmap_wrapper_types: Vec<_> = field_info
        .iter()
        .filter_map(|(field_name, field_type, _lens_struct_name, nested_kind)| {
            if let NestedKind::HashMap(val_ident) = nested_kind {
                let wrapper_name =
                    format_ident!("{}HashMapBinding", to_pascal_case(&field_name.to_string()));
                let val_binding_wrapper = format_ident!("{}Binding", val_ident);

                // Extract the key and value types from HashMap<K, V>
                let (key_type, val_type) = extract_hashmap_types(field_type)?;

                Some((wrapper_name, val_binding_wrapper, key_type.clone(), val_type.clone()))
            } else {
                None
            }
        })
        .collect();

    // Generate HashMap wrapper structs
    let hashmap_wrapper_impls: Vec<_> = hashmap_wrapper_types
        .iter()
        .map(|(wrapper_name, val_binding_wrapper, key_type, val_type)| {
            quote! {
                /// Wrapper around `Binding<Root, HashMap<K, V>, L>` that returns wrapped value types.
                pub struct #wrapper_name<__Root: 'static, __L: floem_store::Lens<__Root, std::collections::HashMap<#key_type, #val_type>>> {
                    inner: floem_store::Binding<__Root, std::collections::HashMap<#key_type, #val_type>, __L>,
                }

                impl<__Root: 'static, __L: floem_store::Lens<__Root, std::collections::HashMap<#key_type, #val_type>>> #wrapper_name<__Root, __L> {
                    /// Create a wrapper from a raw Binding.
                    pub fn from_binding(binding: floem_store::Binding<__Root, std::collections::HashMap<#key_type, #val_type>, __L>) -> Self {
                        Self { inner: binding }
                    }

                    /// Get the underlying Binding.
                    pub fn inner(&self) -> &floem_store::Binding<__Root, std::collections::HashMap<#key_type, #val_type>, __L> {
                        &self.inner
                    }

                    /// Consume wrapper and return the underlying Binding.
                    pub fn into_inner(self) -> floem_store::Binding<__Root, std::collections::HashMap<#key_type, #val_type>, __L> {
                        self.inner
                    }

                    /// Get a wrapped binding for the value at the given key.
                    pub fn key(&self, key: #key_type) -> #val_binding_wrapper<
                        __Root,
                        floem_store::ComposedLens<__L, floem_store::KeyLens<#key_type>, std::collections::HashMap<#key_type, #val_type>>
                    >
                    where
                        #key_type: Copy,
                    {
                        #val_binding_wrapper::from_binding(self.inner.key(key))
                    }

                    /// Get the number of entries in the HashMap.
                    pub fn len(&self) -> usize {
                        self.inner.len()
                    }

                    /// Check if the HashMap is empty.
                    pub fn is_empty(&self) -> bool {
                        self.inner.is_empty()
                    }

                    /// Check if the HashMap contains the given key.
                    pub fn contains_key(&self, key: &#key_type) -> bool {
                        self.inner.contains_key(key)
                    }

                    /// Insert a key-value pair into the HashMap.
                    pub fn insert(&self, key: #key_type, value: #val_type) -> Option<#val_type> {
                        self.inner.insert(key, value)
                    }

                    /// Remove a key from the HashMap.
                    pub fn remove(&self, key: &#key_type) -> Option<#val_type> {
                        self.inner.remove(key)
                    }

                    /// Clear the HashMap.
                    pub fn clear(&self) {
                        self.inner.clear();
                    }

                    /// Get a cloned value for the given key, if present.
                    pub fn get_value(&self, key: &#key_type) -> Option<#val_type>
                    where
                        #val_type: Clone,
                    {
                        self.inner.get_value(key)
                    }

                    /// Update the HashMap with a closure.
                    pub fn update(&self, f: impl FnOnce(&mut std::collections::HashMap<#key_type, #val_type>)) {
                        self.inner.update(f);
                    }

                    /// Read the HashMap by reference.
                    pub fn with<R>(&self, f: impl FnOnce(&std::collections::HashMap<#key_type, #val_type>) -> R) -> R {
                        self.inner.with(f)
                    }
                }

                impl<__Root: 'static, __L: floem_store::Lens<__Root, std::collections::HashMap<#key_type, #val_type>>> Clone for #wrapper_name<__Root, __L> {
                    fn clone(&self) -> Self {
                        Self {
                            inner: self.inner.clone(),
                        }
                    }
                }
            }
        })
        .collect();

    // Collect IndexMap wrapper types we need to generate
    let indexmap_wrapper_types: Vec<_> = field_info
        .iter()
        .filter_map(|(field_name, field_type, _lens_struct_name, nested_kind)| {
            if let NestedKind::IndexMap { val_ident, key_field } = nested_kind {
                let wrapper_name =
                    format_ident!("{}IndexMapBinding", to_pascal_case(&field_name.to_string()));
                let val_binding_wrapper = format_ident!("{}Binding", val_ident);

                // Extract the key and value types from IndexMap<K, V>
                let (key_type, val_type) = extract_indexmap_types(field_type)?;

                Some((wrapper_name, val_binding_wrapper, key_type.clone(), val_type.clone(), key_field.clone()))
            } else {
                None
            }
        })
        .collect();

    // Generate IndexMap wrapper structs
    let indexmap_wrapper_impls: Vec<_> = indexmap_wrapper_types
        .iter()
        .map(|(wrapper_name, val_binding_wrapper, key_type, val_type, key_field)| {
            // Generate push method if key_field is provided (extracts key from value)
            let push_impl = if let Some(key_field) = key_field {
                quote! {
                    /// Push an item to the IndexMap, extracting the key from the value.
                    ///
                    /// This is a convenience method that extracts the key from the value's
                    /// field and inserts it into the map.
                    pub fn push(&self, value: #val_type)
                    where
                        #key_type: Copy,
                    {
                        let key = value.#key_field;
                        self.inner.insert(key, value);
                    }

                    /// Remove an item by its key field value.
                    pub fn remove_by_key(&self, key: &#key_type) -> Option<#val_type>
                    where
                        #key_type: std::hash::Hash + Eq,
                    {
                        self.inner.remove(key)
                    }

                    /// Get bindings for all items that match a filter predicate.
                    ///
                    /// This is useful for `dyn_stack` where you want to return bindings directly.
                    /// The filter closure receives `&V` (plain reference) so it doesn't create
                    /// reactive subscriptions.
                    pub fn filtered_bindings<__F>(
                        &self,
                        filter: __F,
                    ) -> impl Iterator<Item = #val_binding_wrapper<
                        __Root,
                        floem_store::ComposedLens<__L, floem_store::KeyLens<#key_type>, floem_store::IndexMap<#key_type, #val_type>>
                    >> + 'static
                    where
                        __F: Fn(&#val_type) -> bool,
                        #key_type: std::hash::Hash + Eq + Copy + 'static,
                    {
                        let this = self.clone();
                        self.inner.with(|m| {
                            m.iter()
                                .filter(|(_, v)| filter(v))
                                .map(|(k, _)| this.get(*k))
                                .collect::<Vec<_>>()
                        }).into_iter()
                    }

                    /// Get bindings for all items in the IndexMap (in insertion order).
                    ///
                    /// Returns an iterator of bindings, one for each value.
                    pub fn all_bindings(&self) -> impl Iterator<Item = #val_binding_wrapper<
                        __Root,
                        floem_store::ComposedLens<__L, floem_store::KeyLens<#key_type>, floem_store::IndexMap<#key_type, #val_type>>
                    >> + 'static
                    where
                        #key_type: std::hash::Hash + Eq + Copy + 'static,
                    {
                        self.filtered_bindings(|_| true)
                    }
                }
            } else {
                quote! {}
            };

            quote! {
                /// Wrapper around `Binding<Root, IndexMap<K, V>, L>` that returns wrapped value types.
                ///
                /// IndexMap provides O(1) key lookup while preserving insertion order.
                pub struct #wrapper_name<__Root: 'static, __L: floem_store::Lens<__Root, floem_store::IndexMap<#key_type, #val_type>>> {
                    inner: floem_store::Binding<__Root, floem_store::IndexMap<#key_type, #val_type>, __L>,
                }

                impl<__Root: 'static, __L: floem_store::Lens<__Root, floem_store::IndexMap<#key_type, #val_type>>> #wrapper_name<__Root, __L> {
                    /// Create a wrapper from a raw Binding.
                    pub fn from_binding(binding: floem_store::Binding<__Root, floem_store::IndexMap<#key_type, #val_type>, __L>) -> Self {
                        Self { inner: binding }
                    }

                    /// Get the underlying Binding.
                    pub fn inner(&self) -> &floem_store::Binding<__Root, floem_store::IndexMap<#key_type, #val_type>, __L> {
                        &self.inner
                    }

                    /// Consume wrapper and return the underlying Binding.
                    pub fn into_inner(self) -> floem_store::Binding<__Root, floem_store::IndexMap<#key_type, #val_type>, __L> {
                        self.inner
                    }

                    /// Get a wrapped binding for the value at the given key (O(1) lookup).
                    pub fn get(&self, key: #key_type) -> #val_binding_wrapper<
                        __Root,
                        floem_store::ComposedLens<__L, floem_store::KeyLens<#key_type>, floem_store::IndexMap<#key_type, #val_type>>
                    >
                    where
                        #key_type: Copy,
                    {
                        #val_binding_wrapper::from_binding(self.inner.key(key))
                    }

                    /// Get the number of entries in the IndexMap.
                    pub fn len(&self) -> usize {
                        self.inner.len()
                    }

                    /// Check if the IndexMap is empty.
                    pub fn is_empty(&self) -> bool {
                        self.inner.is_empty()
                    }

                    /// Check if the IndexMap contains the given key (O(1) lookup).
                    pub fn contains_key(&self, key: &#key_type) -> bool {
                        self.inner.contains_key(key)
                    }

                    /// Insert a key-value pair into the IndexMap.
                    ///
                    /// If the key already exists, the value is updated but position is preserved.
                    pub fn insert(&self, key: #key_type, value: #val_type) -> Option<#val_type> {
                        self.inner.insert(key, value)
                    }

                    /// Remove a key from the IndexMap (preserves order of remaining elements).
                    pub fn remove(&self, key: &#key_type) -> Option<#val_type> {
                        self.inner.remove(key)
                    }

                    /// Clear the IndexMap.
                    pub fn clear(&self) {
                        self.inner.clear();
                    }

                    /// Get a cloned value for the given key, if present (O(1) lookup).
                    pub fn get_value(&self, key: &#key_type) -> Option<#val_type>
                    where
                        #val_type: Clone,
                    {
                        self.inner.get_value(key)
                    }

                    /// Update the IndexMap with a closure.
                    pub fn update(&self, f: impl FnOnce(&mut floem_store::IndexMap<#key_type, #val_type>)) {
                        self.inner.update(f);
                    }

                    /// Read the IndexMap by reference.
                    pub fn with<R>(&self, f: impl FnOnce(&floem_store::IndexMap<#key_type, #val_type>) -> R) -> R {
                        self.inner.with(f)
                    }

                    #push_impl
                }

                impl<__Root: 'static, __L: floem_store::Lens<__Root, floem_store::IndexMap<#key_type, #val_type>>> Clone for #wrapper_name<__Root, __L> {
                    fn clone(&self) -> Self {
                        Self {
                            inner: self.inner.clone(),
                        }
                    }
                }
            }
        })
        .collect();

    // Generate store wrapper methods
    // For #[nested] fields, return the wrapper type; otherwise return raw Binding
    let store_wrapper_methods: Vec<_> = field_info
        .iter()
        .map(|(field_name, field_type, lens_struct_name, nested_kind)| {
            let field_doc = format!("Get a binding for the `{}` field.", field_name);
            match nested_kind {
                NestedKind::Direct(type_ident) => {
                    // For direct nested fields, return the wrapper type (e.g., UserBinding)
                    let nested_binding_wrapper = format_ident!("{}Binding", type_ident);
                    quote! {
                        #[doc = #field_doc]
                        pub fn #field_name(&self) -> #nested_binding_wrapper<#struct_name, #module_name::#lens_struct_name> {
                            #nested_binding_wrapper::from_binding(self.inner.binding_with_lens(#module_name::#lens_struct_name))
                        }
                    }
                }
                NestedKind::Vec { .. } => {
                    // For Vec nested fields, return the Vec wrapper type
                    let vec_wrapper_name =
                        format_ident!("{}VecBinding", to_pascal_case(&field_name.to_string()));
                    quote! {
                        #[doc = #field_doc]
                        pub fn #field_name(&self) -> #vec_wrapper_name<#struct_name, #module_name::#lens_struct_name> {
                            #vec_wrapper_name::from_binding(self.inner.binding_with_lens(#module_name::#lens_struct_name))
                        }
                    }
                }
                NestedKind::HashMap(_val_ident) => {
                    // For HashMap nested fields, return the HashMap wrapper type
                    let hashmap_wrapper_name =
                        format_ident!("{}HashMapBinding", to_pascal_case(&field_name.to_string()));
                    quote! {
                        #[doc = #field_doc]
                        pub fn #field_name(&self) -> #hashmap_wrapper_name<#struct_name, #module_name::#lens_struct_name> {
                            #hashmap_wrapper_name::from_binding(self.inner.binding_with_lens(#module_name::#lens_struct_name))
                        }
                    }
                }
                NestedKind::IndexMap { .. } => {
                    // For IndexMap nested fields, return the IndexMap wrapper type
                    let indexmap_wrapper_name =
                        format_ident!("{}IndexMapBinding", to_pascal_case(&field_name.to_string()));
                    quote! {
                        #[doc = #field_doc]
                        pub fn #field_name(&self) -> #indexmap_wrapper_name<#struct_name, #module_name::#lens_struct_name> {
                            #indexmap_wrapper_name::from_binding(self.inner.binding_with_lens(#module_name::#lens_struct_name))
                        }
                    }
                }
                NestedKind::None => {
                    // For non-nested fields, return raw Binding
                    quote! {
                        #[doc = #field_doc]
                        pub fn #field_name(&self) -> floem_store::Binding<#struct_name, #field_type, #module_name::#lens_struct_name> {
                            self.inner.binding_with_lens(#module_name::#lens_struct_name)
                        }
                    }
                }
            }
        })
        .collect();

    // Generate reconcile field statements for the binding wrapper
    // This generates code that only updates fields that have changed
    let reconcile_field_stmts: Vec<_> = field_info
        .iter()
        .map(|(field_name, _field_type, _lens_struct_name, nested_kind)| {
            match nested_kind {
                NestedKind::Direct(_type_ident) => {
                    // For nested fields, call reconcile recursively
                    quote! {
                        self.#field_name().reconcile(&new_value.#field_name);
                    }
                }
                NestedKind::Vec { key_field: Some(key_field), .. } => {
                    // Keyed Vec reconciliation:
                    // - If keys are in same order, reconcile each item individually
                    // - If structure changed (keys differ or reordered), replace the whole Vec
                    quote! {
                        {
                            let new_items = &new_value.#field_name;

                            // Check if structure matches (same keys in same order)
                            let structure_matches = self.with_untracked(|v| {
                                if v.#field_name.len() != new_items.len() {
                                    return false;
                                }
                                v.#field_name.iter().zip(new_items.iter())
                                    .all(|(old, new)| old.#key_field == new.#key_field)
                            });

                            if structure_matches {
                                // Same structure - reconcile each item individually
                                for (idx, new_item) in new_items.iter().enumerate() {
                                    self.#field_name().index(idx).reconcile(new_item);
                                }
                            } else {
                                // Structure changed - replace the whole Vec
                                self.#field_name().inner().set(new_items.clone());
                            }
                        }
                    }
                }
                NestedKind::Vec { key_field: None, .. } | NestedKind::HashMap(_) => {
                    // For non-keyed Vec and HashMap nested fields, compare whole collection and replace if different
                    // Do comparison inside closure to avoid lifetime issues
                    // Use the binding wrapper's method to ensure same lens path as user bindings
                    quote! {
                        {
                            let new_field = &new_value.#field_name;
                            let changed = self.with_untracked(|v| &v.#field_name != new_field);
                            if changed {
                                self.#field_name().inner().set(new_value.#field_name.clone());
                            }
                        }
                    }
                }
                NestedKind::IndexMap { key_field: Some(_key_field), .. } => {
                    // IndexMap with key_field: reconcile items with matching keys
                    quote! {
                        {
                            let new_items = &new_value.#field_name;

                            // Check if structure matches (same keys in same order)
                            let structure_matches = self.with_untracked(|v| {
                                if v.#field_name.len() != new_items.len() {
                                    return false;
                                }
                                v.#field_name.keys().zip(new_items.keys())
                                    .all(|(old_k, new_k)| old_k == new_k)
                            });

                            if structure_matches {
                                // Same structure - reconcile each item individually by key
                                for (key, new_item) in new_items.iter() {
                                    self.#field_name().get(*key).reconcile(new_item);
                                }
                            } else {
                                // Structure changed - replace the whole IndexMap
                                self.#field_name().inner().set(new_items.clone());
                            }
                        }
                    }
                }
                NestedKind::IndexMap { key_field: None, .. } => {
                    // IndexMap without key_field: compare whole collection and replace if different
                    quote! {
                        {
                            let new_field = &new_value.#field_name;
                            let changed = self.with_untracked(|v| &v.#field_name != new_field);
                            if changed {
                                self.#field_name().inner().set(new_value.#field_name.clone());
                            }
                        }
                    }
                }
                NestedKind::None => {
                    // For non-nested fields, compare and set if different
                    // Do comparison inside closure to avoid lifetime issues
                    quote! {
                        {
                            let new_field = &new_value.#field_name;
                            let changed = self.with_untracked(|v| &v.#field_name != new_field);
                            if changed {
                                self.#field_name().set(new_value.#field_name.clone());
                            }
                        }
                    }
                }
            }
        })
        .collect();

    // Generate binding wrapper methods
    // For #[nested] fields, return the wrapper type; otherwise return raw Binding
    let binding_wrapper_methods: Vec<_> = field_info
        .iter()
        .map(|(field_name, field_type, lens_struct_name, nested_kind)| {
            let field_doc = format!("Get a binding for the `{}` field.", field_name);
            match nested_kind {
                NestedKind::Direct(type_ident) => {
                    // For direct nested fields, return the wrapper type (e.g., UserBinding)
                    let nested_binding_wrapper = format_ident!("{}Binding", type_ident);
                    quote! {
                        #[doc = #field_doc]
                        pub fn #field_name(&self) -> #nested_binding_wrapper<
                            __Root,
                            floem_store::ComposedLens<__L, #module_name::#lens_struct_name, #struct_name>
                        > {
                            #nested_binding_wrapper::from_binding(self.inner.binding_with_lens(#module_name::#lens_struct_name))
                        }
                    }
                }
                NestedKind::Vec { .. } => {
                    // For Vec nested fields, return the Vec wrapper type
                    let vec_wrapper_name =
                        format_ident!("{}VecBinding", to_pascal_case(&field_name.to_string()));
                    quote! {
                        #[doc = #field_doc]
                        pub fn #field_name(&self) -> #vec_wrapper_name<
                            __Root,
                            floem_store::ComposedLens<__L, #module_name::#lens_struct_name, #struct_name>
                        > {
                            #vec_wrapper_name::from_binding(self.inner.binding_with_lens(#module_name::#lens_struct_name))
                        }
                    }
                }
                NestedKind::HashMap(_val_ident) => {
                    // For HashMap nested fields, return the HashMap wrapper type
                    let hashmap_wrapper_name =
                        format_ident!("{}HashMapBinding", to_pascal_case(&field_name.to_string()));
                    quote! {
                        #[doc = #field_doc]
                        pub fn #field_name(&self) -> #hashmap_wrapper_name<
                            __Root,
                            floem_store::ComposedLens<__L, #module_name::#lens_struct_name, #struct_name>
                        > {
                            #hashmap_wrapper_name::from_binding(self.inner.binding_with_lens(#module_name::#lens_struct_name))
                        }
                    }
                }
                NestedKind::IndexMap { .. } => {
                    // For IndexMap nested fields, return the IndexMap wrapper type
                    let indexmap_wrapper_name =
                        format_ident!("{}IndexMapBinding", to_pascal_case(&field_name.to_string()));
                    quote! {
                        #[doc = #field_doc]
                        pub fn #field_name(&self) -> #indexmap_wrapper_name<
                            __Root,
                            floem_store::ComposedLens<__L, #module_name::#lens_struct_name, #struct_name>
                        > {
                            #indexmap_wrapper_name::from_binding(self.inner.binding_with_lens(#module_name::#lens_struct_name))
                        }
                    }
                }
                NestedKind::None => {
                    // For non-nested fields, return raw Binding
                    quote! {
                        #[doc = #field_doc]
                        pub fn #field_name(&self) -> floem_store::Binding<
                            __Root,
                            #field_type,
                            floem_store::ComposedLens<__L, #module_name::#lens_struct_name, #struct_name>
                        > {
                            self.inner.binding_with_lens(#module_name::#lens_struct_name)
                        }
                    }
                }
            }
        })
        .collect();

    let module_doc = format!(
        "Auto-generated lens types for [`{}`].\n\n\
        Contains lens structs for each field that can be used with `binding_with_lens()`.\n\
        For most use cases, prefer using the wrapper types ([`{}Store`], [`{}Binding`])\n\
        which provide direct method access without imports.",
        struct_name, struct_name, struct_name
    );

    let store_wrapper_doc = format!(
        "Wrapper around `Store<{}>` with direct field access methods.\n\n\
        This wrapper provides method-style access without requiring trait imports.\n\n\
        # Example\n\n\
        ```rust,ignore\n\
        let store = {}Store::new({}::default());\n\
        let field = store.field_name();  // No import needed!\n\
        ```",
        struct_name, struct_name, struct_name
    );

    let binding_wrapper_doc = format!(
        "Wrapper around `Binding<Root, {}, L>` with direct field access methods.\n\n\
        This wrapper provides method-style access without requiring trait imports.",
        struct_name
    );

    let expanded = quote! {
        #[doc = #module_doc]
        pub mod #module_name {
            use super::*;

            #(#lens_impls)*
        }

        #(#vec_wrapper_impls)*

        #(#hashmap_wrapper_impls)*

        #(#indexmap_wrapper_impls)*

        #[doc = #store_wrapper_doc]
        pub struct #store_wrapper_name {
            inner: floem_store::Store<#struct_name>,
        }

        impl #store_wrapper_name {
            /// Create a new store wrapper with the given initial value.
            pub fn new(value: #struct_name) -> Self {
                Self {
                    inner: floem_store::Store::new(value),
                }
            }

            /// Get the underlying Store.
            pub fn inner(&self) -> &floem_store::Store<#struct_name> {
                &self.inner
            }

            /// Get a Binding for the root of the store.
            pub fn root(&self) -> #binding_wrapper_name<#struct_name, floem_store::lens::IdentityLens<#struct_name>> {
                #binding_wrapper_name {
                    inner: self.inner.root(),
                }
            }

            /// Read the entire state.
            pub fn with<R>(&self, f: impl FnOnce(&#struct_name) -> R) -> R {
                self.inner.with(f)
            }

            /// Update the entire state.
            pub fn update(&self, f: impl FnOnce(&mut #struct_name)) {
                self.inner.update(f);
            }

            /// Reconcile the store state with new data, only updating changed fields.
            ///
            /// This is useful for syncing with server data without triggering
            /// unnecessary updates for unchanged fields.
            pub fn reconcile(&self, new_value: &#struct_name) {
                self.root().reconcile(new_value);
            }

            #(#store_wrapper_methods)*
        }

        impl Default for #store_wrapper_name
        where
            #struct_name: Default,
        {
            fn default() -> Self {
                Self::new(#struct_name::default())
            }
        }

        impl Clone for #store_wrapper_name {
            fn clone(&self) -> Self {
                Self {
                    inner: self.inner.clone(),
                }
            }
        }

        #[doc = #binding_wrapper_doc]
        pub struct #binding_wrapper_name<__Root: 'static, __L: floem_store::Lens<__Root, #struct_name>> {
            inner: floem_store::Binding<__Root, #struct_name, __L>,
        }

        impl<__Root: 'static, __L: floem_store::Lens<__Root, #struct_name>> #binding_wrapper_name<__Root, __L> {
            /// Create a wrapper from a raw Binding.
            pub fn from_binding(binding: floem_store::Binding<__Root, #struct_name, __L>) -> Self {
                Self { inner: binding }
            }

            /// Get the underlying Binding.
            pub fn inner(&self) -> &floem_store::Binding<__Root, #struct_name, __L> {
                &self.inner
            }

            /// Consume wrapper and return the underlying Binding.
            pub fn into_inner(self) -> floem_store::Binding<__Root, #struct_name, __L> {
                self.inner
            }

            /// Set the value.
            pub fn set(&self, value: #struct_name) {
                self.inner.set(value);
            }

            /// Update the value with a closure.
            pub fn update(&self, f: impl FnOnce(&mut #struct_name)) {
                self.inner.update(f);
            }

            /// Read the value by reference.
            pub fn with<R>(&self, f: impl FnOnce(&#struct_name) -> R) -> R {
                self.inner.with(f)
            }

            /// Read the value by reference without subscribing to changes.
            pub fn with_untracked<R>(&self, f: impl FnOnce(&#struct_name) -> R) -> R {
                self.inner.with_untracked(f)
            }

            /// Reconcile this binding with new data, only updating fields that changed.
            ///
            /// This is useful when receiving data from a server - instead of replacing
            /// the entire value (which would notify all subscribers), this only updates
            /// fields that are actually different.
            ///
            /// # Example
            ///
            /// ```rust,ignore
            /// // Server returns new data
            /// let server_data = fetch_from_server();
            ///
            /// // Only changed fields are updated, minimizing re-renders
            /// binding.reconcile(&server_data);
            /// ```
            pub fn reconcile(&self, new_value: &#struct_name)
            where
                #struct_name: PartialEq + Clone,
            {
                #(#reconcile_field_stmts)*
            }

            #(#binding_wrapper_methods)*
        }

        impl<__Root: 'static, __L: floem_store::Lens<__Root, #struct_name>> Clone for #binding_wrapper_name<__Root, __L> {
            fn clone(&self) -> Self {
                Self {
                    inner: self.inner.clone(),
                }
            }
        }
    };

    TokenStream::from(expanded)
}

/// Convert a string to snake_case.
fn to_snake_case(s: &str) -> String {
    let mut result = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() {
            if i > 0 {
                result.push('_');
            }
            result.push(c.to_ascii_lowercase());
        } else {
            result.push(c);
        }
    }
    result
}

/// Convert a string to PascalCase.
fn to_pascal_case(s: &str) -> String {
    let mut result = String::new();
    let mut capitalize_next = true;
    for c in s.chars() {
        if c == '_' {
            capitalize_next = true;
        } else if capitalize_next {
            result.push(c.to_ascii_uppercase());
            capitalize_next = false;
        } else {
            result.push(c);
        }
    }
    result
}
