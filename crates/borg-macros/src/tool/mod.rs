use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::parse::{Parse, ParseStream};
use syn::{
    Data, DataEnum, DeriveInput, Expr, ExprAssign, ExprLit, Field, Fields, Lit, LitStr, Result,
    Token, Type,
};

pub fn expand(input: DeriveInput) -> Result<TokenStream> {
    expand_agent_tool(&input)
}

#[derive(Default)]
struct ToolAttrs {
    name: Option<LitStr>,
    description: Option<LitStr>,
}

impl Parse for ToolAttrs {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let exprs = input.parse_terminated(Expr::parse, Token![,])?;
        let mut attrs = Self::default();

        for expr in exprs {
            if let Expr::Assign(ExprAssign { left, right, .. }) = expr {
                let Expr::Path(left) = *left else {
                    continue;
                };
                let Expr::Lit(ExprLit {
                    lit: Lit::Str(value),
                    ..
                }) = *right
                else {
                    continue;
                };
                if left.path.is_ident("name") {
                    attrs.name = Some(value);
                } else if left.path.is_ident("description") {
                    attrs.description = Some(value);
                }
            }
        }

        Ok(attrs)
    }
}

struct VariantSpec {
    helper: Option<TokenStream>,
    definition: TokenStream,
    decode_arm: TokenStream,
}

pub fn expand_agent_tool(input: &DeriveInput) -> Result<TokenStream> {
    let enum_ident = &input.ident;
    let Data::Enum(DataEnum { variants, .. }) = &input.data else {
        return Err(syn::Error::new_spanned(
            input,
            "Tool can only be derived for enums",
        ));
    };

    let mut helpers = Vec::new();
    let mut definitions = Vec::new();
    let mut decode_arms = Vec::new();

    for variant in variants {
        let spec = expand_variant(enum_ident, variant)?;
        if let Some(helper) = spec.helper {
            helpers.push(helper);
        }
        definitions.push(spec.definition);
        decode_arms.push(spec.decode_arm);
    }

    Ok(quote! {
        #(#helpers)*

        impl ::agents::llm::tools::TypedTool for #enum_ident {
            fn tool_definitions() -> Vec<::agents::llm::tools::ToolDefinition> {
                vec![#(#definitions),*]
            }

            fn decode_tool_call(
                name: &str,
                arguments: ::serde_json::Value,
            ) -> ::agents::llm::error::LlmResult<Self> {
                match name {
                    #(#decode_arms),*,
                    other => Err(::agents::llm::error::Error::InvalidResponse {
                        reason: format!("unexpected tool name: {other}"),
                    }),
                }
            }
        }
    })
}

fn expand_variant(enum_ident: &syn::Ident, variant: &syn::Variant) -> Result<VariantSpec> {
    let attrs = parse_variant_attrs(variant)?;
    let ident = variant.ident.clone();
    let tool_name = attrs
        .name
        .as_ref()
        .map(LitStr::value)
        .unwrap_or_else(|| to_snake_case(&ident.to_string()));
    let description = attrs.description.clone();

    match &variant.fields {
        Fields::Unit => Ok(expand_unit_variant(&ident, &tool_name, description)),
        Fields::Unnamed(fields) if fields.unnamed.len() == 1 => {
            let inner_ty = fields
                .unnamed
                .first()
                .map(|field| field.ty.clone())
                .expect("single unnamed field");
            Ok(expand_single_unnamed_variant(
                &ident,
                &tool_name,
                description,
                inner_ty,
            ))
        }
        Fields::Named(fields) => expand_named_variant(
            enum_ident,
            &ident,
            &tool_name,
            description,
            fields.named.iter().collect(),
        ),
        Fields::Unnamed(_) => Err(syn::Error::new_spanned(
            variant,
            "Tool derive only supports unit variants, single-field tuple variants, or named-field variants",
        )),
    }
}

fn expand_unit_variant(
    ident: &syn::Ident,
    tool_name: &str,
    description: Option<LitStr>,
) -> VariantSpec {
    let description = match description {
        Some(description) => quote!(Some(#description)),
        None => quote!(None),
    };

    VariantSpec {
        helper: None,
        definition: quote! {
            ::agents::llm::tools::ToolDefinition::function(
                #tool_name,
                #description,
                ::serde_json::json!({
                    "type": "object",
                    "properties": {},
                    "additionalProperties": false
                }),
            )
        },
        decode_arm: quote! {
            #tool_name => {
                let _: ::std::collections::HashMap<String, ::serde_json::Value> =
                    ::serde_json::from_value(arguments)
                        .map_err(|error| ::agents::llm::error::Error::parse("tool arguments", error))?;
                Ok(Self::#ident)
            }
        },
    }
}

fn expand_single_unnamed_variant(
    ident: &syn::Ident,
    tool_name: &str,
    description: Option<LitStr>,
    inner_ty: Type,
) -> VariantSpec {
    let description = match description {
        Some(description) => quote!(Some(#description)),
        None => quote!(None),
    };

    VariantSpec {
        helper: None,
        definition: quote! {
            ::agents::llm::tools::ToolDefinition::function(
                #tool_name,
                #description,
                ::serde_json::to_value(::schemars::schema_for!(#inner_ty))
                    .expect("serialize tool schema"),
            )
        },
        decode_arm: quote! {
            #tool_name => Ok(Self::#ident(
                ::serde_json::from_value::<#inner_ty>(arguments)
                    .map_err(|error| ::agents::llm::error::Error::parse("tool arguments", error))?
            ))
        },
    }
}

fn expand_named_variant(
    enum_ident: &syn::Ident,
    ident: &syn::Ident,
    tool_name: &str,
    description: Option<LitStr>,
    fields: Vec<&Field>,
) -> Result<VariantSpec> {
    let helper_ident = format_ident!("__BorgAgentToolArgs{}{}", enum_ident, ident,);
    let helper_fields = fields.iter().map(|field| {
        let field_ident = field.ident.clone().expect("named field");
        let field_ty = &field.ty;
        quote!(pub #field_ident: #field_ty)
    });
    let construct_fields = fields.iter().map(|field| {
        let field_ident = field.ident.clone().expect("named field");
        quote!(#field_ident: args.#field_ident)
    });
    let description = match description {
        Some(description) => quote!(Some(#description)),
        None => quote!(None),
    };

    Ok(VariantSpec {
        helper: Some(quote! {
            #[derive(::serde::Deserialize, ::schemars::JsonSchema)]
            struct #helper_ident {
                #(#helper_fields),*
            }
        }),
        definition: quote! {
            ::agents::llm::tools::ToolDefinition::function(
                #tool_name,
                #description,
                ::serde_json::to_value(::schemars::schema_for!(#helper_ident))
                    .expect("serialize tool schema"),
            )
        },
        decode_arm: quote! {
            #tool_name => {
                let args = ::serde_json::from_value::<#helper_ident>(arguments)
                    .map_err(|error| ::agents::llm::error::Error::parse("tool arguments", error))?;
                Ok(Self::#ident { #(#construct_fields),* })
            }
        },
    })
}

fn parse_variant_attrs(variant: &syn::Variant) -> Result<ToolAttrs> {
    let mut merged = ToolAttrs::default();

    for attr in &variant.attrs {
        if !attr.path().is_ident("agent_tool") {
            continue;
        }
        let attrs = attr.parse_args::<ToolAttrs>()?;
        if attrs.name.is_some() {
            merged.name = attrs.name;
        }
        if attrs.description.is_some() {
            merged.description = attrs.description;
        }
    }

    Ok(merged)
}

fn to_snake_case(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for (index, ch) in value.chars().enumerate() {
        if ch.is_uppercase() {
            if index > 0 {
                out.push('_');
            }
            for lower in ch.to_lowercase() {
                out.push(lower);
            }
        } else {
            out.push(ch);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use insta::assert_snapshot;
    use syn::parse_quote;

    use super::*;

    #[test]
    fn expands_agent_tool_enum_snapshot() {
        let input: DeriveInput = parse_quote! {
            enum TestTools {
                #[agent_tool(description = "Ping tool")]
                Ping { value: String },
                #[agent_tool(name = "echo_text", description = "Echo tool")]
                Echo(EchoArgs),
                ListEvents,
            }
        };

        let expanded = expand_agent_tool(&input).expect("expand tool derive");
        let file: syn::File = syn::parse2(expanded).expect("parse expanded file");
        let pretty = prettyplease::unparse(&file);

        assert_snapshot!(pretty, @r#"
        #[derive(::serde::Deserialize, ::schemars::JsonSchema)]
        struct __BorgAgentToolArgsTestToolsPing {
            pub value: String,
        }
        impl ::agents::llm::tools::TypedTool for TestTools {
            fn tool_definitions() -> Vec<::agents::llm::tools::ToolDefinition> {
                vec![
                    ::agents::llm::tools::ToolDefinition::function("ping", Some("Ping tool"),
                    ::serde_json::to_value(::schemars::schema_for!(__BorgAgentToolArgsTestToolsPing))
                    .expect("serialize tool schema"),),
                    ::agents::llm::tools::ToolDefinition::function("echo_text",
                    Some("Echo tool"), ::serde_json::to_value(::schemars::schema_for!(EchoArgs))
                    .expect("serialize tool schema"),),
                    ::agents::llm::tools::ToolDefinition::function("list_events", None,
                    ::serde_json::json!({ "type" : "object", "properties" : {},
                    "additionalProperties" : false }),)
                ]
            }
            fn decode_tool_call(
                name: &str,
                arguments: ::serde_json::Value,
            ) -> ::agents::llm::error::LlmResult<Self> {
                match name {
                    "ping" => {
                        let args = ::serde_json::from_value::<
                            __BorgAgentToolArgsTestToolsPing,
                        >(arguments)
                            .map_err(|error| ::agents::llm::error::Error::parse(
                                "tool arguments",
                                error,
                            ))?;
                        Ok(Self::Ping { value: args.value })
                    }
                    "echo_text" => {
                        Ok(
                            Self::Echo(
                                ::serde_json::from_value::<EchoArgs>(arguments)
                                    .map_err(|error| ::agents::llm::error::Error::parse(
                                        "tool arguments",
                                        error,
                                    ))?,
                            ),
                        )
                    }
                    "list_events" => {
                        let _: ::std::collections::HashMap<String, ::serde_json::Value> = ::serde_json::from_value(
                                arguments,
                            )
                            .map_err(|error| ::agents::llm::error::Error::parse(
                                "tool arguments",
                                error,
                            ))?;
                        Ok(Self::ListEvents)
                    }
                    other => {
                        Err(::agents::llm::error::Error::InvalidResponse {
                            reason: format!("unexpected tool name: {other}"),
                        })
                    }
                }
            }
        }
        "#);
    }

    #[test]
    fn rejects_non_enum_inputs() {
        let input: DeriveInput = parse_quote! {
            struct EchoArgs {
                value: String,
            }
        };

        let error = expand_agent_tool(&input).expect_err("struct derive should fail");
        assert!(error.to_string().contains("only be derived for enums"));
    }

    #[test]
    fn snake_cases_variant_names() {
        assert_eq!(to_snake_case("ToolAdd"), "tool_add");
        assert_eq!(to_snake_case("ListEvents"), "list_events");
    }
}
