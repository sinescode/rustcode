#![forbid(unsafe_code)]

//! Proc-macro derives for rustcode.
//!
//! Provides `#[derive(Tool)]` for zero-boilerplate tool definitions,
//! automatic JSON Schema generation, and more.
//!
//! # Tool Derive
//!
//! ```ignore
//! #[derive(Tool)]
//! #[tool(description = "Read a file from the filesystem")]
//! struct ReadFile {
//!     /// The path to the file to read
//!     file_path: String,
//!     /// Optional line offset
//!     #[tool(desc = "Line number to start from", default = 1)]
//!     offset: Option<u64>,
//!     /// Optional line limit
//!     #[tool(desc = "Number of lines to read", default = 100)]
//!     limit: Option<u64>,
//! }
//! ```

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput, Expr, Lit};

/// Derive the `Tool` trait for a struct.
///
/// Generates:
/// - `fn id(&self) -> &str` — from the struct name (snake_case)
/// - `fn description(&self) -> &str` — from `#[tool(description = "...")]`
/// - `fn json_schema(&self) -> Option<Value>` — from struct fields
/// - `fn parameters_schema(&self) -> Value` — from struct fields
#[proc_macro_derive(Tool, attributes(tool))]
pub fn derive_tool(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    // Parse tool attributes
    let mut description = String::new();
    for attr in &input.attrs {
        if attr.path().is_ident("tool") {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("description") {
                    let value: Lit = meta.value()?.parse()?;
                    description = match value {
                        Lit::Str(s) => s.value(),
                        _ => return Err(meta.error("expected string literal")),
                    };
                }
                Ok(())
            }).unwrap_or(());
        }
    }

    let id_str = name.to_string().to_lowercase();
    let desc_str = if description.is_empty() {
        format!("{} tool", name)
    } else {
        description
    };

    // Generate schema fields from struct fields
    let fields = match &input.data {
        syn::Data::Struct(data) => &data.fields,
        _ => panic!("Tool derive only supports structs"),
    };

    let mut schema_props = Vec::new();
    let mut required_fields = Vec::new();
    let mut param_fields = Vec::new();

    for field in fields.iter() {
        let field_name = field.ident.as_ref().unwrap().to_string();
        let field_type = &field.ty;

        // Check for #[tool(skip)] or #[tool(default = ...)]
        let mut skip = false;
        let mut default_value: Option<String> = None;
        let mut field_desc = String::new();

        for attr in &field.attrs {
            if attr.path().is_ident("tool") {
                attr.parse_nested_meta(|meta| {
                    if meta.path.is_ident("skip") {
                        skip = true;
                    } else if meta.path.is_ident("default") {
                        let value: Lit = meta.value()?.parse()?;
                        default_value = Some(match value {
                            Lit::Str(s) => s.value(),
                            _ => return Err(meta.error("expected string or number literal")),
                        });
                    } else if meta.path.is_ident("desc") {
                        let value: Lit = meta.value()?.parse()?;
                        field_desc = match value {
                            Lit::Str(s) => s.value(),
                            _ => return Err(meta.error("expected string literal")),
                        };
                    }
                    Ok(())
                }).unwrap_or(());
            }
        }

        if skip {
            continue;
        }

        // Check if field is Option<T> (not required)
        let is_optional = if let syn::Type::Path(type_path) = field_type {
            type_path.path.segments.last()
                .map(|seg| seg.ident == "Option")
                .unwrap_or(false)
        } else {
            false
        };

        if !is_optional && default_value.is_none() {
            required_fields.push(field_name.clone());
        }

        // Get description from doc comments or #[tool(desc)]
        if field_desc.is_empty() {
            for attr in &field.attrs {
                if attr.path().is_ident("doc") {
                    if let Ok(meta) = attr.meta.require_name_value() {
                        if let Expr::Lit(expr_lit) = &meta.value {
                            if let Lit::Str(s) = &expr_lit.lit {
                                let val = s.value().trim().to_string();
                                if !val.is_empty() {
                                    field_desc = val;
                                }
                            }
                        }
                    }
                }
            }
        }

        // Build JSON schema property
        let json_type = if is_optional {
            // Get inner type of Option<T>
            let inner = extract_option_inner(field_type);
            json_type_name(&inner)
        } else {
            json_type_name(field_type)
        };

        schema_props.push(quote! {
            #field_name: #field_type,
        });

        let desc = field_desc;
        param_fields.push((field_name, json_type, desc, default_value));
    }

    // Build the final implementation
    // Collect into Vec first so it can be used multiple times in quote!
    let schema_properties: Vec<_> = param_fields.iter().map(|(name, json_type, desc, default)| {
        let desc_lit = if desc.is_empty() {
            quote! { None }
        } else {
            quote! { Some(#desc) }
        };
        let default_lit = match default {
            Some(val) => quote! { Some(serde_json::json!(#val)) },
            None => quote! { None },
        };
        let json_type_str = json_type.as_str();
        quote! {
            map.insert(
                #name.to_string(),
                serde_json::json!({
                    "type": #json_type_str,
                    "description": #desc_lit,
                    "default": #default_lit,
                })
            );
        }
    }).collect();

    let expanded = quote! {
        impl rustcode_core::tool::Tool for #name {
            fn id(&self) -> &str {
                #id_str
            }

            fn description(&self) -> &str {
                #desc_str
            }

            fn json_schema(&self) -> Option<serde_json::Value> {
                Some(serde_json::json!({
                    "type": "object",
                    "properties": {
                        #(#schema_properties)*
                    },
                    "required": [#(#required_fields),*],
                }))
            }

            fn parameters_schema(&self) -> serde_json::Value {
                let mut map = serde_json::Map::new();
                #(#schema_properties)*
                serde_json::Value::Object(map)
            }
        }
    };

    TokenStream::from(expanded)
}

fn json_type_name(ty: &syn::Type) -> String {
    if let syn::Type::Path(type_path) = ty {
        if let Some(seg) = type_path.path.segments.last() {
            return match seg.ident.to_string().as_str() {
                "String" => "string",
                "bool" | "Bool" => "boolean",
                "i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32" | "u64" | "f32" | "f64"
                    | "usize" => "number",
                "Value" | "serde_json::Value" => "object",
                "Vec" => "array",
                _ => "string",
            }.to_string();
        }
    }
    "string".to_string()
}

fn extract_option_inner(ty: &syn::Type) -> syn::Type {
    if let syn::Type::Path(type_path) = ty {
        if let Some(seg) = type_path.path.segments.last() {
            if seg.ident == "Option" {
                if let syn::PathArguments::AngleBracketed(args) = &seg.arguments {
                    if let Some(syn::GenericArgument::Type(inner)) = args.args.first() {
                        return inner.clone();
                    }
                }
            }
        }
    }
    ty.clone()
}
