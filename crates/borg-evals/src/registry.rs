use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;

use anyhow::{Context, Result};
use async_trait::async_trait;
use borg_agent::Agent;
use quote::{format_ident, quote};
use syn::{Expr, ExprAssign, File, Item, ItemFn, Meta, parse_quote};
use walkdir::WalkDir;

use crate::{EvalRunReport, RunConfig, Suite, TargetFilter};

pub type BoxSuiteFuture = Pin<Box<dyn Future<Output = Result<Box<dyn RunnableSuite>>> + Send>>;

#[derive(Clone, Copy, Debug)]
pub struct SuiteDescriptor {
    pub id: &'static str,
    pub eval_ids: &'static [&'static str],
    pub build: fn() -> BoxSuiteFuture,
}

impl SuiteDescriptor {
    pub fn new(
        id: &'static str,
        eval_ids: &'static [&'static str],
        build: fn() -> BoxSuiteFuture,
    ) -> Self {
        Self {
            id,
            eval_ids,
            build,
        }
    }
}

#[async_trait]
pub trait RunnableSuite: Send {
    fn id(&self) -> &str;
    fn eval_ids(&self) -> Vec<String>;
    async fn run_box(
        self: Box<Self>,
        config: RunConfig,
        output_dir: &str,
        filter: TargetFilter,
    ) -> Result<EvalRunReport>;
}

#[async_trait]
impl<State, A> RunnableSuite for Suite<State, A>
where
    State: Send + Sync + 'static,
    A: Agent,
{
    fn id(&self) -> &str {
        self.id()
    }

    fn eval_ids(&self) -> Vec<String> {
        self.evals()
            .iter()
            .map(|eval| eval.id().to_string())
            .collect()
    }

    async fn run_box(
        self: Box<Self>,
        config: RunConfig,
        output_dir: &str,
        filter: TargetFilter,
    ) -> Result<EvalRunReport> {
        self.run_with(config)
            .filter(filter)
            .persist_to(output_dir)
            .run()
            .await
            .map_err(|error| anyhow::anyhow!(error.to_string()))
    }
}

pub fn build() -> Result<()> {
    let manifest_dir =
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").context("read CARGO_MANIFEST_DIR")?);
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").context("read OUT_DIR")?);
    let suites = discover_eval_files(&manifest_dir)?;
    let registry = render_registry(&suites)?;
    let output_path = out_dir.join("evals_registry.rs");

    std::fs::write(&output_path, registry)
        .with_context(|| format!("write {}", output_path.display()))?;

    println!(
        "cargo:rerun-if-changed={}",
        manifest_dir.join("evals").display()
    );
    for suite in &suites {
        println!("cargo:rerun-if-changed={}", suite.path.display());
    }

    Ok(())
}

#[macro_export]
/// Includes the generated eval registry for the current crate.
///
/// Call this once from `src/lib.rs` in the crate that owns the `evals/`
/// directory discovered by [`build`].
///
/// ```rust
/// borg_evals::setup!();
/// ```
macro_rules! setup {
    () => {
        #[allow(non_snake_case)]
        pub mod __evals_generated {
            include!(concat!(env!("OUT_DIR"), "/evals_registry.rs"));
        }

        pub use __evals_generated::registry as __evals_registry;
    };
}

struct SuiteSource {
    id: String,
    path: PathBuf,
    agent_builder_fn: String,
    suite_wrapper_fn: String,
    evals: Vec<EvalSource>,
}

struct EvalSource {
    id: String,
    wrapper_fn: String,
}

fn discover_eval_files(manifest_dir: &Path) -> Result<Vec<SuiteSource>> {
    let evals_root = manifest_dir.join("evals");
    if !evals_root.exists() {
        return Ok(Vec::new());
    }

    WalkDir::new(&evals_root)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| entry.into_path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "rs"))
        .map(|path| parse_suite_source(&path))
        .collect()
}

fn parse_suite_source(path: &Path) -> Result<SuiteSource> {
    let source =
        std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let ast: File =
        syn::parse_file(&source).with_context(|| format!("parse {}", path.display()))?;

    let mut suite_fn = None;
    let mut suite_id = None;
    let mut agent_builder_fn = None;
    let mut evals = Vec::new();

    for item in ast.items {
        let Item::Fn(item_fn) = item else {
            continue;
        };

        if has_evals_attr(&item_fn, "suite") {
            let fn_name = item_fn.sig.ident.to_string();
            suite_fn = Some(fn_name.clone());
            suite_id = parse_suite_id(&item_fn)?;
            agent_builder_fn = Some(parse_suite_agent_builder(&item_fn)?);
            continue;
        }

        if has_evals_attr(&item_fn, "eval") {
            let fn_name = item_fn.sig.ident.to_string();
            evals.push(EvalSource {
                id: fn_name.clone(),
                wrapper_fn: format!("__evals_make_eval_{fn_name}"),
            });
        }
    }

    let suite_fn =
        suite_fn.with_context(|| format!("missing #[evals::suite] in {}", path.display()))?;
    let suite_id = suite_id.unwrap_or(default_suite_id(path)?);

    Ok(SuiteSource {
        id: suite_id,
        path: path.to_path_buf(),
        agent_builder_fn: agent_builder_fn.with_context(|| {
            format!(
                "missing agent = ... in #[evals::suite] for {}",
                path.display()
            )
        })?,
        suite_wrapper_fn: format!("__evals_make_suite_{suite_fn}"),
        evals,
    })
}

fn has_evals_attr(item_fn: &ItemFn, name: &str) -> bool {
    item_fn
        .attrs
        .iter()
        .any(|attr| matches_evals_attr(attr, name))
}

fn matches_evals_attr(attr: &syn::Attribute, name: &str) -> bool {
    attr.path()
        .segments
        .last()
        .is_some_and(|segment| segment.ident == name)
}

fn parse_suite_agent_builder(item_fn: &ItemFn) -> Result<String> {
    for attr in &item_fn.attrs {
        if !matches_evals_attr(attr, "suite") {
            continue;
        }

        if let Meta::List(list) = &attr.meta {
            let exprs = list.parse_args_with(
                syn::punctuated::Punctuated::<Expr, syn::Token![,]>::parse_terminated,
            )?;
            for expr in exprs {
                if let Expr::Assign(ExprAssign { left, right, .. }) = expr
                    && matches!(&*left, Expr::Path(path) if path.path.is_ident("agent"))
                    && let Expr::Path(path) = &*right
                {
                    return Ok(quote!(#path).to_string().replace(' ', ""));
                }
            }
        }
    }

    anyhow::bail!("missing agent = build_agent_fn in #[evals::suite]");
}

fn parse_suite_id(item_fn: &ItemFn) -> Result<Option<String>> {
    for attr in &item_fn.attrs {
        if !matches_evals_attr(attr, "suite") {
            continue;
        }

        if let Meta::List(list) = &attr.meta {
            let exprs = list.parse_args_with(
                syn::punctuated::Punctuated::<Expr, syn::Token![,]>::parse_terminated,
            )?;
            for expr in exprs {
                if let Expr::Assign(ExprAssign { left, right, .. }) = expr
                    && matches!(&*left, Expr::Path(path) if path.path.is_ident("suite_id"))
                    && let Expr::Lit(syn::ExprLit {
                        lit: syn::Lit::Str(value),
                        ..
                    }) = &*right
                {
                    return Ok(Some(value.value()));
                }
            }
        }
    }

    Ok(None)
}

fn default_suite_id(path: &Path) -> Result<String> {
    let evals_index = path
        .components()
        .position(|component| component.as_os_str() == "evals")
        .with_context(|| format!("expected {} to be under evals/", path.display()))?;
    let relative = path.components().skip(evals_index + 1).collect::<PathBuf>();
    let without_extension = relative.with_extension("");
    let parts = without_extension
        .components()
        .map(|component| component.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>();
    Ok(parts.join("_"))
}

fn render_registry(suites: &[SuiteSource]) -> Result<String> {
    let modules = suites
        .iter()
        .enumerate()
        .map(|(index, suite)| {
            let module_ident = format_ident!("suite_{index}");
            let include_path = suite.path.display().to_string();
            let suite_wrapper =
                syn::Ident::new(&suite.suite_wrapper_fn, proc_macro2::Span::call_site());
            let agent_builder =
                syn::Ident::new(&suite.agent_builder_fn, proc_macro2::Span::call_site());
            let eval_lines = suite
                .evals
                .iter()
                .map(|eval| {
                    let wrapper = syn::Ident::new(&eval.wrapper_fn, proc_macro2::Span::call_site());
                    quote! {
                        suite = suite.eval(#wrapper().await.map_err(::anyhow::Error::from)?);
                    }
                })
                .collect::<Vec<_>>();
            let eval_ids = suite
                .evals
                .iter()
                .map(|eval| eval.id.as_str())
                .collect::<Vec<_>>();
            let suite_id = suite.id.as_str();

            quote! {
                mod #module_ident {
                    include!(#include_path);

                    pub fn descriptor() -> ::agents::evals::SuiteDescriptor {
                        ::agents::evals::SuiteDescriptor::new(
                            #suite_id,
                            &[#(#eval_ids),*],
                            || Box::pin(async {
                                let mut suite = #suite_wrapper(#suite_id)
                                    .await
                                    .map_err(::anyhow::Error::from)?
                                    .agent(|ctx| async move { #agent_builder(ctx).await });
                                #(#eval_lines)*
                                Ok(Box::new(suite) as Box<dyn ::agents::evals::RunnableSuite>)
                            }),
                        )
                    }
                }
            }
        })
        .collect::<Vec<_>>();

    let descriptors = (0..suites.len())
        .map(|index| {
            let module_ident = format_ident!("suite_{index}");
            quote!(#module_ident::descriptor())
        })
        .collect::<Vec<_>>();

    let file: syn::File = parse_quote! {
        #(#modules)*

        pub fn registry() -> Vec<::agents::evals::SuiteDescriptor> {
            vec![#(#descriptors),*]
        }
    };

    Ok(prettyplease::unparse(&file))
}
