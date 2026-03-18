use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::parse::{Parse, ParseStream};
use syn::{
    Expr, ExprAssign, ExprLit, FnArg, ItemFn, Lit, LitStr, Result, Token, Type, parse_quote,
};

pub fn expand(attr: TokenStream, input: ItemFn) -> Result<TokenStream> {
    let args: GradeArgs = syn::parse2(attr)?;
    expand_grade(&args, &input)
}

#[derive(Default)]
struct GradeArgs {
    name: Option<LitStr>,
}

impl Parse for GradeArgs {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let exprs = input.parse_terminated(Expr::parse, Token![,])?;
        let mut args = Self::default();

        for expr in exprs {
            if let Expr::Assign(ExprAssign { left, right, .. }) = expr
                && matches!(*left, Expr::Path(ref path) if path.path.is_ident("name"))
                && let Expr::Lit(ExprLit {
                    lit: Lit::Str(value),
                    ..
                }) = *right
            {
                args.name = Some(value);
            }
        }

        Ok(args)
    }
}

fn extract_trial_output_type(
    inputs: &syn::punctuated::Punctuated<FnArg, Token![,]>,
) -> Result<Type> {
    let Some(FnArg::Typed(arg)) = inputs.first() else {
        return Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            "#[grade] requires first arg trial: AgentTrial<Output>",
        ));
    };

    let Type::Path(type_path) = &*arg.ty else {
        return Err(syn::Error::new_spanned(
            &arg.ty,
            "expected trial: AgentTrial<Output>",
        ));
    };
    let last = type_path
        .path
        .segments
        .last()
        .ok_or_else(|| syn::Error::new_spanned(type_path, "missing AgentTrial segment"))?;
    if last.ident != "AgentTrial" {
        return Err(syn::Error::new_spanned(
            type_path,
            "expected first arg trial: AgentTrial<Output>",
        ));
    }
    let syn::PathArguments::AngleBracketed(args) = &last.arguments else {
        return Err(syn::Error::new_spanned(last, "expected AgentTrial<Output>"));
    };
    let Some(syn::GenericArgument::Type(output_ty)) = args.args.first() else {
        return Err(syn::Error::new_spanned(args, "expected AgentTrial<Output>"));
    };
    Ok(output_ty.clone())
}

fn extract_state_type(inputs: &syn::punctuated::Punctuated<FnArg, Token![,]>) -> Result<Type> {
    let Some(FnArg::Typed(arg)) = inputs.iter().nth(1) else {
        return Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            "#[grade] requires second arg ctx: EvalContext<State>",
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
            "expected second arg ctx: EvalContext<State>",
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

fn expand_grade(args: &GradeArgs, input: &ItemFn) -> Result<TokenStream> {
    let wrapper_ident = &input.sig.ident;
    let impl_ident = format_ident!("__borg_grade_impl_{}", wrapper_ident);
    let mut impl_fn = input.clone();
    impl_fn.sig.ident = impl_ident.clone();

    let state_ty = extract_state_type(&input.sig.inputs)?;
    let output_ty = extract_trial_output_type(&input.sig.inputs)?;
    let grade_name = args
        .name
        .as_ref()
        .map(LitStr::value)
        .unwrap_or_else(|| wrapper_ident.to_string());

    let mut wrapper_args = input.sig.inputs.clone();
    for arg in &mut wrapper_args {
        if let FnArg::Typed(pat) = arg {
            pat.attrs.clear();
        }
    }

    let wrapper_fn: ItemFn = parse_quote! {
        pub fn #wrapper_ident() -> ::borg_evals::Grader<#state_ty, #output_ty> {
            ::borg_evals::predicate(#grade_name, |trial, ctx| async move {
                #impl_ident(trial, ctx).await
            })
        }
    };

    Ok(quote! {
        #impl_fn
        #wrapper_fn
    })
}

#[cfg(test)]
mod tests {
    use insta::assert_snapshot;
    use quote::quote;
    use syn::parse_quote;

    use super::*;

    #[test]
    fn expands_grade_wrapper_snapshot() {
        let args: GradeArgs = syn::parse2(quote! {
            name = "echoes-empty"
        })
        .expect("parse grade args");
        let input: ItemFn = parse_quote! {
            async fn echoes_empty(
                trial: AgentTrial<EchoRes>,
                _ctx: EvalContext<EchoHarness>,
            ) -> EvalResult<GradeResult> {
                let reply = trial.final_reply.unwrap();
                Ok(GradeResult {
                    score: if reply.text.is_empty() { 1.0 } else { 0.0 },
                    summary: "echo agent should preserve empty string".to_string(),
                    evidence: serde_json::json!({ "reply": reply.text }),
                })
            }
        };

        let expanded = expand_grade(&args, &input).expect("expand grade");
        let file: syn::File = syn::parse2(expanded).expect("parse expanded file");
        let pretty = prettyplease::unparse(&file);

        assert_snapshot!(pretty, @r#"
        async fn __borg_grade_impl_echoes_empty(
            trial: AgentTrial<EchoRes>,
            _ctx: EvalContext<EchoHarness>,
        ) -> EvalResult<GradeResult> {
            let reply = trial.final_reply.unwrap();
            Ok(GradeResult {
                score: if reply.text.is_empty() { 1.0 } else { 0.0 },
                summary: "echo agent should preserve empty string".to_string(),
                evidence: serde_json::json!({ "reply" : reply.text }),
            })
        }
        pub fn echoes_empty() -> ::borg_evals::Grader<EchoHarness, EchoRes> {
            ::borg_evals::predicate(
                "echoes-empty",
                |trial, ctx| async move { __borg_grade_impl_echoes_empty(trial, ctx).await },
            )
        }
        "#);
    }
}
