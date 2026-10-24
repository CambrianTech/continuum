//! `#[derive(Packable)]`: one line per type. Struct attribute
//! `#[pack(kind = "gene", schema = 3, migrate = GENE_MIGRATIONS)]` (migrate optional, an
//! expression of type `&'static [continuum_pack::Migration]`); field attributes name each
//! field's part: `#[pack(blob = "media/type")]` (a `Blob`), `#[pack(child)]` (a `Handle<T>`
//! or `Vec<Handle<T>>`), `#[pack(requires)]` (a `Required`); an unmarked field is a `field`.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as Tokens;
use quote::quote;
use syn::{parse_macro_input, Data, DeriveInput, Expr, Fields, Ident, LitInt, LitStr};

#[proc_macro_derive(Packable, attributes(pack))]
pub fn derive_packable(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match expand(&input) {
        Ok(tokens) => tokens.into(),
        Err(e) => e.into_compile_error().into(),
    }
}

enum Role {
    Field,
    Blob(LitStr),
    Child,
    Requires,
}

fn expand(input: &DeriveInput) -> syn::Result<Tokens> {
    if !input.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &input.generics,
            "Packable cannot be derived for a generic type; write pack/unpack by hand",
        ));
    }
    let Data::Struct(data) = &input.data else {
        return Err(syn::Error::new_spanned(
            &input.ident,
            "Packable can be derived only for a struct with named fields",
        ));
    };
    let Fields::Named(named) = &data.fields else {
        return Err(syn::Error::new_spanned(
            &input.ident,
            "Packable can be derived only for a struct with named fields",
        ));
    };

    let (mut kind, mut schema, mut migrate): (Option<LitStr>, Option<LitInt>, Option<Expr>) =
        (None, None, None);
    for attr in input.attrs.iter().filter(|a| a.path().is_ident("pack")) {
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("kind") {
                kind = Some(meta.value()?.parse()?);
            } else if meta.path.is_ident("schema") {
                schema = Some(meta.value()?.parse()?);
            } else if meta.path.is_ident("migrate") {
                migrate = Some(meta.value()?.parse()?);
            } else {
                return Err(meta.error("expected `kind`, `schema` or `migrate`"));
            }
            Ok(())
        })?;
    }
    let kind = kind
        .ok_or_else(|| syn::Error::new_spanned(&input.ident, "missing #[pack(kind = \"...\")]"))?;
    let schema = schema
        .ok_or_else(|| syn::Error::new_spanned(&input.ident, "missing #[pack(schema = N)]"))?;
    let schema_n: u32 = schema.base10_parse()?;
    if schema_n == 0 {
        return Err(syn::Error::new_spanned(&schema, "schemas start at 1"));
    }

    let mut packs = Vec::new();
    let mut unpacks = Vec::new();
    for field in &named.named {
        let Some(ident) = field.ident.as_ref() else {
            return Err(syn::Error::new_spanned(field, "unnamed field"));
        };
        let name = ident.to_string();
        let (pack, unpack) = match role(field, ident)? {
            Role::Field => (
                quote! { p.field(#name, &self.#ident)?; },
                quote! { u.field(#name)? },
            ),
            Role::Blob(media) => (
                quote! { p.blob_value(#name, #media, &self.#ident)?; },
                quote! { u.blob(#name, #media)? },
            ),
            Role::Child => (
                quote! { ::continuum_pack::ChildPart::pack_part(&self.#ident, #name, p)?; },
                quote! { ::continuum_pack::ChildPart::unpack_part(#name, u)? },
            ),
            Role::Requires => (
                quote! { p.requires(#name, ::continuum_pack::Required::to_requirement(&self.#ident))?; },
                quote! { ::continuum_pack::Required::from_requirement(#name, u.requirement(#name)?)? },
            ),
        };
        packs.push(pack);
        unpacks.push(quote! { #ident: #unpack });
    }

    let ty = &input.ident;
    let migrations = migrate.map(|m| {
        quote! {
            fn migrations() -> &'static [::continuum_pack::Migration] { #m }
        }
    });
    Ok(quote! {
        impl ::continuum_pack::Packable for #ty {
            const KIND: ::continuum_pack::Kind = ::continuum_pack::Kind::new(#kind, #schema_n);
            type Context = ();
            fn pack(&self, p: &mut ::continuum_pack::Packer<'_>) -> ::core::result::Result<(), ::continuum_pack::PackError> {
                #(#packs)*
                ::core::result::Result::Ok(())
            }
            fn unpack(u: &mut ::continuum_pack::Unpacker<'_>, _cx: &()) -> ::core::result::Result<Self, ::continuum_pack::PackError> {
                ::core::result::Result::Ok(Self { #(#unpacks),* })
            }
            #migrations
        }
    })
}

fn role(field: &syn::Field, ident: &Ident) -> syn::Result<Role> {
    let mut role = Role::Field;
    let mut seen = false;
    for attr in field.attrs.iter().filter(|a| a.path().is_ident("pack")) {
        attr.parse_nested_meta(|meta| {
            if seen {
                return Err(meta.error(format!("field `{ident}` names more than one part role")));
            }
            seen = true;
            if meta.path.is_ident("blob") {
                role = Role::Blob(meta.value()?.parse()?);
            } else if meta.path.is_ident("child") {
                role = Role::Child;
            } else if meta.path.is_ident("requires") {
                role = Role::Requires;
            } else {
                return Err(meta.error("expected `blob = \"media/type\"`, `child` or `requires`"));
            }
            Ok(())
        })?;
    }
    Ok(role)
}
