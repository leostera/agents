use std::collections::BTreeMap;
use std::path::Path;

use walkdir::WalkDir;

#[derive(Debug, Clone)]
pub(super) struct EvalCrate {
    pub package_name: String,
    pub crate_ident: String,
}

pub(super) fn discover_eval_crates(workspace_root: &Path) -> Vec<EvalCrate> {
    let mut crates = BTreeMap::<String, EvalCrate>::new();

    for path in WalkDir::new(workspace_root.join("crates"))
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| entry.into_path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "rs"))
    {
        let relative = path
            .strip_prefix(workspace_root.join("crates"))
            .expect("eval file under crates/");
        let mut components = relative.components();
        let package_name = components
            .next()
            .expect("crate component")
            .as_os_str()
            .to_string_lossy()
            .to_string();
        let Some(evals_component) = components.next() else {
            continue;
        };
        if evals_component.as_os_str() != "evals" {
            continue;
        }

        crates
            .entry(package_name.clone())
            .or_insert_with(|| EvalCrate {
                crate_ident: package_name.replace('-', "_"),
                package_name,
            });
    }

    crates.into_values().collect()
}
