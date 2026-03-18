use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::parse::{Parse, ParseStream};
use syn::{Expr, ExprAssign, ItemFn, LitStr, Path, Result, Token, Type};

pub fn expand(attr: TokenStream, item: ItemFn) -> Result<TokenStream> {
    let args: SuiteArgs = syn::parse2(attr)?;
    expand_suite(&args, &item)
}

struct SuiteArgs {
    kind: LitStr,
    state_builder: Option<Path>,
    agent_builder: Path,
}

impl Parse for SuiteArgs {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let exprs = input.parse_terminated(Expr::parse, Token![,])?;
        let mut kind = None;
        let mut state_builder = None;
        let mut agent_builder = None;

        for expr in exprs {
            if let Expr::Assign(ExprAssign { left, right, .. }) = expr {
                if matches!(*left, Expr::Path(ref path) if path.path.is_ident("kind")) {
                    if let Expr::Lit(expr_lit) = *right
                        && let syn::Lit::Str(value) = expr_lit.lit
                    {
                        kind = Some(value);
                    }
                    continue;
                }

                if matches!(*left, Expr::Path(ref path) if path.path.is_ident("state"))
                    && let Expr::Path(path) = *right
                {
                    state_builder = Some(path.path);
                    continue;
                }

                if matches!(*left, Expr::Path(ref path) if path.path.is_ident("agent"))
                    && let Expr::Path(path) = *right
                {
                    agent_builder = Some(path.path);
                }
            }
        }

        Ok(Self {
            kind: kind.unwrap_or_else(|| LitStr::new("regression", proc_macro2::Span::call_site())),
            state_builder,
            agent_builder: agent_builder.ok_or_else(|| {
                syn::Error::new(
                    proc_macro2::Span::call_site(),
                    "#[suite] requires agent = build_agent_fn",
                )
            })?,
        })
    }
}

fn expand_suite(args: &SuiteArgs, item: &ItemFn) -> Result<TokenStream> {
    let fn_ident = &item.sig.ident;
    let wrapper_ident = format_ident!("__evals_make_suite_{}", fn_ident);
    let suite_ctor = match args.kind.value().as_str() {
        "capability" => quote!(::borg_evals::Suite::capability),
        _ => quote!(::borg_evals::Suite::regression),
    };
    let _ = &args.agent_builder;
    let (state_ty, state_expr) = match &args.state_builder {
        Some(state_builder) => {
            let state_ty = extract_result_inner_type_from_fn_path(state_builder, item)?;
            (state_ty, quote!(#state_builder().await?))
        }
        None => (syn::parse_quote!(()), quote!(())),
    };

    Ok(quote! {
        #item

        pub async fn #wrapper_ident(suite_id: &str) -> ::anyhow::Result<::borg_evals::Suite<#state_ty>> {
            Ok(
                #suite_ctor(suite_id)
                    .state(#state_expr)
            )
        }
    })
}

fn extract_result_inner_type_from_fn_path(state_builder: &Path, item: &ItemFn) -> Result<Type> {
    if item.sig.ident
        == state_builder
            .segments
            .last()
            .expect("state builder segment")
            .ident
    {
        return extract_result_inner_type(&item.sig.output);
    }

    Err(syn::Error::new_spanned(
        state_builder,
        "#[suite] currently requires state = the annotated async fn path",
    ))
}

fn extract_result_inner_type(output: &syn::ReturnType) -> Result<Type> {
    let syn::ReturnType::Type(_, ty) = output else {
        return Err(syn::Error::new_spanned(
            output,
            "#[suite] requires a function returning Result<State>",
        ));
    };

    let Type::Path(type_path) = &**ty else {
        return Err(syn::Error::new_spanned(
            ty,
            "expected Result<State> return type",
        ));
    };

    let last = type_path
        .path
        .segments
        .last()
        .ok_or_else(|| syn::Error::new_spanned(type_path, "missing return type segment"))?;

    if last.ident != "Result" {
        return Err(syn::Error::new_spanned(
            type_path,
            "expected Result<State> return type",
        ));
    }

    let syn::PathArguments::AngleBracketed(args) = &last.arguments else {
        return Err(syn::Error::new_spanned(last, "expected Result<State>"));
    };

    let Some(syn::GenericArgument::Type(state_ty)) = args.args.first() else {
        return Err(syn::Error::new_spanned(args, "expected Result<State>"));
    };

    Ok(state_ty.clone())
}
