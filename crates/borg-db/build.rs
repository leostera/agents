use std::{env, fs, path::Path};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=migrations");

    let manifest_dir =
        env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR must be set by cargo");
    let migrations_dir = Path::new(&manifest_dir).join("migrations");
    emit_watch_entries(&migrations_dir);
}

fn emit_watch_entries(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            emit_watch_entries(&path);
            continue;
        }
        if path.extension().and_then(|ext| ext.to_str()) == Some("sql") {
            println!("cargo:rerun-if-changed={}", path.display());
        }
    }
}
