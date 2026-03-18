use std::collections::{BTreeMap, BTreeSet};

use deno_core::{
    ModuleLoadOptions, ModuleLoadReferrer, ModuleLoadResponse, ModuleLoader, ModuleSource,
    ModuleSourceCode, ModuleSpecifier, ModuleType, ResolutionKind,
};
use deno_error::JsErrorBox;

use crate::providers::Package;

const PACKAGE_HOST: &str = "packages";

pub(crate) struct CodeModeModuleLoader {
    packages: BTreeMap<String, String>,
    allowed_imports: BTreeSet<String>,
}

impl CodeModeModuleLoader {
    pub(crate) fn new(packages: Vec<Package>, allowed_imports: Vec<String>) -> Self {
        Self {
            packages: packages
                .into_iter()
                .map(|package| (package.name, package.code))
                .collect(),
            allowed_imports: allowed_imports.into_iter().collect(),
        }
    }
}

impl ModuleLoader for CodeModeModuleLoader {
    fn resolve(
        &self,
        specifier: &str,
        _referrer: &str,
        _kind: ResolutionKind,
    ) -> Result<ModuleSpecifier, JsErrorBox> {
        if let Ok(specifier) = ModuleSpecifier::parse(specifier) {
            return Ok(specifier);
        }

        if !self.allowed_imports.is_empty() && !self.allowed_imports.contains(specifier) {
            return Err(JsErrorBox::generic(format!(
                "import `{specifier}` is not allowed by this request"
            )));
        }

        if !self.packages.contains_key(specifier) {
            return Err(JsErrorBox::generic(format!(
                "package not found for import `{specifier}`"
            )));
        }

        ModuleSpecifier::parse(&format!("codemode://{PACKAGE_HOST}/{specifier}")).map_err(|error| {
            JsErrorBox::generic(format!("failed to resolve import `{specifier}`: {error}"))
        })
    }

    fn load(
        &self,
        module_specifier: &ModuleSpecifier,
        _maybe_referrer: Option<&ModuleLoadReferrer>,
        _options: ModuleLoadOptions,
    ) -> ModuleLoadResponse {
        if module_specifier.scheme() != "codemode"
            || module_specifier.host_str() != Some(PACKAGE_HOST)
        {
            return ModuleLoadResponse::Sync(Err(JsErrorBox::generic(format!(
                "unsupported module specifier `{module_specifier}`"
            ))));
        }

        let package_name = module_specifier.path().trim_start_matches('/');
        let Some(code) = self.packages.get(package_name) else {
            return ModuleLoadResponse::Sync(Err(JsErrorBox::generic(format!(
                "package not found for `{package_name}`"
            ))));
        };

        ModuleLoadResponse::Sync(Ok(ModuleSource::new(
            ModuleType::JavaScript,
            ModuleSourceCode::String(code.clone().into()),
            module_specifier,
            None,
        )))
    }
}
