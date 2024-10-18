use proc_macro::TokenStream;
use quote::{format_ident, quote};
#[allow(unused_imports)]
use syn::{
    parse::Parse, parse2, parse_macro_input, punctuated::Punctuated, Attribute, Data, DeriveInput,
    Fields, Ident, Path, Token, Visibility,
};

struct StateArgs {
    derives: Punctuated<Path, Token![,]>,
}

impl Parse for StateArgs {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        Ok(StateArgs {
            derives: input.parse_terminated(Path::parse, Token![,])?,
        })
    }
}

#[proc_macro_derive(
    State,
    attributes(state_skip, state_read_only, state_write_only, state_derives)
)]
pub fn derive_state(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let struct_name = &input.ident;
    let state_struct_name = format_ident!("{}State", struct_name);

    let state_derives = input
        .attrs
        .iter()
        .find(|attr| attr.path().is_ident("state_derives"))
        .and_then(|attr| attr.parse_args::<StateArgs>().ok())
        .map(|args| args.derives)
        .unwrap_or_else(Punctuated::new);

    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => &fields.named,
            _ => panic!("Only named fields are supported"),
        },
        _ => panic!("State can only be derived for structs"),
    };

    let state_fields = fields.iter().map(|f| {
        let name = &f.ident;
        let ty = &f.ty;
        let vis = &f.vis;

        let skip = f
            .attrs
            .iter()
            .any(|attr| attr.path().is_ident("state_skip"));
        let read_only = f
            .attrs
            .iter()
            .any(|attr| attr.path().is_ident("state_read_only"));
        let write_only = f
            .attrs
            .iter()
            .any(|attr| attr.path().is_ident("state_write_only"));

        if skip {
            quote! { #vis #name: #ty }
        } else if read_only {
            quote! { #vis #name: ::floem::reactive::ReadSignal<#ty> }
        } else if write_only {
            quote! { #vis #name: ::floem::reactive::WriteSignal<#ty> }
        } else {
            quote! { #vis #name: ::floem::reactive::RwSignal<#ty> }
        }
    });

    let impl_fields = fields.iter().map(|f| {
        let name = &f.ident;

        let skip = f
            .attrs
            .iter()
            .any(|attr| attr.path().is_ident("state_skip"));
        let read_only = f
            .attrs
            .iter()
            .any(|attr| attr.path().is_ident("state_read_only"));
        let write_only = f
            .attrs
            .iter()
            .any(|attr| attr.path().is_ident("state_write_only"));

        if skip {
            quote! { #name: self.#name }
        } else if read_only {
            quote! { #name: ::floem::reactive::create_signal(self.#name).0 }
        } else if write_only {
            quote! { #name: ::floem::reactive::create_signal(self.#name).1 }
        } else {
            quote! { #name: ::floem::reactive::create_rw_signal(self.#name) }
        }
    });

    let derive_list = if state_derives.is_empty() {
        quote! {}
    } else {
        let derives = state_derives.iter().collect::<Vec<_>>();
        quote! { #[derive(#(#derives),*)] }
    };

    let expanded = quote! {
        #derive_list
        pub struct #state_struct_name {
            #(#state_fields,)*
        }

        impl #struct_name {
            pub fn to_state(self) -> #state_struct_name {
                #state_struct_name {
                    #(#impl_fields,)*
                }
            }
        }
    };

    TokenStream::from(expanded)
}
