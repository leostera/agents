use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::parse::{Parse, ParseStream};
use syn::{
    Expr, ExprArray, ExprAssign, ExprLit, FnArg, ItemFn, Lit, LitStr, Path, Result, Token, Type,
};

pub fn expand(attr: TokenStream, input: ItemFn) -> Result<TokenStream> {
    let args: EvalArgs = syn::parse2(attr)?;
    expand_eval(&args, &input)
}

struct EvalArgs {
    agent: Path,
    tags: Vec<LitStr>,
}

impl Parse for EvalArgs {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let exprs = input.parse_terminated(Expr::parse, Token![,])?;
        let mut agent = None;
        let mut tags = Vec::new();

        for expr in exprs {
            if let Expr::Assign(ExprAssign { left, right, .. }) = expr {
                if matches!(*left, Expr::Path(ref path) if path.path.is_ident("agent")) {
                    if let Expr::Path(path) = *right {
                        agent = Some(path.path);
                    }
                    continue;
                }
                if matches!(*left, Expr::Path(ref path) if path.path.is_ident("tags"))
                    && let Expr::Array(ExprArray { elems, .. }) = *right
                {
                    for elem in elems {
                        if let Expr::Lit(ExprLit {
                            lit: Lit::Str(value),
                            ..
                        }) = elem
                        {
                            tags.push(value);
                        }
                    }
                }
            }
        }

        Ok(Self {
            agent: agent.ok_or_else(|| {
                syn::Error::new(
                    proc_macro2::Span::call_site(),
                    "#[eval] requires agent = AgentType",
                )
            })?,
            tags,
        })
    }
}

fn extract_state_type(inputs: &syn::punctuated::Punctuated<FnArg, Token![,]>) -> Result<Type> {
    let Some(FnArg::Typed(arg)) = inputs.first() else {
        return Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            "#[eval] requires first arg ctx: EvalContext<State>",
        ));
    };

    let Type::Path(type_path) = &*arg.ty else {
        return Err(syn::Error::new_spanned(
            &arg.ty,
            "expected ctx: EvalContext<State>",
        ));
    };
    let last = type_path
        .path
        .segments
        .last()
        .ok_or_else(|| syn::Error::new_spanned(type_path, "missing EvalContext segment"))?;
    if last.ident != "EvalContext" {
        return Err(syn::Error::new_spanned(
            type_path,
            "expected first arg ctx: EvalContext<State>",
        ));
    }
    let syn::PathArguments::AngleBracketed(args) = &last.arguments else {
        return Err(syn::Error::new_spanned(last, "expected EvalContext<State>"));
    };
    let Some(syn::GenericArgument::Type(state_ty)) = args.args.first() else {
        return Err(syn::Error::new_spanned(args, "expected EvalContext<State>"));
    };
    Ok(state_ty.clone())
}

fn extract_agent_type(output: &syn::ReturnType) -> Result<Path> {
    let syn::ReturnType::Type(_, ty) = output else {
        return Err(syn::Error::new_spanned(
            output,
            "#[eval] requires a function returning Result<Trajectory<Agent>>",
        ));
    };

    match &**ty {
        Type::Path(type_path) => {
            let last =
                type_path.path.segments.last().ok_or_else(|| {
                    syn::Error::new_spanned(type_path, "missing return type segment")
                })?;

            if last.ident == "Result" {
                let syn::PathArguments::AngleBracketed(args) = &last.arguments else {
                    return Err(syn::Error::new_spanned(
                        last,
                        "expected Result<Trajectory<Agent>>",
                    ));
                };
                let Some(syn::GenericArgument::Type(Type::Path(inner))) = args.args.first() else {
                    return Err(syn::Error::new_spanned(
                        args,
                        "expected Result<Trajectory<Agent>>",
                    ));
                };
                extract_agent_from_trajectory(&inner.path)
            } else {
                extract_agent_from_trajectory(&type_path.path)
            }
        }
        other => Err(syn::Error::new_spanned(
            other,
            "expected Result<Trajectory<Agent>> return type",
        )),
    }
}

fn extract_agent_from_trajectory(path: &Path) -> Result<Path> {
    let last = path
        .segments
        .last()
        .ok_or_else(|| syn::Error::new_spanned(path, "missing trajectory path segment"))?;
    if last.ident != "Trajectory" {
        return Err(syn::Error::new_spanned(
            path,
            "expected Result<Trajectory<Agent>> return type",
        ));
    }

    let syn::PathArguments::AngleBracketed(args) = &last.arguments else {
        return Err(syn::Error::new_spanned(last, "expected Trajectory<Agent>"));
    };

    let Some(syn::GenericArgument::Type(Type::Path(agent_ty))) = args.args.first() else {
        return Err(syn::Error::new_spanned(args, "expected Trajectory<Agent>"));
    };

    Ok(agent_ty.path.clone())
}

fn same_path(left: &Path, right: &Path) -> bool {
    quote!(#left).to_string() == quote!(#right).to_string()
}

fn expand_eval(args: &EvalArgs, input: &ItemFn) -> Result<TokenStream> {
    let fn_ident = &input.sig.ident;
    let wrapper_ident = format_ident!("__evals_make_eval_{}", fn_ident);
    let eval_id = fn_ident.to_string();
    let agent_ty = args.agent.clone();
    let state_ty = extract_state_type(&input.sig.inputs)?;
    let trajectory_agent_ty = extract_agent_type(&input.sig.output)?;
    if !same_path(&trajectory_agent_ty, &agent_ty) {
        return Err(syn::Error::new_spanned(
            &input.sig.output,
            "trajectory agent type must match #[eval(agent = ...)]",
        ));
    }
    let tags = args.tags.iter();
    let tags_expr = if args.tags.is_empty() {
        quote!()
    } else {
        quote!(.tags([#(#tags),*]))
    };

    Ok(quote! {
        #input

        pub async fn #wrapper_ident() -> ::anyhow::Result<::borg_evals::Eval<#state_ty, #agent_ty>> {
            Ok(
                ::borg_evals::Eval::new(#eval_id)
                    #tags_expr
                    .run(|ctx, agent| async move {
                        let trajectory = #fn_ident(ctx.clone())
                            .await
                            .map_err(|error| ::borg_evals::EvalError::message(error.to_string()))?;
                        trajectory.runner()(ctx, agent).await
                    })
            )
        }
    })
}

#[cfg(test)]
mod tests {
    use insta::assert_snapshot;
    use quote::quote;
    use syn::parse_quote;

    use super::*;

    #[test]
    fn expands_eval_wrapper_snapshot() {
        let args: EvalArgs = syn::parse2(quote! {
            agent = EchoAgent,
            tags = ["echo", "baseline"]
        })
        .expect("parse eval args");
        let input: ItemFn = parse_quote! {
            async fn echoes_plain_text(
                ctx: EvalContext<EchoHarness>,
            ) -> Result<Trajectory<EchoAgent, EchoHarness>> {
                let _ = ctx;
                todo!()
            }
        };

        let expanded = expand_eval(&args, &input).expect("expand eval");
        let file: syn::File = syn::parse2(expanded).expect("parse expanded file");
        let pretty = prettyplease::unparse(&file);

        assert_snapshot!(pretty, @r#"
        async fn echoes_plain_text(
            ctx: EvalContext<EchoHarness>,
        ) -> Result<Trajectory<EchoAgent, EchoHarness>> {
            let _ = ctx;
            todo!()
        }
        pub async fn __evals_make_eval_echoes_plain_text() -> ::anyhow::Result<
            ::borg_evals::Eval<EchoHarness, EchoAgent>,
        > {
            Ok(
                ::borg_evals::Eval::new("echoes_plain_text")
                    .tags(["echo", "baseline"])
                    .run(|ctx, agent| async move {
                        let trajectory = echoes_plain_text(ctx.clone())
                            .await
                            .map_err(|error| ::borg_evals::EvalError::message(
                                error.to_string(),
                            ))?;
                        trajectory.runner()(ctx, agent).await
                    }),
            )
        }
        "#);
    }

    #[test]
    fn expands_eval_wrapper_without_tags() {
        let args: EvalArgs = syn::parse2(quote! {
            agent = EchoAgent
        })
        .expect("parse eval args");
        let input: ItemFn = parse_quote! {
            async fn smoke_eval(
                ctx: EvalContext<()>,
            ) -> Result<Trajectory<EchoAgent, ()>> {
                let _ = ctx;
                todo!()
            }
        };

        let expanded = expand_eval(&args, &input).expect("expand eval");
        let file: syn::File = syn::parse2(expanded).expect("parse expanded file");
        let pretty = prettyplease::unparse(&file);

        assert!(pretty.contains("::borg_evals::Eval::new(\"smoke_eval\")"));
        assert!(!pretty.contains(".tags(["));
    }

    #[test]
    fn rejects_missing_eval_context_param() {
        let args: EvalArgs = syn::parse2(quote! {
            agent = EchoAgent,
            tags = ["echo"]
        })
        .expect("parse eval args");
        let input: ItemFn = parse_quote! {
            async fn echoes_plain_text() -> Result<Trajectory<EchoAgent, EchoHarness>> {
                todo!()
            }
        };

        let error = expand_eval(&args, &input).expect_err("missing ctx should fail");
        assert_snapshot!(error.to_string(), @"#[eval] requires first arg ctx: EvalContext<State>");
    }

    #[test]
    fn rejects_agent_mismatch() {
        let args: EvalArgs = syn::parse2(quote! {
            agent = EchoAgent,
            tags = ["echo"]
        })
        .expect("parse eval args");
        let input: ItemFn = parse_quote! {
            async fn echoes_plain_text(
                ctx: EvalContext<EchoHarness>,
            ) -> Result<Trajectory<OtherAgent, EchoHarness>> {
                let _ = ctx;
                todo!()
            }
        };

        let error = expand_eval(&args, &input).expect_err("agent mismatch should fail");
        assert_snapshot!(
            error.to_string(),
            @"trajectory agent type must match #[eval(agent = ...)]"
        );
    }
}
