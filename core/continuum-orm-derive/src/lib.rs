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
//! - `#[entity(primary_key)]` — declares this field IS the BaseEntity
//!   id (`id: Uuid`). Pulls in `base_entity_fields()` and skips
//!   emitting this field separately so the schema doesn't get a
//!   duplicate `id` column. Use for entities that carry the primary
//!   key as a bare `Uuid` rather than embedding a `BaseEntity`
//!   struct via `#[serde(flatten)]`. Mutually exclusive with the
//!   embedded-BaseEntity form.
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
use proc_macro_crate::{crate_name, FoundCrate};
use quote::quote;
use syn::{
    parse_macro_input, Data, DeriveInput, Field, Fields, GenericArgument, PathArguments, Type,
};

/// Resolve the path prefix for `continuum-core` types in the
/// consumer's Cargo.toml. Per Reviewer 1 #7: emitting absolute
/// `::continuum_core::*` paths breaks when downstream renames the
/// dep (`continuum-core = { package = "continuum-core-alt" }`).
/// `proc-macro-crate` reads the consumer's Cargo.toml at compile
/// time and tells us what name they chose.
///
/// Returns a path prefix like `::continuum_core` or `::renamed_dep`,
/// or `crate` when the consumer IS continuum-core itself (matches
/// the `extern crate self as continuum_core;` self-alias).
fn resolve_continuum_core_path() -> proc_macro2::TokenStream {
    match crate_name("continuum-core") {
        Ok(FoundCrate::Itself) => quote!(crate),
        Ok(FoundCrate::Name(name)) => {
            let ident = syn::Ident::new(&name, proc_macro2::Span::call_site());
            quote!(::#ident)
        }
        // If the consumer's Cargo.toml doesn't list continuum-core
        // at all, the user is using the derive macro standalone —
        // not supported, but we fall back to the conventional name
        // so the diagnostic at compile time is clear ("can't find
        // continuum_core in the crate root").
        Err(_) => quote!(::continuum_core),
    }
}

/// Derive `impl OrmEntity for #name { ... }` from the struct
/// definition + `#[entity(...)]` attributes.
#[proc_macro_derive(Entity, attributes(entity))]
pub fn derive_entity(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as DeriveInput);
    let name = &ast.ident;
    let core = resolve_continuum_core_path();

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
    let mut saw_base: Option<String> = None;

    for field in fields {
        let field_info = match parse_field(field) {
            Ok(info) => info,
            Err(e) => return e.to_compile_error().into(),
        };

        if field_info.skip {
            continue;
        }

        if field_info.is_base_entity || field_info.primary_key {
            if let Some(prior) = &saw_base {
                let here = field
                    .ident
                    .as_ref()
                    .map(|i| i.to_string())
                    .unwrap_or_else(|| "<unnamed>".to_string());
                return syn::Error::new_spanned(
                    field,
                    format!(
                        "duplicate BaseEntity source on `{here}` — already declared by `{prior}`. \
                         Entities use ONE of: `#[serde(flatten)] base: BaseEntity` OR \
                         `#[entity(primary_key)] id: Uuid`. Both are mutually exclusive."
                    ),
                )
                .to_compile_error()
                .into();
            }
            saw_base = field
                .ident
                .as_ref()
                .map(|i| i.to_string())
                .or(Some("<unnamed>".to_string()));
            field_pushes.push(quote! {
                fields.extend(#core::orm::base_entity_fields());
            });
            // `primary_key` is the "no embedded BaseEntity struct"
            // form — the field is `id: Uuid` directly. We pull in
            // base_entity_fields() (which already declares `id` as
            // the PK) and skip emitting this field separately so
            // the schema doesn't get a duplicate `id` column.
            continue;
        }

        let name_str = field_info.serde_name;
        let field_type = field_info.field_type;
        let indexed = field_info.indexed;
        let unique = field_info.unique;
        let nullable = field_info.nullable;

        let field_type_tokens = match field_type {
            InferredFieldType::String => quote!(#core::orm::FieldType::String),
            InferredFieldType::Number => quote!(#core::orm::FieldType::Number),
            InferredFieldType::Boolean => quote!(#core::orm::FieldType::Boolean),
            InferredFieldType::Date => quote!(#core::orm::FieldType::Date),
            InferredFieldType::Json => quote!(#core::orm::FieldType::Json),
            InferredFieldType::Uuid => quote!(#core::orm::FieldType::Uuid),
        };

        let fk_tokens = match &field_info.foreign_key {
            Some(fk) => {
                let target_collection = &fk.target_collection;
                let target_field = &fk.target_field;
                let on_delete = cascade_rule_tokens(&fk.on_delete, &core);
                let on_update = cascade_rule_tokens(&fk.on_update, &core);
                quote! {
                    Some(#core::orm::ForeignKeyRef {
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
            fields.push(#core::orm::SchemaField {
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
            indexes.push(#core::orm::SchemaIndex {
                name: #name.to_string(),
                fields: ::std::vec![#(#fields_lit),*],
                unique: #unique,
            });
        });
    }

    let expanded = quote! {
        impl #core::orm::OrmEntity for #name {
            const COLLECTION: &'static str = #collection;

            fn collection_schema() -> #core::orm::CollectionSchema {
                let mut fields = ::std::vec::Vec::new();
                #(#field_pushes)*
                let mut indexes = ::std::vec::Vec::new();
                #(#index_pushes)*
                #core::orm::CollectionSchema {
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
    /// True if the field carries `#[entity(primary_key)]`. Treated
    /// equivalently to `is_base_entity` at codegen — pulls in
    /// `base_entity_fields()`, skips this field separately. Use
    /// this form when the entity's primary key is a bare `Uuid`
    /// field (`id: Uuid`) rather than an embedded BaseEntity struct.
    primary_key: bool,
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

fn cascade_rule_tokens(
    rule: &CascadeRuleAttr,
    core: &proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    match rule {
        CascadeRuleAttr::Restrict => quote!(#core::orm::CascadeRule::Restrict),
        CascadeRuleAttr::Cascade => quote!(#core::orm::CascadeRule::Cascade),
        CascadeRuleAttr::SetNull => quote!(#core::orm::CascadeRule::SetNull),
        CascadeRuleAttr::NoAction => quote!(#core::orm::CascadeRule::NoAction),
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
                // Hard-error on unknown keys. A typo like `collecton`
                // would silently fall through to the "missing
                // collection" error and confuse the user; better to
                // call it out at the exact attribute span. Same
                // doctrine as the field-level parser above.
                let key = meta
                    .path
                    .get_ident()
                    .map(|i| i.to_string())
                    .unwrap_or_else(|| "<unknown>".to_string());
                Err(meta.error(format!(
                    "unknown struct-level entity attribute `{}`. \
                     Known keys: collection, index.",
                    key
                )))
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
    // Per Reviewer 1 #8: empty index name was previously accepted and
    // would produce `SchemaIndex { name: "".to_string(), … }` —
    // adapters would later choke on CREATE UNIQUE INDEX `` ON …
    // with a cryptic SQL error far from the macro span. Catch it at
    // attribute-parse time.
    if name.is_empty() {
        return Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            "composite index `name` must be non-empty",
        ));
    }
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
    let mut primary_key = false;
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
                } else if meta.path.is_ident("primary_key") {
                    primary_key = true;
                } else if meta.path.is_ident("foreign_key") {
                    foreign_key = Some(parse_foreign_key(&meta)?);
                } else {
                    // Hard-error on unknown keys. Per Joel's no-fallback
                    // doctrine + the substrate's "schema = struct"
                    // commitment: a typo like `indexd` or `foriegn_key`
                    // must surface at compile time, not silently
                    // produce a wrong schema. Listing known keys in
                    // the error message helps the user fix their
                    // attribute fast.
                    let key = meta
                        .path
                        .get_ident()
                        .map(|i| i.to_string())
                        .unwrap_or_else(|| "<unknown>".to_string());
                    return Err(meta.error(format!(
                        "unknown field-level entity attribute `{}`. \
                         Known keys: indexed, unique, nullable, json, skip, \
                         primary_key, foreign_key.",
                        key
                    )));
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

    // Reject `primary_key + foreign_key` on the same field at compile
    // time. `primary_key` means "this is the BaseEntity id" — already
    // UNIQUE + PRIMARY KEY by base_entity_fields(). Adding a FK to
    // the same field would have to override base_entity_fields()'s
    // declaration of `id`, which the macro doesn't do. Silently
    // dropping the FK (the prior behavior) was exactly the kind of
    // schema-vs-attribute drift the macro is supposed to prevent.
    if primary_key && foreign_key.is_some() {
        return Err(syn::Error::new_spanned(
            ident,
            "`#[entity(primary_key)]` and `#[entity(foreign_key(...))]` are mutually exclusive — \
             primary_key implies the BaseEntity id (unique by design). Declare the FK on a \
             different field, or drop primary_key if this is actually a relational pointer.",
        ));
    }

    Ok(FieldInfo {
        serde_name,
        field_type,
        indexed,
        unique,
        nullable: nullable_override.unwrap_or(nullable_from_option),
        skip,
        is_base_entity,
        primary_key,
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

    // First positional argument — required. Format strictly
    // "collection.field" with EXACTLY one dot — multi-dot inputs
    // like "engrams.id.too.many.dots" would silently produce a
    // garbage target_field that fails at runtime with a cryptic
    // SQL error. Catch it at compile time.
    let target_lit: syn::LitStr = content.parse()?;
    let raw = target_lit.value();
    let parts: Vec<&str> = raw.split('.').collect();
    let (target_collection, target_field) = match parts.as_slice() {
        [c, f] if !c.is_empty()
            && !f.is_empty()
            && c.chars().all(|ch| ch.is_alphanumeric() || ch == '_')
            && f.chars().all(|ch| ch.is_alphanumeric() || ch == '_') =>
        {
            (c.to_string(), f.to_string())
        }
        _ => {
            return Err(syn::Error::new_spanned(
                target_lit,
                "foreign_key target must be \"collection.field\" — exactly one dot, both halves \
                 non-empty alphanumeric+underscore. Example: \"engrams.id\".",
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

/// Field-type inference from a Rust type expression. Recursive through
/// transparent wrappers (`Option`, `Box`, `Arc`, `Rc`, `Cow`) — each
/// strips one level and re-infers from the inner type. The outer
/// `unwrap_option` separately tracks `nullable` so callers know the
/// field is optional even after the type system stripped the Option
/// for inference.
///
/// **Recognized types** (post strip-wrappers):
///
/// | Rust type                                  | FieldType  |
/// |--------------------------------------------|------------|
/// | `String` / `str` / `PathBuf` / `Path`      | `String`   |
/// | `Uuid`                                     | `Uuid`     |
/// | `bool`                                     | `Boolean`  |
/// | `u*` / `i*` / `f32` / `f64`                | `Number`   |
/// | `DateTime` / `NaiveDateTime` / `Date`      | `Date`     |
/// | `Vec` / `HashMap` / `BTreeMap` / `HashSet` | `Json`     |
/// | Any other named type (enum/struct)         | `Json`     |
///
/// The "any other → Json" tail is deliberate per
/// [[orm-everything-not-hand-edited-files]]: domain types serdes-
/// round-trip through JSON columns reliably. Callers wanting a
/// different mapping use `#[entity(json)]` (already JSON), or in
/// the future could add `#[entity(string_enum)]` for enums-as-text.
///
/// Tuple types, fixed-size arrays, `Result<T, E>`, and other non-
/// path types fall through to `Json` (not silently — the doctrine
/// is "schema = struct"; if you persist a tuple it becomes JSON).
fn infer_field_type(ty: &Type) -> InferredFieldType {
    // First strip transparent wrappers (Box, Arc, Rc, Cow) so the
    // wrapper-name doesn't drive inference. The reviewer's example:
    // `boxed_name: Box<String>` should be String, not Json.
    if let Some(inner) = strip_transparent_wrapper(ty) {
        return infer_field_type(inner);
    }

    let name = type_last_segment(ty);
    match name.as_deref() {
        Some("String" | "str") => InferredFieldType::String,
        // PathBuf / Path serdes as String; without this branch they
        // fall to the "any other named type → Json" tail.
        Some("PathBuf" | "Path") => InferredFieldType::String,
        Some("Uuid") => InferredFieldType::Uuid,
        Some("bool") => InferredFieldType::Boolean,
        Some(
            "u8" | "u16" | "u32" | "u64" | "u128" | "usize" | "i8" | "i16" | "i32" | "i64"
            | "i128" | "isize" | "f32" | "f64",
        ) => InferredFieldType::Number,
        // Timestamp types. chrono's DateTime / NaiveDateTime serdes
        // as ISO 8601 strings; the substrate's Date FieldType exists
        // for exactly this. Without this branch every timestamp
        // becomes a Json column and the existing `created_at` etc.
        // pattern in BaseEntity loses its semantic.
        Some("DateTime" | "NaiveDateTime" | "Date" | "NaiveDate" | "SystemTime") => {
            InferredFieldType::Date
        }
        Some("Vec" | "HashMap" | "BTreeMap" | "HashSet" | "BTreeSet") => InferredFieldType::Json,
        // Any other named type (enum or struct) → Json. JSON-tagged
        // unions + nested structs round-trip perfectly through serde
        // + a JSON column. Per CLAUDE.md compression: one safe
        // default for the long tail; overrides via attribute.
        Some(_) => InferredFieldType::Json,
        None => InferredFieldType::Json,
    }
}

/// Recursively unwrap `Option<T>` (one or more layers). Returns
/// `(innermost_non_option_ty, is_optional)`. `Option<Option<T>>` is
/// flattened to `T` + nullable; the outer Option's nullability is
/// the load-bearing signal (serde collapses double-Option via
/// `#[serde(default)]` patterns anyway).
fn unwrap_option(ty: &Type) -> (&Type, bool) {
    if let Some(inner) = peel_option_once(ty) {
        let (innermost, _) = unwrap_option(inner);
        return (innermost, true);
    }
    (ty, false)
}

/// Single-step Option peel. None if `ty` isn't `Option<T>`.
fn peel_option_once(ty: &Type) -> Option<&Type> {
    let Type::Path(tp) = ty else { return None };
    let seg = tp.path.segments.last()?;
    if seg.ident != "Option" {
        return None;
    }
    let PathArguments::AngleBracketed(args) = &seg.arguments else { return None };
    let GenericArgument::Type(inner) = args.args.first()? else { return None };
    Some(inner)
}

/// If `ty` is a transparent wrapper (`Box<T>`, `Arc<T>`, `Rc<T>`,
/// `Cow<'_, T>`), return the inner type. These wrappers don't change
/// the persisted shape — `Box<String>` is still a String column —
/// so inference walks through them.
///
/// `Option<T>` is NOT considered a transparent wrapper here because
/// nullability is load-bearing for the schema. `unwrap_option`
/// handles it separately.
fn strip_transparent_wrapper(ty: &Type) -> Option<&Type> {
    let Type::Path(tp) = ty else { return None };
    let seg = tp.path.segments.last()?;
    let name = seg.ident.to_string();
    if !matches!(name.as_str(), "Box" | "Arc" | "Rc" | "Cow") {
        return None;
    }
    let PathArguments::AngleBracketed(args) = &seg.arguments else { return None };
    // Cow has a lifetime arg first; find the first Type arg.
    for arg in &args.args {
        if let GenericArgument::Type(inner) = arg {
            return Some(inner);
        }
    }
    None
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

/// Convert snake_case to camelCase. Matches serde's `rename_all =
/// "camelCase"` behavior so the schema field name and the
/// serialized JSON key agree.
///
/// Per Reviewer 1 #9: previously mishandled `_leading_underscore`
/// (produced `Leading…`), `field__double` (silently coalesced
/// underscores), and `trailing_` (silently dropped). Now:
/// - Leading underscores are preserved (treated as a single
///   suppress-uppercase token), matching serde's `_field` → `_field`
///   convention (no camelization of the leading char).
/// - Doubled internal underscores collapse to a single capital
///   bump, matching serde's behavior — `field__double` → `fieldDouble`.
/// - Trailing underscores are stripped, matching serde — `trailing_`
///   → `trailing`. The trailing underscore is a Rust ident artifact
///   (reserved word workaround like `type_` → `type`), not a wire
///   shape signal.
///
/// The substrate's entity authors generally use clean snake_case
/// without edge characters; these rules exist to match serde's
/// behavior so attribute-name and JSON-key never disagree, not to
/// encourage edge-case identifiers.
fn to_camel_case(snake: &str) -> String {
    let mut out = String::with_capacity(snake.len());
    // Preserve any run of leading underscores literally (serde does).
    let mut chars = snake.chars().peekable();
    while let Some(&'_') = chars.peek() {
        out.push('_');
        chars.next();
    }
    let mut capitalize_next = false;
    for c in chars {
        if c == '_' {
            // Set the flag but don't push — handles internal +
            // doubled underscores uniformly.
            capitalize_next = true;
        } else if capitalize_next {
            out.push(c.to_ascii_uppercase());
            capitalize_next = false;
        } else {
            out.push(c);
        }
    }
    // If the string ENDED with an underscore (capitalize_next still
    // true), drop it — matches serde's "trailing _ is a rust
    // workaround, not a wire signal" semantics.
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What this catches: the `to_camel_case` edge cases Reviewer 1
    /// flagged in #9. These don't appear in production entity field
    /// names today, but the macro is used wherever an entity is
    /// declared — pinning the behavior now means future entity
    /// authors using these patterns get serde-consistent output.
    #[test]
    fn to_camel_case_handles_edge_cases() {
        // Standard snake_case → camelCase.
        assert_eq!(to_camel_case("admitted_at_ms"), "admittedAtMs");
        assert_eq!(to_camel_case("single"), "single");
        // Leading underscore preserved (matches serde rename_all
        // behavior — `_field` stays `_field`, not `Field`).
        assert_eq!(to_camel_case("_internal"), "_internal");
        assert_eq!(to_camel_case("__double_leading"), "__doubleLeading");
        // Doubled internal underscores coalesce to a single bump
        // (matches serde — `field__double` produces `fieldDouble`).
        assert_eq!(to_camel_case("field__double"), "fieldDouble");
        assert_eq!(to_camel_case("a___b"), "aB");
        // Trailing underscore dropped (rust ident workaround like
        // `type_` → wire-side `type`).
        assert_eq!(to_camel_case("trailing_"), "trailing");
        assert_eq!(to_camel_case("type_"), "type");
    }

    /// What this catches: `to_camel_case` doesn't accidentally
    /// uppercase characters that weren't preceded by underscore.
    #[test]
    fn to_camel_case_preserves_existing_case() {
        // No underscore to trigger capitalization; lower stays lower.
        assert_eq!(to_camel_case("alreadycamel"), "alreadycamel");
        assert_eq!(to_camel_case("Mixed_Case"), "MixedCase");
    }
}
