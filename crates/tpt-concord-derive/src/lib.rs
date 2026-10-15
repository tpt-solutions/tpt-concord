// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright 2026 TPT Solutions

//! `#[derive(SpecState)]` — map a Rust struct onto Concord specification state.
//!
//! ```ignore
//! use tpt_concord_spec::SpecState;
//!
//! #[derive(SpecState)]
//! #[spec(id = "spec.wal", version = "1.0.0", description = "WAL state")]
//! struct Wal {
//!     committed: u64,
//!     #[spec(rename = "in_flight")]
//!     pending: Vec<u64>,
//!     #[spec(skip)]
//!     scratch: Vec<u8>,
//! }
//! ```
//!
//! This implements `tpt_concord_spec::SpecState` for the struct:
//!
//! - `state_vars()` declares each non-skipped field with its spec type, taken
//!   from the field's `SpecValue` impl (so an unsupported field type is a
//!   compile error, not a silent mismatch);
//! - `to_spec_state(&self)` reads the current field values into a spec state,
//!   which is what an implementation step is compared against;
//! - `specification()` builds the skeleton with the identity from `#[spec]`.
//!
//! Attributes: container `#[spec(id = "..", version = "..", description = "..")]`
//! (`id` defaults to `spec.<snake_case struct name>`, `version` to `0.1.0`);
//! field `#[spec(rename = "..")]` and `#[spec(skip)]`.

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, Data, DeriveInput, Error, Fields, Ident, LitStr, Result};

#[proc_macro_derive(SpecState, attributes(spec))]
pub fn derive_spec_state(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match expand(&input) {
        Ok(tokens) => tokens.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

#[derive(Default)]
struct ContainerAttrs {
    id: Option<String>,
    version: Option<String>,
    description: Option<String>,
}

#[derive(Default)]
struct FieldAttrs {
    rename: Option<String>,
    skip: bool,
}

fn container_attrs(input: &DeriveInput) -> Result<ContainerAttrs> {
    let mut out = ContainerAttrs::default();
    for attr in input.attrs.iter().filter(|a| a.path().is_ident("spec")) {
        attr.parse_nested_meta(|meta| {
            let value = |meta: &syn::meta::ParseNestedMeta| -> Result<String> {
                Ok(meta.value()?.parse::<LitStr>()?.value())
            };
            if meta.path.is_ident("id") {
                out.id = Some(value(&meta)?);
            } else if meta.path.is_ident("version") {
                out.version = Some(value(&meta)?);
            } else if meta.path.is_ident("description") {
                out.description = Some(value(&meta)?);
            } else {
                return Err(
                    meta.error("unknown container attribute; expected id, version or description")
                );
            }
            Ok(())
        })?;
    }
    Ok(out)
}

fn field_attrs(field: &syn::Field) -> Result<FieldAttrs> {
    let mut out = FieldAttrs::default();
    for attr in field.attrs.iter().filter(|a| a.path().is_ident("spec")) {
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("skip") {
                out.skip = true;
            } else if meta.path.is_ident("rename") {
                out.rename = Some(meta.value()?.parse::<LitStr>()?.value());
            } else {
                return Err(meta.error("unknown field attribute; expected rename or skip"));
            }
            Ok(())
        })?;
    }
    Ok(out)
}

/// `CamelCase` → `camel_case`, for the default specification id.
fn snake_case(ident: &Ident) -> String {
    let name = ident.to_string();
    let mut out = String::new();
    for (i, c) in name.chars().enumerate() {
        if c.is_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.extend(c.to_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

fn expand(input: &DeriveInput) -> Result<proc_macro2::TokenStream> {
    let name = &input.ident;
    if !input.generics.params.is_empty() {
        return Err(Error::new_spanned(
            &input.generics,
            "#[derive(SpecState)] does not support generic structs",
        ));
    }
    let Data::Struct(data) = &input.data else {
        return Err(Error::new_spanned(
            name,
            "#[derive(SpecState)] only supports structs",
        ));
    };
    let Fields::Named(fields) = &data.fields else {
        return Err(Error::new_spanned(
            name,
            "#[derive(SpecState)] requires a struct with named fields",
        ));
    };

    let container = container_attrs(input)?;
    let id = container
        .id
        .unwrap_or_else(|| format!("spec.{}", snake_case(name)));
    let version = container.version.unwrap_or_else(|| "0.1.0".to_string());
    let description = container.description.unwrap_or_default();
    let display_name = name.to_string();

    let mut declared = Vec::new();
    let mut read = Vec::new();
    for field in &fields.named {
        let attrs = field_attrs(field)?;
        if attrs.skip {
            continue;
        }
        let ident = field.ident.as_ref().expect("named field");
        let ty = &field.ty;
        let key = attrs.rename.unwrap_or_else(|| ident.to_string());
        declared.push(quote! {
            vars.insert(
                #key.to_string(),
                <#ty as ::tpt_concord_spec::SpecValue>::value_type(),
            );
        });
        read.push(quote! {
            state.insert(
                #key.to_string(),
                <#ty as ::tpt_concord_spec::SpecValue>::to_spec_value(&self.#ident),
            );
        });
    }

    Ok(quote! {
        impl ::tpt_concord_spec::SpecState for #name {
            fn state_vars() -> ::std::collections::BTreeMap<
                ::std::string::String,
                ::tpt_concord_spec::ValueType,
            > {
                let mut vars = ::std::collections::BTreeMap::new();
                #(#declared)*
                vars
            }

            fn to_spec_state(&self) -> ::tpt_concord_spec::State {
                let mut state = ::std::collections::BTreeMap::new();
                #(#read)*
                state
            }

            fn specification() -> ::tpt_concord_spec::Specification {
                let mut spec = ::tpt_concord_spec::skeleton(
                    #id,
                    #version,
                    #display_name,
                    #description,
                );
                spec.state_vars = <Self as ::tpt_concord_spec::SpecState>::state_vars();
                spec
            }
        }
    })
}
