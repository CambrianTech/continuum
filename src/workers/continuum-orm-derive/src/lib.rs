//! `#[derive(Entity)]` — the Rust analogue of TS class decorators for
//! continuum-core's ORM.
//!
//! ### What it does
//!
//! Walks the struct's fields, infers each field's `FieldType` from the
//! Rust type, honors per-field `#[entity(...)]` attribute overrides,
//! and emits `impl OrmEntity for #name` automatically. The hand-written
//! 100-line `collection_schema()` block collapses to per-field
//! attributes — and structural drift between the struct and the schema
//! becomes impossible: the schema IS the struct.
//!
//! ### Doctrine
//!
//! Per [[orm-everything-not-hand-edited-files]] +
//! [[no-sql-everything-through-orm-entities]] + Joel's 2026-06-03
//! directive ("Entities need to be defined, and in one place, rust"):
//! the Rust struct is the single source of truth for what an entity
//! IS. Everything else — the ORM schema, the ts-rs TS type, future
//! JSON Schema / OpenAPI / IPC payloads — is generated. Drift is a
//! structural impossibility, not a discipline problem.
//!
//! ### Example
//!
//! ```ignore
//! use continuum_orm_derive::Entity;
//! use continuum_core::orm::BaseEntity;
//! use serde::{Deserialize, Serialize};
//! use uuid::Uuid;
//!
//! #[derive(Debug, Clone, Serialize, Deserialize, Entity)]
//! #[entity(collection = "engrams")]
//! pub struct Engram {
//!     #[serde(flatten)]
//!     pub base: BaseEntity,
//!     #[entity(indexed)]
//!     pub kind: String,
//!     pub content: String,
//!     #[entity(json)]
//!     pub origin: serde_json::Value,
//!     #[entity(indexed)]
//!     pub admitted_at_ms: u64,
//!     pub admission_trace_id: Option<String>,
//! }
//! ```
//!
//! ### Type inference rules
//!
//! | Rust type                       | FieldType          | Notes                       |
//! |---------------------------------|--------------------|-----------------------------|
//! | `String` / `&str`               | `String`           |                             |
//! | `Uuid`                          | `Uuid`             | Detected by type-name match |
//! | `bool`                          | `Boolean`          |                             |
//! | `u*` / `i*` / `f32` / `f64`     | `Number`           |                             |
//! | `Vec<T>` / `HashMap` / `BTreeMap` | `Json`           | JSON-serialized container   |
//! | `Option<T>`                     | inner T's FieldType + nullable | unwrapped one level     |
//! | Enum (variant-only)             | `String`           | Serializes as variant name  |
//! | Any other named struct          | `Json`             | Nested struct → JSON column |
//!
//! Override with `#[entity(json)]` to force any field to a JSON column
//! regardless of inferred type. The `BaseEntity` field marked
//! `#[serde(flatten)]` is detected by type name and its columns are
//! added via `base_entity_fields()` rather than appearing as a
//! single JSON column.
//!
//! ### Field-level attributes
//!
//! - `#[entity(indexed)]` — single-field index
//! - `#[entity(unique)]` — unique constraint
//! - `#[entity(nullable)]` — explicit nullable (auto for `Option<T>`)
//! - `#[entity(json)]` — force JSON column (override inferred type)
//! - `#[entity(skip)]` — exclude from the schema (in-memory only)
//! - `#[entity(foreign_key("engrams.id"))]` — declare a foreign-key
//!   reference. Cascade rules default to `Restrict` on delete and
//!   update; override with
//!   `foreign_key("engrams.id", on_delete = "cascade", on_update = "restrict")`.
//!   Cascade keywords: `"restrict" | "cascade" | "set_null" | "no_action"`.
//!
//! ### Struct-level attributes
//!
//! - `#[entity(collection = "name")]` — REQUIRED. The collection name.
//! - `#[entity(index(name = "idx_x", fields = ["a", "b"]))]` — composite
//!   index across multiple fields. Add `unique = true` for a unique
//!   composite. Repeat the attribute for multiple indexes.

use proc_macro::TokenStream;
use quote::quote;
use syn::{
    parse_macro_input, Data, DeriveInput, Field, Fields, GenericArgument, PathArguments, Type,
};

/// Derive `impl OrmEntity for #name { ... }` from the struct
/// definition + `#[entity(...)]` attributes.
#[proc_macro_derive(Entity, attributes(entity))]
pub fn derive_entity(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as DeriveInput);
    let name = &ast.ident;

    // Collection name + composite indexes from struct-level
    // #[entity(collection = "...")] and #[entity(index(...))] attrs.
    let (collection, composite_indexes) = match extract_struct_meta(&ast.attrs) {
        Ok(meta) => meta,
        Err(e) => return e.to_compile_error().into(),
    };

    // Extract struct fields. Only named-fields structs are supported;
    // tuple structs + unit structs have no logical schema.
    let fields = match &ast.data {
        Data::Struct(s) => match &s.fields {
            Fields::Named(named) => &named.named,
            _ => {
                return syn::Error::new_spanned(
                    name,
                    "Entity derive requires a struct with named fields",
                )
                .to_compile_error()
                .into()
            }
        },
        _ => {
            return syn::Error::new_spanned(name, "Entity derive supports structs only")
                .to_compile_error()
                .into()
        }
    };

    // Walk fields, emit SchemaField construction for each. The
    // BaseEntity field (recognized by type name) inserts the base
    // columns via `base_entity_fields()` rather than appearing as
    // one big JSON column.
    let mut field_pushes = Vec::new();
    let mut saw_base = false;

    for field in fields {
        let field_info = match parse_field(field) {
            Ok(info) => info,
            Err(e) => return e.to_compile_error().into(),
        };

        if field_info.skip {
            continue;
        }

        if field_info.is_base_entity {
            if saw_base {
                return syn::Error::new_spanned(
                    field,
                    "duplicate BaseEntity field — only one allowed per entity",
                )
                .to_compile_error()
                .into();
            }
            saw_base = true;
            field_pushes.push(quote! {
                fields.extend(::continuum_core::orm::base_entity_fields());
            });
            continue;
        }

        let name_str = field_info.serde_name;
        let field_type = field_info.field_type;
        let indexed = field_info.indexed;
        let unique = field_info.unique;
        let nullable = field_info.nullable;

        let field_type_tokens = match field_type {
            InferredFieldType::String => quote!(::continuum_core::orm::FieldType::String),
            InferredFieldType::Number => quote!(::continuum_core::orm::FieldType::Number),
            InferredFieldType::Boolean => quote!(::continuum_core::orm::FieldType::Boolean),
            InferredFieldType::Date => quote!(::continuum_core::orm::FieldType::Date),
            InferredFieldType::Json => quote!(::continuum_core::orm::FieldType::Json),
            InferredFieldType::Uuid => quote!(::continuum_core::orm::FieldType::Uuid),
        };

        let fk_tokens = match &field_info.foreign_key {
            Some(fk) => {
                let target_collection = &fk.target_collection;
                let target_field = &fk.target_field;
                let on_delete = cascade_rule_tokens(&fk.on_delete);
                let on_update = cascade_rule_tokens(&fk.on_update);
                quote! {
                    Some(::continuum_core::orm::ForeignKeyRef {
                        collection: #target_collection.to_string(),
                        field: #target_field.to_string(),
                        on_delete: #on_delete,
                        on_update: #on_update,
                    })
                }
            }
            None => quote!(None),
        };

        field_pushes.push(quote! {
            fields.push(::continuum_core::orm::SchemaField {
                name: #name_str.to_string(),
                field_type: #field_type_tokens,
                indexed: #indexed,
                unique: #unique,
                nullable: #nullable,
                max_length: None,
                foreign_key: #fk_tokens,
            });
        });
    }

    // Composite-index emission. Struct-level
    // #[entity(index(name = "...", fields = [...], unique = ...))]
    // declarations land here. Each becomes a SchemaIndex pushed into
    // the indexes vec the adapter walks at CREATE TABLE time.
    let mut index_pushes = Vec::new();
    for idx in composite_indexes {
        let name = idx.name;
        let fields_lit = idx.fields.iter().map(|f| quote!(#f.to_string()));
        let unique = idx.unique;
        index_pushes.push(quote! {
            indexes.push(::continuum_core::orm::SchemaIndex {
                name: #name.to_string(),
                fields: ::std::vec![#(#fields_lit),*],
                unique: #unique,
            });
        });
    }

    let expanded = quote! {
        impl ::continuum_core::orm::OrmEntity for #name {
            const COLLECTION: &'static str = #collection;

            fn collection_schema() -> ::continuum_core::orm::CollectionSchema {
                let mut fields = ::std::vec::Vec::new();
                #(#field_pushes)*
                let mut indexes = ::std::vec::Vec::new();
                #(#index_pushes)*
                ::continuum_core::orm::CollectionSchema {
                    collection: Self::COLLECTION.to_string(),
                    fields,
                    indexes,
                }
            }
        }
    };

    TokenStream::from(expanded)
}

// ─── Attribute parsing ────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
enum InferredFieldType {
    String,
    Number,
    Boolean,
    #[allow(dead_code)]
    Date,
    Json,
    Uuid,
}

struct FieldInfo {
    serde_name: String,
    field_type: InferredFieldType,
    indexed: bool,
    unique: bool,
    nullable: bool,
    skip: bool,
    is_base_entity: bool,
    foreign_key: Option<ForeignKeyAttr>,
}

/// Parsed `#[entity(foreign_key("collection.field"[, on_delete = "..."][, on_update = "..."]))]`
/// representation. Lowered to `ForeignKeyRef` tokens at codegen time.
struct ForeignKeyAttr {
    target_collection: String,
    target_field: String,
    on_delete: CascadeRuleAttr,
    on_update: CascadeRuleAttr,
}

#[derive(Debug, Clone, Copy)]
enum CascadeRuleAttr {
    Restrict,
    Cascade,
    SetNull,
    NoAction,
}

impl Default for CascadeRuleAttr {
    fn default() -> Self {
        CascadeRuleAttr::Restrict
    }
}

fn parse_cascade_rule(s: &str) -> Option<CascadeRuleAttr> {
    match s {
        "restrict" => Some(CascadeRuleAttr::Restrict),
        "cascade" => Some(CascadeRuleAttr::Cascade),
        "set_null" | "setnull" => Some(CascadeRuleAttr::SetNull),
        "no_action" | "noaction" => Some(CascadeRuleAttr::NoAction),
        _ => None,
    }
}

fn cascade_rule_tokens(rule: &CascadeRuleAttr) -> proc_macro2::TokenStream {
    match rule {
        CascadeRuleAttr::Restrict => quote!(::continuum_core::orm::CascadeRule::Restrict),
        CascadeRuleAttr::Cascade => quote!(::continuum_core::orm::CascadeRule::Cascade),
        CascadeRuleAttr::SetNull => quote!(::continuum_core::orm::CascadeRule::SetNull),
        CascadeRuleAttr::NoAction => quote!(::continuum_core::orm::CascadeRule::NoAction),
    }
}

/// Parsed `#[entity(index(name = "...", fields = ["a", "b"], unique = true))]`
/// representation. Lowered to `SchemaIndex` tokens at codegen time.
struct CompositeIndexAttr {
    name: String,
    fields: Vec<String>,
    unique: bool,
}

fn extract_struct_meta(
    attrs: &[syn::Attribute],
) -> syn::Result<(String, Vec<CompositeIndexAttr>)> {
    let mut collection: Option<String> = None;
    let mut indexes: Vec<CompositeIndexAttr> = Vec::new();

    for attr in attrs {
        if !attr.path().is_ident("entity") {
            continue;
        }
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("collection") {
                let value: syn::LitStr = meta.value()?.parse()?;
                collection = Some(value.value());
                Ok(())
            } else if meta.path.is_ident("index") {
                let idx = parse_composite_index(&meta)?;
                indexes.push(idx);
                Ok(())
            } else {
                // Tolerate unknown keys for forward compatibility.
                let _ = meta.value();
                Ok(())
            }
        })?;
    }

    let collection = collection.ok_or_else(|| {
        syn::Error::new(
            proc_macro2::Span::call_site(),
            "missing `#[entity(collection = \"...\")]` on the struct",
        )
    })?;

    Ok((collection, indexes))
}

/// Parse `index(name = "...", fields = ["a", "b"], unique = true)` from
/// inside an `#[entity(...)]` attribute. `unique` defaults to false.
fn parse_composite_index(
    meta: &syn::meta::ParseNestedMeta<'_>,
) -> syn::Result<CompositeIndexAttr> {
    let content;
    syn::parenthesized!(content in meta.input);

    let mut name: Option<String> = None;
    let mut fields: Vec<String> = Vec::new();
    let mut unique = false;

    // Manually walk comma-separated key=value pairs inside the
    // index(...) parens. syn's nested-meta parser handles flat
    // attributes; composite indexes need an array literal for
    // `fields`, which requires a small hand-roll.
    while !content.is_empty() {
        let key: syn::Ident = content.parse()?;
        let _eq: syn::Token![=] = content.parse()?;
        match key.to_string().as_str() {
            "name" => {
                let lit: syn::LitStr = content.parse()?;
                name = Some(lit.value());
            }
            "fields" => {
                let arr: syn::ExprArray = content.parse()?;
                for el in arr.elems {
                    if let syn::Expr::Lit(syn::ExprLit {
                        lit: syn::Lit::Str(s),
                        ..
                    }) = el
                    {
                        fields.push(s.value());
                    } else {
                        return Err(syn::Error::new_spanned(
                            el,
                            "index `fields` entries must be string literals",
                        ));
                    }
                }
            }
            "unique" => {
                let lit: syn::LitBool = content.parse()?;
                unique = lit.value;
            }
            other => {
                return Err(syn::Error::new_spanned(
                    key,
                    format!("unknown index attribute `{}`", other),
                ));
            }
        }
        if content.peek(syn::Token![,]) {
            let _: syn::Token![,] = content.parse()?;
        }
    }

    let name = name.ok_or_else(|| {
        syn::Error::new(
            proc_macro2::Span::call_site(),
            "composite index missing `name = \"...\"`",
        )
    })?;
    if fields.is_empty() {
        return Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            "composite index missing `fields = [...]`",
        ));
    }
    Ok(CompositeIndexAttr {
        name,
        fields,
        unique,
    })
}

fn parse_field(field: &Field) -> syn::Result<FieldInfo> {
    let ident = field
        .ident
        .as_ref()
        .ok_or_else(|| syn::Error::new_spanned(field, "field missing identifier"))?;

    // Detect BaseEntity by type name.
    let is_base_entity = is_base_entity_type(&field.ty);

    // Detect Option<T> wrapping → nullable + unwrap inner.
    let (inner_ty, is_option) = unwrap_option(&field.ty);
    let nullable_from_option = is_option;

    // Walk field-level attributes.
    let mut indexed = false;
    let mut unique = false;
    let mut nullable_override: Option<bool> = None;
    let mut force_json = false;
    let mut skip = false;
    let mut serde_rename: Option<String> = None;
    let mut foreign_key: Option<ForeignKeyAttr> = None;

    for attr in &field.attrs {
        if attr.path().is_ident("entity") {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("indexed") {
                    indexed = true;
                } else if meta.path.is_ident("unique") {
                    unique = true;
                } else if meta.path.is_ident("nullable") {
                    nullable_override = Some(true);
                } else if meta.path.is_ident("json") {
                    force_json = true;
                } else if meta.path.is_ident("skip") {
                    skip = true;
                } else if meta.path.is_ident("foreign_key") {
                    foreign_key = Some(parse_foreign_key(&meta)?);
                } else {
                    // Tolerate unknown keys for forward compatibility.
                    let _ = meta.value();
                }
                Ok(())
            })?;
        } else if attr.path().is_ident("serde") {
            // Pick up #[serde(rename = "...")] so the schema field
            // name matches the serialized JSON key.
            let _ = attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("rename") {
                    if let Ok(value) = meta.value().and_then(|v| v.parse::<syn::LitStr>()) {
                        serde_rename = Some(value.value());
                    }
                }
                Ok(())
            });
        }
    }

    // Infer FieldType from the (unwrapped) Rust type, unless overridden.
    let field_type = if force_json {
        InferredFieldType::Json
    } else {
        infer_field_type(inner_ty)
    };

    let serde_name = serde_rename.unwrap_or_else(|| to_camel_case(&ident.to_string()));

    Ok(FieldInfo {
        serde_name,
        field_type,
        indexed,
        unique,
        nullable: nullable_override.unwrap_or(nullable_from_option),
        skip,
        is_base_entity,
        foreign_key,
    })
}

/// Parse `foreign_key("collection.field"[, on_delete = "..."][, on_update = "..."])`
/// from inside an `#[entity(...)]` field attribute. Defaults: both
/// cascade rules to `Restrict`. The target reference is positional —
/// always the first argument as a `"collection.field"` string literal —
/// so the common case (`#[entity(foreign_key("engrams.id"))]`) reads
/// cleanly without keyword bloat.
fn parse_foreign_key(meta: &syn::meta::ParseNestedMeta<'_>) -> syn::Result<ForeignKeyAttr> {
    let content;
    syn::parenthesized!(content in meta.input);

    // First positional argument — required.
    let target_lit: syn::LitStr = content.parse()?;
    let raw = target_lit.value();
    let (target_collection, target_field) = match raw.split_once('.') {
        Some((c, f)) if !c.is_empty() && !f.is_empty() => (c.to_string(), f.to_string()),
        _ => {
            return Err(syn::Error::new_spanned(
                target_lit,
                "foreign_key target must be \"collection.field\" (e.g. \"engrams.id\")",
            ))
        }
    };

    let mut on_delete = CascadeRuleAttr::default();
    let mut on_update = CascadeRuleAttr::default();

    while content.peek(syn::Token![,]) {
        let _: syn::Token![,] = content.parse()?;
        if content.is_empty() {
            break;
        }
        let key: syn::Ident = content.parse()?;
        let _eq: syn::Token![=] = content.parse()?;
        let value: syn::LitStr = content.parse()?;
        let parsed = parse_cascade_rule(&value.value()).ok_or_else(|| {
            syn::Error::new_spanned(
                &value,
                "cascade rule must be one of \"restrict\" | \"cascade\" | \"set_null\" | \"no_action\"",
            )
        })?;
        match key.to_string().as_str() {
            "on_delete" => on_delete = parsed,
            "on_update" => on_update = parsed,
            other => {
                return Err(syn::Error::new_spanned(
                    key,
                    format!("unknown foreign_key attribute `{}`", other),
                ));
            }
        }
    }

    Ok(ForeignKeyAttr {
        target_collection,
        target_field,
        on_delete,
        on_update,
    })
}

// ─── Type inference helpers ────────────────────────────────────────────

fn infer_field_type(ty: &Type) -> InferredFieldType {
    let name = type_last_segment(ty);
    match name.as_deref() {
        Some("String" | "str") => InferredFieldType::String,
        Some("Uuid") => InferredFieldType::Uuid,
        Some("bool") => InferredFieldType::Boolean,
        Some(
            "u8" | "u16" | "u32" | "u64" | "usize" | "i8" | "i16" | "i32" | "i64" | "isize"
            | "f32" | "f64",
        ) => InferredFieldType::Number,
        Some("Vec" | "HashMap" | "BTreeMap" | "HashSet" | "BTreeSet") => InferredFieldType::Json,
        // Any other named type (enum or struct) → Json. Enums that
        // serialize as plain variant names can be overridden via
        // a future `#[entity(string_enum)]` attribute, but for v1
        // Json is the safe default — JSON-tagged unions round-trip
        // perfectly through serde + a JSON column.
        Some(_) => InferredFieldType::Json,
        None => InferredFieldType::Json,
    }
}

/// Unwraps `Option<T>` once. Returns `(inner_or_self, is_option)`.
fn unwrap_option(ty: &Type) -> (&Type, bool) {
    if let Type::Path(tp) = ty {
        if let Some(seg) = tp.path.segments.last() {
            if seg.ident == "Option" {
                if let PathArguments::AngleBracketed(args) = &seg.arguments {
                    if let Some(GenericArgument::Type(inner)) = args.args.first() {
                        return (inner, true);
                    }
                }
            }
        }
    }
    (ty, false)
}

fn type_last_segment(ty: &Type) -> Option<String> {
    if let Type::Path(tp) = ty {
        return tp.path.segments.last().map(|s| s.ident.to_string());
    }
    if let Type::Reference(r) = ty {
        return type_last_segment(&r.elem);
    }
    None
}

fn is_base_entity_type(ty: &Type) -> bool {
    type_last_segment(ty).as_deref() == Some("BaseEntity")
}

fn to_camel_case(snake: &str) -> String {
    let mut out = String::with_capacity(snake.len());
    let mut capitalize_next = false;
    for c in snake.chars() {
        if c == '_' {
            capitalize_next = true;
        } else if capitalize_next {
            out.push(c.to_ascii_uppercase());
            capitalize_next = false;
        } else {
            out.push(c);
        }
    }
    out
}
