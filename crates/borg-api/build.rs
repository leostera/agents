use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let manifest_dir =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR must be set"));
    let workspace_root = manifest_dir
        .join("../..")
        .canonicalize()
        .expect("failed to resolve workspace root");
    let dist_dir = workspace_root.join("packages/borg-app/dist");

    if is_release_profile() {
        run_web_build(&workspace_root);
    }

    let dist_dir = dist_dir
        .canonicalize()
        .unwrap_or_else(|_| panic!("dashboard dist not found: {}", dist_dir.display()));

    println!("cargo:rerun-if-changed={}", dist_dir.display());
    println!("cargo:rerun-if-changed=build.rs");

    let index_path = dist_dir.join("index.html");
    if !index_path.exists() {
        panic!(
            "dashboard index.html not found at {}. Run `bun run build:web`.",
            index_path.display()
        );
    }

    let index_html = fs::read_to_string(&index_path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", index_path.display()));
    for asset_ref in find_asset_references(&index_html) {
        let relative = asset_ref.strip_prefix('/').unwrap_or(asset_ref.as_str());
        let referenced_path = dist_dir.join(relative);
        if !referenced_path.exists() {
            panic!(
                "dashboard build references missing asset `{}` (expected at {}). Run `bun run build:web`.",
                asset_ref,
                referenced_path.display()
            );
        }
    }

    println!(
        "cargo:rustc-env=BORG_DASHBOARD_DIST_ABS={}",
        dist_dir.display()
    );
}

fn is_release_profile() -> bool {
    env::var("PROFILE")
        .map(|value| value.eq_ignore_ascii_case("release"))
        .unwrap_or(false)
}

fn run_web_build(workspace_root: &PathBuf) {
    let output = Command::new("bun")
        .args(["run", "build:web"])
        .current_dir(workspace_root)
        .output()
        .unwrap_or_else(|err| panic!("failed to execute `bun run build:web`: {err}"));

    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        panic!(
            "`bun run build:web` failed in release build.\nstdout:\n{}\nstderr:\n{}",
            stdout, stderr
        );
    }
}

fn find_asset_references(index_html: &str) -> Vec<String> {
    let mut refs = Vec::new();
    for quote in ['"', '\''] {
        let needle = format!("{quote}/assets/");
        let mut cursor = index_html;
        while let Some(start) = cursor.find(&needle) {
            let candidate = &cursor[start + 1..];
            if let Some(end) = candidate.find(quote) {
                let value = &candidate[..end];
                if value.starts_with("/assets/") {
                    refs.push(value.to_string());
                }
                cursor = &candidate[end + 1..];
            } else {
                break;
            }
        }
    }
    refs.sort();
    refs.dedup();
    refs
}
