//! `#[derive(Table)]`: declare a table as a Rust struct.
//!
//! ```ignore
//! #[derive(rustable_sql::Table)]
//! #[rustable(table = "bookmarks", index = "user_id, created_at")]
//! struct Bookmark {
//!     #[rustable(primary_key, sql_type = "BIGSERIAL")]
//!     id: i64,
//!     #[rustable(references = "users(id)", on_delete = "CASCADE")]
//!     user_id: i64,
//!     #[rustable(unique)]
//!     url: String,
//!     archived_at: Option<std::time::SystemTime>, // Option = nullable
//!     #[rustable(default = "now()")]
//!     created_at: std::time::SystemTime,
//! }
//! ```
//!
//! Struct attributes: `table`, `schema`, `index` / `unique_index` = `"[name:] a, b [using GIN] [where …]"`,
//! `check = "[name:] expr"` (all repeatable).
//! Field attributes: `primary_key`, `column`, `sql_type`, `default`, `indexed`, `unique`,
//! `references = "table"` / `"table(column)"`, `on_delete`, `on_update`, `skip`.

#![allow(
    clippy::struct_excessive_bools,
    clippy::too_many_lines,
    reason = "one flat attribute struct and one expansion function read best for a small derive"
)]

use proc_macro::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields, LitStr, parse_macro_input};

#[proc_macro_derive(Table, attributes(rustable))]
pub fn derive_table(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match expand(&input) {
        Ok(tokens) => tokens.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

#[derive(Default)]
struct FieldAttrs {
    primary_key: bool,
    column: Option<String>,
    sql_type: Option<String>,
    default: Option<String>,
    indexed: bool,
    unique: bool,
    references: Option<String>,
    on_delete: Option<String>,
    on_update: Option<String>,
    skip: bool,
}

fn string(meta: &syn::meta::ParseNestedMeta<'_>) -> syn::Result<String> {
    Ok(meta.value()?.parse::<LitStr>()?.value())
}

fn expand(input: &DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let ident = &input.ident;
    let Data::Struct(data) = &input.data else {
        return Err(syn::Error::new_spanned(
            ident,
            "#[derive(Table)] only supports structs",
        ));
    };
    let Fields::Named(fields) = &data.fields else {
        return Err(syn::Error::new_spanned(
            ident,
            "#[derive(Table)] needs named fields",
        ));
    };

    let mut table_name = snake_case(&ident.to_string());
    let mut schema: Option<String> = None;
    let mut indexes: Vec<(IndexSpec, bool)> = Vec::new();
    let mut checks: Vec<(Option<String>, String)> = Vec::new();
    for attr in input.attrs.iter().filter(|a| a.path().is_ident("rustable")) {
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("table") {
                table_name = string(&meta)?;
            } else if meta.path.is_ident("schema") {
                schema = Some(string(&meta)?);
            } else if meta.path.is_ident("index") {
                indexes.push((IndexSpec::parse(&string(&meta)?), false));
            } else if meta.path.is_ident("unique_index") {
                indexes.push((IndexSpec::parse(&string(&meta)?), true));
            } else if meta.path.is_ident("check") {
                let spec = string(&meta)?;
                checks.push(match spec.split_once(':') {
                    Some((name, expr)) if !name.contains('(') => {
                        (Some(name.trim().to_string()), expr.trim().to_string())
                    }
                    _ => (None, spec.trim().to_string()),
                });
            } else {
                return Err(meta.error("unknown rustable table attribute"));
            }
            Ok(())
        })?;
    }
    let full_name = match &schema {
        Some(s) if s != "public" => format!("{s}.{table_name}"),
        _ => table_name,
    };

    let mut statements = Vec::new();
    for field in &fields.named {
        let mut attrs = FieldAttrs::default();
        for attr in field.attrs.iter().filter(|a| a.path().is_ident("rustable")) {
            attr.parse_nested_meta(|meta| {
                let p = &meta.path;
                if p.is_ident("primary_key") {
                    attrs.primary_key = true;
                } else if p.is_ident("indexed") {
                    attrs.indexed = true;
                } else if p.is_ident("unique") {
                    attrs.unique = true;
                } else if p.is_ident("skip") {
                    attrs.skip = true;
                } else if p.is_ident("column") {
                    attrs.column = Some(string(&meta)?);
                } else if p.is_ident("sql_type") {
                    attrs.sql_type = Some(string(&meta)?);
                } else if p.is_ident("default") {
                    attrs.default = Some(string(&meta)?);
                } else if p.is_ident("references") {
                    attrs.references = Some(string(&meta)?);
                } else if p.is_ident("on_delete") {
                    attrs.on_delete = Some(string(&meta)?);
                } else if p.is_ident("on_update") {
                    attrs.on_update = Some(string(&meta)?);
                } else {
                    return Err(meta.error("unknown rustable field attribute"));
                }
                Ok(())
            })?;
        }
        if attrs.skip {
            continue;
        }
        let field_ident = field.ident.as_ref().expect("named field");
        let ty = &field.ty;
        let column = attrs
            .column
            .clone()
            .unwrap_or_else(|| field_ident.to_string());

        let data_type = if let Some(t) = &attrs.sql_type {
            quote! { ::std::string::String::from(#t) }
        } else {
            quote! { <#ty as ::rustable_sql::__private::code::SqlType>::sql_type(dialect) }
        };
        let default = opt_string(attrs.default.as_ref());
        // `Option<T>` is nullable. Decided from the syntax, so it also works with `sql_type`.
        let nullable = is_option(ty);
        statements.push(quote! {
            table.columns.push(::rustable_sql::__private::schema::Column {
                name: ::std::string::String::from(#column),
                data_type: #data_type,
                nullable: #nullable,
                default: #default,
                ..::std::default::Default::default()
            });
        });
        if attrs.primary_key {
            statements
                .push(quote! { table.primary_key.push(::std::string::String::from(#column)); });
        }
        if attrs.indexed || attrs.unique {
            let unique = attrs.unique;
            statements.push(quote! {
                table.indexes.push(::rustable_sql::__private::schema::Index {
                    columns: ::std::vec![::std::string::String::from(#column)],
                    unique: #unique,
                    constraint: #unique,
                    ..::std::default::Default::default()
                });
            });
        }
        if let Some(reference) = &attrs.references {
            let (ref_table, ref_column) = match reference.split_once('(') {
                Some((t, c)) => (
                    t.trim().to_string(),
                    c.trim_end_matches(')').trim().to_string(),
                ),
                None => (reference.trim().to_string(), "id".to_string()),
            };
            let on_delete = opt_string(attrs.on_delete.as_ref());
            let on_update = opt_string(attrs.on_update.as_ref());
            statements.push(quote! {
                table.foreign_keys.push(::rustable_sql::__private::schema::ForeignKey {
                    name: ::std::option::Option::None,
                    columns: ::std::vec![::std::string::String::from(#column)],
                    references_table: ::std::string::String::from(#ref_table),
                    references_columns: ::std::vec![::std::string::String::from(#ref_column)],
                    on_delete: #on_delete,
                    on_update: #on_update,
                });
            });
        }
    }
    for (spec, unique) in indexes {
        let columns = &spec.columns;
        let name = opt_string(spec.name.as_ref());
        let using = opt_string(spec.using.as_ref());
        let predicate = opt_string(spec.predicate.as_ref());
        statements.push(quote! {
            table.indexes.push(::rustable_sql::__private::schema::Index {
                name: #name,
                columns: ::std::vec![#(::std::string::String::from(#columns)),*],
                unique: #unique,
                constraint: false,
                using: #using,
                predicate: #predicate,
            });
        });
    }
    for (name, expr) in checks {
        let name = opt_string(name.as_ref());
        statements.push(quote! {
            table.checks.push(::rustable_sql::__private::schema::Check {
                name: #name,
                expr: ::std::string::String::from(#expr),
            });
        });
    }

    Ok(quote! {
        impl ::rustable_sql::__private::code::Table for #ident {
            fn table(dialect: ::rustable_sql::__private::Dialect) -> ::rustable_sql::__private::schema::Table {
                let mut table = ::rustable_sql::__private::schema::Table::new(::std::string::String::from(#full_name));
                #(#statements)*
                ::rustable_sql::__private::code::finish(table, dialect)
            }
        }

        ::rustable_sql::__private::code::inventory::submit! {
            ::rustable_sql::__private::code::TableDef {
                build: <#ident as ::rustable_sql::__private::code::Table>::table,
            }
        }
    })
}

fn is_option(ty: &syn::Type) -> bool {
    match ty {
        syn::Type::Path(p) => p
            .path
            .segments
            .last()
            .is_some_and(|seg| seg.ident == "Option"),
        _ => false,
    }
}

fn opt_string(value: Option<&String>) -> proc_macro2::TokenStream {
    if let Some(v) = value {
        quote! { ::std::option::Option::Some(::std::string::String::from(#v)) }
    } else {
        quote! { ::std::option::Option::None }
    }
}

/// `"[name:] col, col [using METHOD] [where PREDICATE]"`.
struct IndexSpec {
    name: Option<String>,
    columns: Vec<String>,
    using: Option<String>,
    predicate: Option<String>,
}

impl IndexSpec {
    fn parse(spec: &str) -> IndexSpec {
        let (name, rest) = match spec.split_once(':') {
            Some((n, r)) if !n.contains('(') => (Some(n.trim().to_string()), r),
            _ => (None, spec),
        };
        let lower = rest.to_ascii_lowercase();
        let (rest, predicate) = match lower.find(" where ") {
            Some(i) => (&rest[..i], Some(rest[i + 7..].trim().to_string())),
            None => (rest, None),
        };
        let lower = rest.to_ascii_lowercase();
        let (cols, using) = match lower.find(" using ") {
            Some(i) => (&rest[..i], Some(rest[i + 7..].trim().to_ascii_uppercase())),
            None => (rest, None),
        };
        IndexSpec {
            name,
            columns: split_columns(cols),
            using,
            predicate,
        }
    }
}

fn split_columns(s: &str) -> Vec<String> {
    s.split(',')
        .map(|c| c.trim().to_string())
        .filter(|c| !c.is_empty())
        .collect()
}

/// `UserAccount` → `user_account`.
fn snake_case(name: &str) -> String {
    let mut out = String::new();
    for (i, ch) in name.chars().enumerate() {
        if ch.is_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.extend(ch.to_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}
