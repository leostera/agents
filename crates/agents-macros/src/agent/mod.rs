use proc_macro2::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Field, Fields, Result};

pub fn expand(input: DeriveInput) -> Result<TokenStream> {
    expand_agent(&input)
}

fn expand_agent(input: &DeriveInput) -> Result<TokenStream> {
    let struct_ident = &input.ident;
    let accessor = find_agent_field(input)?;
    let field_ty = find_agent_field_type(input)?;

    Ok(quote! {
        impl ::agents::agent::Agent for #struct_ident {
            type Input = <#field_ty as ::agents::agent::Agent>::Input;
            type ToolCall = <#field_ty as ::agents::agent::Agent>::ToolCall;
            type ToolResult = <#field_ty as ::agents::agent::Agent>::ToolResult;
            type Output = <#field_ty as ::agents::agent::Agent>::Output;

            async fn send(
                &mut self,
                input: ::agents::agent::AgentInput<Self::Input>,
            ) -> ::agents::agent::AgentResult<()> {
                ::agents::agent::Agent::send(&mut self.#accessor, input).await
            }

            async fn next(
                &mut self,
            ) -> ::agents::agent::AgentResult<
                Option<
                    ::agents::agent::AgentEvent<
                        Self::ToolCall,
                        Self::ToolResult,
                        Self::Output,
                    >,
                >,
            > {
                ::agents::agent::Agent::next(&mut self.#accessor).await
            }

            async fn spawn(
                self,
            ) -> ::agents::agent::AgentResult<(
                ::agents::agent::AgentRunInput<Self::Input>,
                ::agents::agent::AgentRunOutput<Self::ToolCall, Self::ToolResult, Self::Output>,
            )> {
                ::agents::agent::Agent::spawn(self.#accessor).await
            }
        }
    })
}

fn find_agent_field(input: &DeriveInput) -> Result<TokenStream> {
    let Data::Struct(data) = &input.data else {
        return Err(syn::Error::new_spanned(
            input,
            "Agent can only be derived for structs",
        ));
    };

    match &data.fields {
        Fields::Unnamed(fields) if fields.unnamed.len() == 1 => Ok(quote!(0)),
        Fields::Named(fields) if fields.named.len() == 1 => {
            let field = fields.named.first().expect("single named field");
            let ident = field.ident.as_ref().expect("named field ident");
            Ok(quote!(#ident))
        }
        Fields::Unnamed(fields) => {
            let marked: Vec<_> = fields
                .unnamed
                .iter()
                .enumerate()
                .filter(|(_, field)| has_agent_attr(field))
                .collect();
            match marked.as_slice() {
                [(index, _)] => {
                    let index = syn::Index::from(*index);
                    Ok(quote!(#index))
                }
                [] => Err(syn::Error::new_spanned(
                    fields,
                    "Agent derive needs exactly one field or one #[agent] field",
                )),
                _ => Err(syn::Error::new_spanned(
                    fields,
                    "Agent derive found multiple #[agent] fields",
                )),
            }
        }
        Fields::Named(fields) => {
            let marked: Vec<_> = fields
                .named
                .iter()
                .filter(|field| has_agent_attr(field))
                .collect();
            match marked.as_slice() {
                [field] => {
                    let ident = field.ident.as_ref().expect("named field ident");
                    Ok(quote!(#ident))
                }
                [] => Err(syn::Error::new_spanned(
                    fields,
                    "Agent derive needs exactly one field or one #[agent] field",
                )),
                _ => Err(syn::Error::new_spanned(
                    fields,
                    "Agent derive found multiple #[agent] fields",
                )),
            }
        }
        Fields::Unit => Err(syn::Error::new_spanned(
            input,
            "Agent derive requires a field containing the inner agent",
        )),
    }
}

fn find_agent_field_type(input: &DeriveInput) -> Result<syn::Type> {
    let Data::Struct(data) = &input.data else {
        return Err(syn::Error::new_spanned(
            input,
            "Agent can only be derived for structs",
        ));
    };

    match &data.fields {
        Fields::Unnamed(fields) if fields.unnamed.len() == 1 => Ok(fields
            .unnamed
            .first()
            .expect("single unnamed field")
            .ty
            .clone()),
        Fields::Named(fields) if fields.named.len() == 1 => {
            Ok(fields.named.first().expect("single named field").ty.clone())
        }
        Fields::Unnamed(fields) => {
            let marked: Vec<_> = fields
                .unnamed
                .iter()
                .filter(|field| has_agent_attr(field))
                .collect();
            match marked.as_slice() {
                [field] => Ok(field.ty.clone()),
                [] => Err(syn::Error::new_spanned(
                    fields,
                    "Agent derive needs exactly one field or one #[agent] field",
                )),
                _ => Err(syn::Error::new_spanned(
                    fields,
                    "Agent derive found multiple #[agent] fields",
                )),
            }
        }
        Fields::Named(fields) => {
            let marked: Vec<_> = fields
                .named
                .iter()
                .filter(|field| has_agent_attr(field))
                .collect();
            match marked.as_slice() {
                [field] => Ok(field.ty.clone()),
                [] => Err(syn::Error::new_spanned(
                    fields,
                    "Agent derive needs exactly one field or one #[agent] field",
                )),
                _ => Err(syn::Error::new_spanned(
                    fields,
                    "Agent derive found multiple #[agent] fields",
                )),
            }
        }
        Fields::Unit => Err(syn::Error::new_spanned(
            input,
            "Agent derive requires a field containing the inner agent",
        )),
    }
}

fn has_agent_attr(field: &Field) -> bool {
    field.attrs.iter().any(|attr| attr.path().is_ident("agent"))
}

#[cfg(test)]
mod tests {
    use insta::assert_snapshot;
    use syn::parse_quote;

    use super::*;

    #[test]
    fn expands_single_tuple_field_snapshot() {
        let input: DeriveInput = parse_quote! {
            struct EchoAgent(::agents::SessionAgent<EchoReq, EchoTool, String, EchoRes>);
        };

        let expanded = expand_agent(&input).expect("expand eval agent derive");
        let file: syn::File = syn::parse2(expanded).expect("parse expanded file");
        let pretty = prettyplease::unparse(&file);

        assert_snapshot!(pretty, @r#"
        impl ::agents::agent::Agent for EchoAgent {
            type Input = <::agents::SessionAgent<
                EchoReq,
                EchoTool,
                String,
                EchoRes,
            > as ::agents::agent::Agent>::Input;
            type ToolCall = <::agents::SessionAgent<
                EchoReq,
                EchoTool,
                String,
                EchoRes,
            > as ::agents::agent::Agent>::ToolCall;
            type ToolResult = <::agents::SessionAgent<
                EchoReq,
                EchoTool,
                String,
                EchoRes,
            > as ::agents::agent::Agent>::ToolResult;
            type Output = <::agents::SessionAgent<
                EchoReq,
                EchoTool,
                String,
                EchoRes,
            > as ::agents::agent::Agent>::Output;
            async fn send(
                &mut self,
                input: ::agents::agent::AgentInput<Self::Input>,
            ) -> ::agents::agent::AgentResult<()> {
                ::agents::agent::Agent::send(&mut self.0, input).await
            }
            async fn next(
                &mut self,
            ) -> ::agents::agent::AgentResult<
                Option<
                    ::agents::agent::AgentEvent<Self::ToolCall, Self::ToolResult, Self::Output>,
                >,
            > {
                ::agents::agent::Agent::next(&mut self.0).await
            }
            async fn spawn(
                self,
            ) -> ::agents::agent::AgentResult<
                (
                    ::agents::agent::AgentRunInput<Self::Input>,
                    ::agents::agent::AgentRunOutput<
                        Self::ToolCall,
                        Self::ToolResult,
                        Self::Output,
                    >,
                ),
            > {
                ::agents::agent::Agent::spawn(self.0).await
            }
        }
        "#);
    }

    #[test]
    fn expands_marked_named_field_snapshot() {
        let input: DeriveInput = parse_quote! {
            struct EchoAgent {
                #[agent]
                agent: ::agents::SessionAgent<EchoReq, EchoTool, String, EchoRes>,
                other: String,
            }
        };

        let expanded = expand_agent(&input).expect("expand eval agent derive");
        let file: syn::File = syn::parse2(expanded).expect("parse expanded file");
        let pretty = prettyplease::unparse(&file);

        assert_snapshot!(pretty, @r#"
        impl ::agents::agent::Agent for EchoAgent {
            type Input = <::agents::SessionAgent<
                EchoReq,
                EchoTool,
                String,
                EchoRes,
            > as ::agents::agent::Agent>::Input;
            type ToolCall = <::agents::SessionAgent<
                EchoReq,
                EchoTool,
                String,
                EchoRes,
            > as ::agents::agent::Agent>::ToolCall;
            type ToolResult = <::agents::SessionAgent<
                EchoReq,
                EchoTool,
                String,
                EchoRes,
            > as ::agents::agent::Agent>::ToolResult;
            type Output = <::agents::SessionAgent<
                EchoReq,
                EchoTool,
                String,
                EchoRes,
            > as ::agents::agent::Agent>::Output;
            async fn send(
                &mut self,
                input: ::agents::agent::AgentInput<Self::Input>,
            ) -> ::agents::agent::AgentResult<()> {
                ::agents::agent::Agent::send(&mut self.agent, input).await
            }
            async fn next(
                &mut self,
            ) -> ::agents::agent::AgentResult<
                Option<
                    ::agents::agent::AgentEvent<Self::ToolCall, Self::ToolResult, Self::Output>,
                >,
            > {
                ::agents::agent::Agent::next(&mut self.agent).await
            }
            async fn spawn(
                self,
            ) -> ::agents::agent::AgentResult<
                (
                    ::agents::agent::AgentRunInput<Self::Input>,
                    ::agents::agent::AgentRunOutput<
                        Self::ToolCall,
                        Self::ToolResult,
                        Self::Output,
                    >,
                ),
            > {
                ::agents::agent::Agent::spawn(self.agent).await
            }
        }
        "#);
    }

    #[test]
    fn expands_single_named_field_snapshot() {
        let input: DeriveInput = parse_quote! {
            struct EchoAgent {
                agent: ::agents::SessionAgent<EchoReq, EchoTool, String, EchoRes>,
            }
        };

        let expanded = expand_agent(&input).expect("expand eval agent derive");
        let file: syn::File = syn::parse2(expanded).expect("parse expanded file");
        let pretty = prettyplease::unparse(&file);

        assert_snapshot!(pretty, @r#"
        impl ::agents::agent::Agent for EchoAgent {
            type Input = <::agents::SessionAgent<
                EchoReq,
                EchoTool,
                String,
                EchoRes,
            > as ::agents::agent::Agent>::Input;
            type ToolCall = <::agents::SessionAgent<
                EchoReq,
                EchoTool,
                String,
                EchoRes,
            > as ::agents::agent::Agent>::ToolCall;
            type ToolResult = <::agents::SessionAgent<
                EchoReq,
                EchoTool,
                String,
                EchoRes,
            > as ::agents::agent::Agent>::ToolResult;
            type Output = <::agents::SessionAgent<
                EchoReq,
                EchoTool,
                String,
                EchoRes,
            > as ::agents::agent::Agent>::Output;
            async fn send(
                &mut self,
                input: ::agents::agent::AgentInput<Self::Input>,
            ) -> ::agents::agent::AgentResult<()> {
                ::agents::agent::Agent::send(&mut self.agent, input).await
            }
            async fn next(
                &mut self,
            ) -> ::agents::agent::AgentResult<
                Option<
                    ::agents::agent::AgentEvent<Self::ToolCall, Self::ToolResult, Self::Output>,
                >,
            > {
                ::agents::agent::Agent::next(&mut self.agent).await
            }
            async fn spawn(
                self,
            ) -> ::agents::agent::AgentResult<
                (
                    ::agents::agent::AgentRunInput<Self::Input>,
                    ::agents::agent::AgentRunOutput<
                        Self::ToolCall,
                        Self::ToolResult,
                        Self::Output,
                    >,
                ),
            > {
                ::agents::agent::Agent::spawn(self.agent).await
            }
        }
        "#);
    }

    #[test]
    fn rejects_multiple_unmarked_fields() {
        let input: DeriveInput = parse_quote! {
            struct EchoAgent {
                agent: ::agents::SessionAgent<EchoReq, EchoTool, String, EchoRes>,
                other: String,
            }
        };

        let error = expand_agent(&input).expect_err("derive should fail");
        assert!(
            error
                .to_string()
                .contains("needs exactly one field or one #[agent] field")
        );
    }
}
