use std::sync::{Arc, Mutex};

use anyhow::{Result, anyhow};
use borg_core::borgdir::BorgDir;
use deno_config::deno_json::NodeModulesDirMode;
use deno_core::futures::FutureExt;
use deno_core::{
    ModuleLoadResponse, ModuleLoader, ModuleSource, ModuleSourceCode, ModuleSpecifier, ModuleType,
    RequestedModuleType, ResolutionKind,
};
use deno_npm_cache::{
    DownloadError, NpmCacheHttpClient, NpmCacheHttpClientBytesResponse, NpmCacheHttpClientResponse,
    NpmCacheSetting,
};
use deno_npm_installer::graph::NpmCachingStrategy;
use deno_npm_installer::lifecycle_scripts::NullLifecycleScriptsExecutor;
use deno_npm_installer::{
    LifecycleScriptsConfig, LogReporter, NpmInstallerFactory, NpmInstallerFactoryOptions,
};
use deno_resolver::factory::{
    ConfigDiscoveryOption, ResolverFactory, ResolverFactoryOptions, WorkspaceFactory,
    WorkspaceFactoryOptions,
};
use deno_semver::npm::NpmPackageReqReference;
use deno_semver::package::PackageReq;
use node_resolver::{NodeResolutionKind, ResolutionMode};
use reqwest::header::{ETAG, IF_NONE_MATCH};
use sys_traits::impls::RealSys;
use url::Url;

const CODEMODE_DIR: &str = "codemode";
const DENO_CACHE_DIR: &str = "deno_cache";
const RESOLUTION_REFERRER: &str = "file:///__borg_codemode_entry__.mjs";

pub(crate) struct CodeModeModuleLoader {
    subsystem: Arc<ResolverSubsystem>,
}

impl CodeModeModuleLoader {
    pub fn new() -> Self {
        Self {
            subsystem: Arc::new(ResolverSubsystem::new()),
        }
    }
}

impl Default for CodeModeModuleLoader {
    fn default() -> Self {
        Self::new()
    }
}

impl ModuleLoader for CodeModeModuleLoader {
    fn resolve(
        &self,
        specifier: &str,
        referrer: &str,
        _kind: ResolutionKind,
    ) -> Result<ModuleSpecifier, anyhow::Error> {
        self.subsystem.resolve(specifier, referrer)
    }

    fn load(
        &self,
        module_specifier: &ModuleSpecifier,
        _maybe_referrer: Option<&ModuleSpecifier>,
        _is_dynamic: bool,
        requested_module_type: RequestedModuleType,
    ) -> ModuleLoadResponse {
        let specifier = module_specifier.clone();
        let fut = async move {
            let bytes = match specifier.scheme() {
                "file" => load_file_module(&specifier)?,
                "http" | "https" => load_http_module(&specifier)?,
                other => {
                    return Err(anyhow!(
                        "unsupported module scheme `{other}` for {specifier}"
                    ));
                }
            };

            let module_type = module_type_for_specifier(&specifier, &requested_module_type);
            if module_type == ModuleType::Json
                && requested_module_type != RequestedModuleType::Json
            {
                return Err(anyhow!(
                    "attempted to load JSON module without `with {{ type: \"json\" }}` import attribute"
                ));
            }

            Ok(ModuleSource::new(
                module_type,
                ModuleSourceCode::Bytes(bytes.into_boxed_slice().into()),
                &specifier,
                None,
            ))
        }
        .boxed_local();
        ModuleLoadResponse::Async(fut)
    }
}

struct ResolverSubsystem {
    state: Mutex<Option<ResolverRuntime>>,
}

impl ResolverSubsystem {
    fn new() -> Self {
        Self {
            state: Mutex::new(None),
        }
    }

    fn resolve(&self, specifier: &str, referrer: &str) -> Result<ModuleSpecifier> {
        let mut lock = self
            .state
            .lock()
            .map_err(|_| anyhow!("resolver subsystem lock poisoned"))?;
        if lock.is_none() {
            *lock = Some(ResolverRuntime::init()?);
        }
        let runtime = lock
            .as_mut()
            .ok_or_else(|| anyhow!("resolver runtime not initialized"))?;
        runtime.resolve(specifier, referrer)
    }
}

struct ResolverRuntime {
    resolver_factory: Arc<ResolverFactory<RealSys>>,
    npm_installer_factory: Arc<NpmInstallerFactory<ReqwestNpmHttpClient, LogReporter, RealSys>>,
    async_rt: &'static tokio::runtime::Runtime,
}

impl ResolverRuntime {
    fn init() -> Result<Self> {
        let borg_root = BorgDir::new().root().to_path_buf();
        let codemode_root = borg_root.join(CODEMODE_DIR);
        let node_modules_path = codemode_root.join("node_modules");
        let deno_cache_root = codemode_root.join(DENO_CACHE_DIR);
        std::fs::create_dir_all(&node_modules_path)?;
        std::fs::create_dir_all(&deno_cache_root)?;

        let async_rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|err| anyhow!("failed to initialize resolver async runtime: {err}"))?;
        let async_rt = Box::leak(Box::new(async_rt));

        let workspace_factory = Arc::new(WorkspaceFactory::new(
            RealSys,
            codemode_root.clone(),
            WorkspaceFactoryOptions {
                config_discovery: ConfigDiscoveryOption::Disabled,
                maybe_custom_deno_dir_root: Some(deno_cache_root),
                node_modules_dir: Some(NodeModulesDirMode::Auto),
                root_node_modules_dir_override: Some(node_modules_path),
                ..Default::default()
            },
        ));

        let resolver_factory = Arc::new(ResolverFactory::new(
            workspace_factory,
            ResolverFactoryOptions::default(),
        ));

        let npm_installer_factory = Arc::new(NpmInstallerFactory::new(
            resolver_factory.clone(),
            Arc::new(ReqwestNpmHttpClient::default()),
            Arc::new(NullLifecycleScriptsExecutor),
            LogReporter,
            None,
            NpmInstallerFactoryOptions {
                cache_setting: NpmCacheSetting::Use,
                caching_strategy: NpmCachingStrategy::Lazy,
                clean_on_install: false,
                lifecycle_scripts_config: LifecycleScriptsConfig {
                    initial_cwd: codemode_root.clone(),
                    root_dir: codemode_root,
                    ..LifecycleScriptsConfig::default()
                },
                resolve_npm_resolution_snapshot: Box::new(|| Ok(None)),
            },
        ));

        Ok(Self {
            resolver_factory,
            npm_installer_factory,
            async_rt,
        })
    }

    fn resolve(&mut self, specifier: &str, referrer: &str) -> Result<ModuleSpecifier> {
        let referrer_url = parse_referrer_url(referrer)?;

        if specifier.starts_with("npm:") {
            return self.resolve_npm_like_specifier(specifier, &referrer_url);
        }

        if specifier.starts_with("jsr:") {
            // JSR packages are mirrored to npm under the @jsr scope.
            let npm_specifier = map_jsr_to_npm_specifier(specifier)?;
            return self.resolve_npm_like_specifier(&npm_specifier, &referrer_url);
        }

        let resolution = self.async_rt.block_on(async {
            let raw_resolver = self.resolver_factory.raw_deno_resolver().await?;
            raw_resolver
                .resolve(
                    &specifier,
                    &referrer_url,
                    ResolutionMode::Import,
                    NodeResolutionKind::Execution,
                )
                .map(|resolved| resolved.url)
                .map_err(|err| anyhow!("failed to resolve `{specifier}`: {err}"))
        })?;

        Ok(ModuleSpecifier::parse(resolution.as_str())?)
    }

    fn resolve_npm_like_specifier(
        &mut self,
        specifier: &str,
        referrer: &Url,
    ) -> Result<ModuleSpecifier> {
        let req_ref = NpmPackageReqReference::from_str(specifier)
            .map_err(|err| anyhow!("invalid npm specifier `{specifier}`: {err}"))?;
        self.ensure_npm_req_cached(req_ref.req().clone(), specifier)?;

        let resolved = self
            .resolver_factory
            .npm_req_resolver()?
            .resolve_req_reference(
                &req_ref,
                referrer,
                ResolutionMode::Import,
                NodeResolutionKind::Execution,
            )
            .map_err(|err| anyhow!("failed to resolve npm specifier `{specifier}`: {err}"))?;
        let resolved_url = resolved.into_url().map_err(|err| {
            anyhow!("failed to convert npm resolution `{specifier}` to URL: {err}")
        })?;
        Ok(ModuleSpecifier::parse(resolved_url.as_str())?)
    }

    fn ensure_npm_req_cached(&mut self, req: PackageReq, specifier: &str) -> Result<()> {
        self.async_rt.block_on(async {
            let installer = self.npm_installer_factory.npm_installer().await?;
            installer
                .add_and_cache_package_reqs(&[req])
                .await
                .map_err(|err| anyhow!("failed to install npm package for `{specifier}`: {err}"))
        })?;

        Ok(())
    }
}

#[derive(Debug, Clone)]
struct ReqwestNpmHttpClient {
    client: reqwest::Client,
}

impl Default for ReqwestNpmHttpClient {
    fn default() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait::async_trait(?Send)]
impl NpmCacheHttpClient for ReqwestNpmHttpClient {
    async fn download_with_retries_on_any_tokio_runtime(
        &self,
        url: Url,
        maybe_auth: Option<String>,
        maybe_etag: Option<String>,
    ) -> std::result::Result<NpmCacheHttpClientResponse, DownloadError> {
        let mut request = self.client.get(url.clone());
        if let Some(auth) = maybe_auth {
            request = request.header("authorization", auth);
        }
        if let Some(etag) = maybe_etag {
            request = request.header(IF_NONE_MATCH, etag);
        }

        let response = request.send().await.map_err(|err| DownloadError {
            status_code: None,
            error: deno_error::JsErrorBox::generic(err.to_string()),
        })?;

        let status = response.status();
        if status.as_u16() == 304 {
            return Ok(NpmCacheHttpClientResponse::NotModified);
        }
        if status.as_u16() == 404 {
            return Ok(NpmCacheHttpClientResponse::NotFound);
        }
        if !status.is_success() {
            return Err(DownloadError {
                status_code: Some(status.as_u16()),
                error: deno_error::JsErrorBox::generic(format!(
                    "HTTP {} while downloading {}",
                    status, url
                )),
            });
        }

        let etag = response
            .headers()
            .get(ETAG)
            .and_then(|value| value.to_str().ok())
            .map(ToString::to_string);
        let bytes = response.bytes().await.map_err(|err| DownloadError {
            status_code: Some(status.as_u16()),
            error: deno_error::JsErrorBox::generic(err.to_string()),
        })?;
        Ok(NpmCacheHttpClientResponse::Bytes(
            NpmCacheHttpClientBytesResponse {
                bytes: bytes.to_vec(),
                etag,
            },
        ))
    }
}

fn parse_referrer_url(referrer: &str) -> Result<Url> {
    if referrer.trim().is_empty() {
        return Ok(Url::parse(RESOLUTION_REFERRER)?);
    }
    Url::parse(referrer)
        .or_else(|_| Url::parse(RESOLUTION_REFERRER))
        .map_err(Into::into)
}

fn map_jsr_to_npm_specifier(specifier: &str) -> Result<String> {
    let without_prefix = specifier
        .strip_prefix("jsr:")
        .ok_or_else(|| anyhow!("expected jsr: prefix"))?;
    if !without_prefix.starts_with('@') {
        return Err(anyhow!("invalid jsr package specifier `{specifier}`"));
    }
    let slash_idx = without_prefix
        .find('/')
        .ok_or_else(|| anyhow!("invalid jsr package specifier `{specifier}`"))?;
    let scope = &without_prefix[1..slash_idx];
    let rest = &without_prefix[slash_idx + 1..];
    let (name_and_version, maybe_subpath) = split_name_and_subpath(rest);
    let (name, maybe_version) = split_name_and_version(name_and_version)?;
    let base = format!("@jsr/{scope}__{name}");
    let with_version = if let Some(version) = maybe_version {
        format!("{base}@{version}")
    } else {
        base
    };
    let with_subpath = if let Some(subpath) = maybe_subpath {
        format!("{with_version}/{subpath}")
    } else {
        with_version
    };
    Ok(format!("npm:{with_subpath}"))
}

fn split_name_and_subpath(spec: &str) -> (&str, Option<&str>) {
    if let Some(second_slash) = spec.find('/') {
        (&spec[..second_slash], Some(&spec[second_slash + 1..]))
    } else {
        (spec, None)
    }
}

fn split_name_and_version(name_and_version: &str) -> Result<(&str, Option<&str>)> {
    if let Some(idx) = name_and_version.rfind('@') {
        let name = &name_and_version[..idx];
        let version = &name_and_version[idx + 1..];
        if name.is_empty() || version.is_empty() {
            return Err(anyhow!("invalid package specifier `{name_and_version}`"));
        }
        Ok((name, Some(version)))
    } else {
        Ok((name_and_version, None))
    }
}

fn load_file_module(specifier: &ModuleSpecifier) -> Result<Vec<u8>> {
    let path = specifier
        .to_file_path()
        .map_err(|_| anyhow!("provided module specifier `{specifier}` is not a file URL"))?;
    Ok(std::fs::read(path)?)
}

fn load_http_module(specifier: &ModuleSpecifier) -> Result<Vec<u8>> {
    let response = reqwest::blocking::get(specifier.as_str())?;
    let response = response.error_for_status()?;
    Ok(response.bytes()?.to_vec())
}

fn module_type_for_specifier(
    specifier: &ModuleSpecifier,
    requested_module_type: &RequestedModuleType,
) -> ModuleType {
    let path = specifier.path().to_ascii_lowercase();
    if path.ends_with(".json") {
        return ModuleType::Json;
    }
    match requested_module_type {
        RequestedModuleType::Json => ModuleType::Json,
        RequestedModuleType::Other(ty) => ModuleType::Other(ty.clone()),
        RequestedModuleType::None => ModuleType::JavaScript,
    }
}

#[cfg(test)]
mod tests {
    use super::map_jsr_to_npm_specifier;

    #[test]
    fn maps_jsr_package_to_jsr_npm_scope() {
        let out =
            map_jsr_to_npm_specifier("jsr:@std/encoding@1.0.0/base64").expect("map jsr to npm");
        assert_eq!(out, "npm:@jsr/std__encoding@1.0.0/base64");
    }
}
